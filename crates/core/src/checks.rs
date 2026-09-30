//! Vérifications de cohérence entre compteurs : un disque d'occasion dont les compteurs ne se
//! tiennent pas a peut-être été remis à zéro, ou a eu un usage particulier (serveur, boîtier USB).
//!
//! Heuristiques volontairement prudentes : un avertissement signale une question à poser au
//! vendeur, pas une preuve. Les seuils sont dans des constantes pour être ajustés sur de vrais cas.

use serde::Serialize;

use crate::disk::{DiskInfo, MediaKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckLevel {
    Ok,
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub level: CheckLevel,
    pub text: String,
}

impl Check {
    fn new(level: CheckLevel, text: impl Into<String>) -> Self {
        Check {
            level,
            text: text.into(),
        }
    }
}

/// Au-delà de ces heures, moins de `MIN_WRITES_BYTES` écrits n'est pas plausible pour un disque
/// système : environ 6 semaines allumé.
const RESET_HOURS: u64 = 1_000;
const MIN_WRITES_BYTES: u64 = 100_000_000_000;
/// Session moyenne plus courte que 6 minutes sur au moins 100 démarrages.
const SHORT_SESSION_HOURS: f64 = 0.1;
const SHORT_SESSION_MIN_CYCLES: u64 = 100;
/// Session moyenne de plus de 2 jours : machine rarement éteinte.
const LONG_SESSION_HOURS: f64 = 48.0;
/// Écritures complètes du disque au-delà desquelles une usure de 0 à 1 % surprend
/// (un SSD TLC grand public est prévu pour environ 600 écritures complètes).
const FULL_WRITES_FOR_WEAR: f64 = 150.0;
/// Part des démarrages terminés par une coupure brutale au-delà de laquelle on le signale.
const UNSAFE_SHUTDOWN_RATIO: f64 = 0.25;

/// Compteurs d'erreurs ATA et leur nom au pluriel.
const ATA_ERROR_COUNTERS: [(u8, &str); 5] = [
    (5, "secteur(s) réalloué(s)"),
    (197, "secteur(s) en attente"),
    (198, "secteur(s) non corrigible(s)"),
    (187, "erreur(s) non corrigible(s)"),
    (199, "erreur(s) CRC du câble"),
];

pub fn run(d: &DiskInfo) -> Vec<Check> {
    let mut out = Vec::new();
    error_counters(d, &mut out);
    writes_vs_hours(d, &mut out);
    hours_vs_cycles(d, &mut out);
    wear_vs_writes(d, &mut out);
    unsafe_shutdowns(d, &mut out);
    out
}

fn error_counters(d: &DiskInfo, out: &mut Vec<Check>) {
    if let Some(h) = &d.nvme_health {
        match (h.media_errors, h.critical_warning) {
            (Some(0), Some(0)) => out.push(Check::new(
                CheckLevel::Ok,
                "Aucune erreur de média, aucun avertissement critique.",
            )),
            (Some(m), Some(c)) => out.push(Check::new(
                CheckLevel::Warn,
                format!(
                    "{} erreur(s) de média, avertissement critique {}.",
                    fmt_int(m),
                    if c == 0 { "absent" } else { "présent" }
                ),
            )),
            _ => {}
        }
        return;
    }
    let found: Vec<(u64, &str)> = ATA_ERROR_COUNTERS
        .iter()
        .filter_map(|(id, label)| {
            let a = d.ata_attributes.iter().find(|a| a.id == *id)?;
            Some((a.raw_value, *label))
        })
        .collect();
    if found.is_empty() {
        return;
    }
    let problems: Vec<String> = found
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{} {label}", fmt_int(*n)))
        .collect();
    if problems.is_empty() {
        let labels: Vec<&str> = found.iter().map(|(_, l)| *l).collect();
        out.push(Check::new(
            CheckLevel::Ok,
            format!("0 pour chaque compteur d'erreurs : {}.", labels.join(", ")),
        ));
    } else {
        out.push(Check::new(
            CheckLevel::Warn,
            format!(
                "{}. Ces compteurs ne redescendent jamais.",
                capitalize(&problems.join(", "))
            ),
        ));
    }
}

fn writes_vs_hours(d: &DiskInfo, out: &mut Vec<Check>) {
    let (Some(written), Some(hours)) = (d.bytes_written, d.power_on_hours) else {
        return;
    };
    if hours >= RESET_HOURS && written < MIN_WRITES_BYTES {
        out.push(Check::new(
            CheckLevel::Warn,
            format!(
                "Seulement {} écrits pour {} h d'utilisation : compteurs possiblement remis à zéro, ou disque de stockage peu utilisé.",
                fmt_bytes(written),
                fmt_int(hours)
            ),
        ));
    } else if hours > 0 {
        out.push(Check::new(
            CheckLevel::Ok,
            format!(
                "Écritures ({}) cohérentes avec {} h d'utilisation.",
                fmt_bytes(written),
                fmt_int(hours)
            ),
        ));
    }
}

fn hours_vs_cycles(d: &DiskInfo, out: &mut Vec<Check>) {
    let (Some(hours), Some(cycles)) = (d.power_on_hours, d.power_cycles) else {
        return;
    };
    if cycles == 0 {
        return;
    }
    let session = hours as f64 / cycles as f64;
    if cycles >= SHORT_SESSION_MIN_CYCLES && session < SHORT_SESSION_HOURS {
        out.push(Check::new(
            CheckLevel::Warn,
            format!(
                "{} h pour {} démarrages, soit moins de 6 minutes par démarrage : compteur d'heures possiblement remis à zéro, ou disque souvent branché brièvement (boîtier USB).",
                fmt_int(hours),
                fmt_int(cycles)
            ),
        ));
    } else if session > LONG_SESSION_HOURS {
        out.push(Check::new(
            CheckLevel::Info,
            format!(
                "{} démarrages pour {} h : environ {} jours par démarrage, machine rarement éteinte (serveur, NAS, minage ?).",
                fmt_int(cycles),
                fmt_int(hours),
                (session / 24.0).round() as u64
            ),
        ));
    }
}

fn wear_vs_writes(d: &DiskInfo, out: &mut Vec<Check>) {
    if d.media != MediaKind::Ssd {
        return;
    }
    let (Some(life), Some(written), Some(capacity)) =
        (d.life_remaining_pct, d.bytes_written, d.capacity_bytes)
    else {
        return;
    };
    if capacity == 0 {
        return;
    }
    let full_writes = written as f64 / capacity as f64;
    if life >= 99 && full_writes > FULL_WRITES_FOR_WEAR {
        out.push(Check::new(
            CheckLevel::Warn,
            format!(
                "Usure affichée nulle malgré environ {} écritures complètes du disque : indicateur d'usure douteux.",
                full_writes.round() as u64
            ),
        ));
    }
}

fn unsafe_shutdowns(d: &DiskInfo, out: &mut Vec<Check>) {
    let (Some(h), Some(cycles)) = (&d.nvme_health, d.power_cycles) else {
        return;
    };
    let Some(unsafe_count) = h.unsafe_shutdowns else {
        return;
    };
    if cycles >= 20 && unsafe_count as f64 / cycles as f64 > UNSAFE_SHUTDOWN_RATIO {
        out.push(Check::new(
            CheckLevel::Info,
            format!(
                "{} coupures brutales sur {} démarrages : arrêts forcés fréquents (batterie vide, bouton maintenu).",
                fmt_int(unsafe_count),
                fmt_int(cycles)
            ),
        ));
    }
}

/// Entier avec espace fine insécable comme séparateur de milliers (usage français).
pub fn fmt_int(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202F}');
        }
        out.push(c);
    }
    out
}

/// Octets en unités décimales (Go, To), comme les fabricants de disques.
pub fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let text = if unit == 0 || value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}").replace('.', ",")
    };
    format!("{text} {}", UNITS[unit])
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_number_formatting() {
        assert_eq!(fmt_int(8), "8");
        assert_eq!(fmt_int(3864), "3\u{202F}864");
        assert_eq!(fmt_int(1_234_567), "1\u{202F}234\u{202F}567");
        assert_eq!(fmt_bytes(18_408_744_448_000), "18,4 To");
        assert_eq!(fmt_bytes(512_000_000_000), "512 Go");
        assert_eq!(fmt_bytes(900), "900 o");
    }
}
