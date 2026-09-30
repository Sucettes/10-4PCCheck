//! Test de charge CPU : un fil de calcul par processeur logique, un échantillon par seconde.
//!
//! Le bridage thermique se voit sans pilote : quand le processeur chauffe, il baisse sa
//! fréquence, donc le débit de calcul (itérations par seconde) baisse à travail identique.
//! La température n'est qu'un complément, quand un capteur est lisible.
//!
//! Chaque itération calcule un résultat déterministe, comparé à une référence : un écart
//! signale un processeur instable (surcadencé, sous-alimenté ou défectueux).

use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Durée de la référence : médiane de `BASELINE_SECONDS` secondes, prises après le turbo.
pub const BASELINE_SECONDS: usize = 20;
/// Secondes ignorées au début pour la référence : pendant ~30 s, le processeur tourne au-dessus de
/// sa puissance soutenue (Intel PL2 / tau, AMD PPT). La baisse qui suit est normale, pas un bridage
/// thermique ; la prendre pour référence ferait passer tout portable pour « bridé ».
pub const TURBO_SECONDS: usize = 30;
/// Baisse de débit, en %, à partir de laquelle on parle de bridage.
pub const THROTTLE_DROP_PCT: f64 = 15.0;
/// Un creux plus long que ceci n'est plus « bref ».
pub const BRIEF_MAX_SECONDS: usize = 10;
/// Échantillons consécutifs sous le seuil pour compter un creux : une seconde isolée vient
/// plus souvent d'une tâche de fond de l'OS que du processeur.
pub const MIN_DIP_SAMPLES: usize = 2;

/// Travail d'une itération : assez court pour vérifier souvent l'arrêt (quelques dizaines de µs).
const INNER_STEPS: u32 = 2048;
/// Chaînes de calcul indépendantes : le processeur les exécute en parallèle, ce qui charge
/// davantage ses unités qu'une seule chaîne dépendante.
const LANES: usize = 4;
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const SAMPLE_PERIOD: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StressSample {
    /// Temps écoulé depuis le début du test, en ms.
    pub elapsed_ms: u64,
    /// Débit total de tous les fils pendant la dernière période.
    pub iterations_per_sec: f64,
    /// Température maximale des capteurs lisibles, si disponible.
    pub max_celsius: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThrottleLevel {
    None,
    Brief,
    Sustained,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThrottleAnalysis {
    /// Débit de référence (médiane des `BASELINE_SECONDS` premières secondes).
    pub baseline: f64,
    /// Débit final (médiane du dernier tiers).
    pub final_rate: f64,
    /// Baisse du débit final par rapport à la référence, en %. Négatif si le débit a monté.
    pub drop_pct: f64,
    pub level: ThrottleLevel,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StressResult {
    pub threads: u32,
    pub duration_ms: u64,
    pub total_iterations: u64,
    /// Résultats de calcul différents de la référence. Doit valoir 0.
    pub computation_errors: u64,
    pub cancelled: bool,
    pub samples: Vec<StressSample>,
    pub max_celsius: Option<f64>,
    pub throttling: ThrottleAnalysis,
}

/// Compteur d'un fil, seul sur sa ligne de cache : sans cet alignement, les fils écriraient
/// sur la même ligne et se ralentiraient mutuellement (faux partage).
#[repr(align(64))]
#[derive(Default)]
struct PaddedCounter(AtomicU64);

/// Lance le test pendant `duration` (ou jusqu'à `cancel`), et appelle `on_sample` chaque
/// seconde depuis le fil appelant. Bloquant : à appeler hors du fil de l'interface.
pub fn run_cpu_stress(
    duration: Duration,
    cancel: &AtomicBool,
    mut on_sample: impl FnMut(&StressSample),
) -> StressResult {
    let wanted = thread::available_parallelism().map_or(1, |n| n.get());
    let expected = work_unit(SEED);
    let counters: Vec<PaddedCounter> = (0..wanted).map(|_| PaddedCounter::default()).collect();
    let errors = AtomicU64::new(0);
    let stop = AtomicBool::new(false);
    // Température partagée par le fil capteur : bits d'un f64, NaN = inconnue.
    let temperature = AtomicU64::new(f64::NAN.to_bits());

    let start = Instant::now();
    let mut samples = Vec::new();
    let mut cancelled = false;
    let mut spawned = 0usize;

    thread::scope(|s| {
        for counter in &counters {
            let (stop, errors) = (&stop, &errors);
            let r = thread::Builder::new()
                .name("pccheck-stress".into())
                .spawn_scoped(s, move || worker(counter, stop, cancel, errors, expected));
            if r.is_ok() {
                spawned += 1;
            }
        }
        let (stop_ref, temp_ref) = (&stop, &temperature);
        // Capteur dans son propre fil : une requête WMI lente ne retarde pas les échantillons.
        let _ = thread::Builder::new()
            .name("pccheck-stress-temp".into())
            .spawn_scoped(s, move || sensor_loop(stop_ref, temp_ref));

        let end = start + duration;
        let mut next = start + SAMPLE_PERIOD;
        let (mut last_t, mut last_total) = (start, 0u64);
        loop {
            let now = Instant::now();
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            let finished = now >= end;
            // Dernière période partielle gardée si elle dure au moins une demi-seconde.
            let partial_ok = finished && now.duration_since(last_t) >= SAMPLE_PERIOD / 2;
            if now >= next || partial_ok {
                let total = sum(&counters);
                let secs = now.duration_since(last_t).as_secs_f64();
                let sample = StressSample {
                    elapsed_ms: millis(now.duration_since(start)),
                    iterations_per_sec: if secs > 0.0 {
                        total.saturating_sub(last_total) as f64 / secs
                    } else {
                        0.0
                    },
                    max_celsius: read_temperature(&temperature),
                };
                on_sample(&sample);
                samples.push(sample);
                (last_t, last_total) = (now, total);
                next += SAMPLE_PERIOD;
            }
            if finished {
                break;
            }
            let wait = next.min(end).saturating_duration_since(Instant::now());
            thread::sleep(wait.min(POLL));
        }
        stop.store(true, Ordering::Relaxed);
    });

    let max_celsius = samples
        .iter()
        .filter_map(|s| s.max_celsius)
        .reduce(f64::max);
    let rates: Vec<f64> = samples.iter().map(|s| s.iterations_per_sec).collect();
    StressResult {
        threads: u32::try_from(spawned).unwrap_or(u32::MAX),
        duration_ms: millis(start.elapsed()),
        total_iterations: sum(&counters),
        computation_errors: errors.load(Ordering::Relaxed),
        cancelled,
        max_celsius,
        throttling: analyse_throttling(&rates),
        samples,
    }
}

fn worker(
    counter: &PaddedCounter,
    stop: &AtomicBool,
    cancel: &AtomicBool,
    errors: &AtomicU64,
    expected: u64,
) {
    while !stop.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
        // `black_box` empêche le compilateur de calculer le résultat une fois pour toutes.
        if black_box(work_unit(black_box(SEED))) != expected {
            errors.fetch_add(1, Ordering::Relaxed);
        }
        counter.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn sensor_loop(stop: &AtomicBool, temperature: &AtomicU64) {
    let mut probe = crate::platform::temperature_probe();
    // Aucun capteur à la première lecture : inutile d'insister pendant tout le test.
    let Some(first) = probe() else {
        return;
    };
    temperature.store(first.to_bits(), Ordering::Relaxed);
    while !stop.load(Ordering::Relaxed) {
        let deadline = Instant::now() + SAMPLE_PERIOD;
        let value = probe().unwrap_or(f64::NAN);
        temperature.store(value.to_bits(), Ordering::Relaxed);
        while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
            thread::sleep(POLL);
        }
    }
}

fn read_temperature(t: &AtomicU64) -> Option<f64> {
    let v = f64::from_bits(t.load(Ordering::Relaxed));
    (!v.is_nan()).then_some(v)
}

fn sum(counters: &[PaddedCounter]) -> u64 {
    counters
        .iter()
        .map(|c| c.0.load(Ordering::Relaxed))
        .fold(0u64, u64::saturating_add)
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Une itération : générateur congruentiel sur entiers et racine carrée sur flottants,
/// sur `LANES` chaînes indépendantes. Déterministe : IEEE 754 impose un arrondi exact pour
/// `+`, `*` et `sqrt`, donc le résultat est identique d'une exécution à l'autre.
fn work_unit(seed: u64) -> u64 {
    let mut x = [0u64; LANES];
    let mut f = [0f64; LANES];
    for (i, (xi, fi)) in x.iter_mut().zip(f.iter_mut()).enumerate() {
        *xi = seed ^ (i as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
        *fi = 1.0 + i as f64 * 0.125;
    }
    for _ in 0..INNER_STEPS {
        for lane in 0..LANES {
            let v = x[lane]
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            x[lane] = v ^ (v >> 29);
            // 53 bits de poids fort ramenés dans [0, 1).
            let r = (x[lane] >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
            f[lane] = (f[lane] * 0.75 + r).sqrt() + 0.25;
        }
    }
    x.iter()
        .zip(f.iter())
        .fold(0u64, |acc, (xi, fi)| acc.rotate_left(7) ^ xi ^ fi.to_bits())
}

/// Analyse le débit (une valeur par seconde) :
/// - référence = médiane de `BASELINE_SECONDS` valeurs prises après `TURBO_SECONDS` (si la série
///   est assez longue, sinon dès le début : tests courts) ;
/// - débit final = médiane du dernier tiers ;
/// - **soutenu** si le débit final est au moins `THROTTLE_DROP_PCT` % sous la référence ;
/// - sinon **bref** s'il existe un creux d'au moins `MIN_DIP_SAMPLES` valeurs consécutives
///   sous ce seuil. Un creux court (< `BRIEF_MAX_SECONDS`) est le cas typique ; un creux plus
///   long, résorbé avant la fin, reste classé bref faute d'être soutenu jusqu'au bout.
///
/// Série vide ou référence nulle : aucune conclusion (`ThrottleLevel::None`, valeurs à 0).
pub fn analyse_throttling(rates: &[f64]) -> ThrottleAnalysis {
    let none = ThrottleAnalysis {
        baseline: 0.0,
        final_rate: 0.0,
        drop_pct: 0.0,
        level: ThrottleLevel::None,
    };
    if rates.is_empty() {
        return none;
    }
    // Au moins une minute après la fenêtre de référence pour juger la fin : sinon on garde le début.
    let skip = if rates.len() >= TURBO_SECONDS + BASELINE_SECONDS + 60 {
        TURBO_SECONDS
    } else {
        0
    };
    let baseline = median(&rates[skip..rates.len().min(skip + BASELINE_SECONDS)]);
    let tail = rates.len().div_ceil(3);
    let final_rate = median(&rates[rates.len() - tail..]);
    if baseline <= 0.0 {
        return ThrottleAnalysis { final_rate, ..none };
    }
    let drop_pct = (baseline - final_rate) / baseline * 100.0;
    let threshold = baseline * (1.0 - THROTTLE_DROP_PCT / 100.0);

    let level = if drop_pct >= THROTTLE_DROP_PCT {
        ThrottleLevel::Sustained
    } else if longest_dip(&rates[skip..], threshold) >= MIN_DIP_SAMPLES {
        ThrottleLevel::Brief
    } else {
        ThrottleLevel::None
    };
    ThrottleAnalysis {
        baseline,
        final_rate,
        drop_pct,
        level,
    }
}

/// Plus longue suite de valeurs consécutives sous `threshold`.
fn longest_dip(rates: &[f64], threshold: f64) -> usize {
    let (mut best, mut run) = (0, 0);
    for r in rates {
        run = if *r <= threshold { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turbo_decay_is_not_throttling() {
        // Portable : 30 s de turbo à 130, puis 100 stable pendant 4 min 30. Pas de bridage thermique.
        let mut rates = vec![130.0; 30];
        rates.extend(std::iter::repeat_n(100.0, 270));
        assert_eq!(analyse_throttling(&rates).level, ThrottleLevel::None);
        // Même portable qui chauffe : la fin tombe à 70 → soutenu.
        let mut hot = rates.clone();
        for r in hot.iter_mut().skip(200) {
            *r = 70.0;
        }
        assert_eq!(analyse_throttling(&hot).level, ThrottleLevel::Sustained);
    }

    #[test]
    fn work_unit_is_deterministic_and_seed_dependent() {
        assert_eq!(work_unit(SEED), work_unit(SEED));
        assert_ne!(work_unit(SEED), work_unit(SEED ^ 1));
    }

    #[test]
    fn median_even_and_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median(&[]), 0.0);
    }

    #[test]
    fn longest_dip_counts_consecutive_values() {
        assert_eq!(longest_dip(&[10.0, 5.0, 5.0, 10.0, 5.0], 8.0), 2);
        assert_eq!(longest_dip(&[10.0, 10.0], 8.0), 0);
    }
}
