//! Analyse du bridage sur des séries synthétiques, et test de charge réel de 2 s.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use pccheck_inventory::{analyse_throttling, run_cpu_stress, ThrottleLevel};

/// Débit stable avec un léger bruit déterministe (± 2 %).
fn steady(len: usize, rate: f64) -> Vec<f64> {
    (0..len)
        .map(|i| rate * (1.0 + 0.02 * ((i * 7 % 5) as f64 - 2.0) / 2.0))
        .collect()
}

#[test]
fn steady_series_is_not_throttled() {
    let a = analyse_throttling(&steady(300, 1000.0));
    assert_eq!(a.level, ThrottleLevel::None);
    assert!((a.baseline - 1000.0).abs() < 25.0);
    assert!(a.drop_pct.abs() < 3.0);
}

#[test]
fn drop_right_after_turbo_is_power_limit_not_throttling() {
    // 30 s de turbo puis 70 % : passage normal à la puissance soutenue d'un portable (PL2 → PL1).
    let mut rates = steady(30, 1000.0);
    rates.extend(steady(270, 700.0));
    let a = analyse_throttling(&rates);
    assert_eq!(a.level, ThrottleLevel::None);
    assert!(
        (a.baseline - 700.0).abs() < 20.0,
        "référence prise après le turbo"
    );
}

#[test]
fn sustained_drop_later_in_the_run() {
    // Stable 2 min après le turbo, puis chute à 70 % jusqu'à la fin : bridage thermique.
    let mut rates = steady(30, 1000.0);
    rates.extend(steady(120, 900.0));
    rates.extend(steady(150, 630.0));
    let a = analyse_throttling(&rates);
    assert_eq!(a.level, ThrottleLevel::Sustained);
    assert!((a.drop_pct - 30.0).abs() < 3.0, "baisse {}", a.drop_pct);
    assert!((a.final_rate - 630.0).abs() < 20.0);
}

#[test]
fn short_dip_is_brief() {
    let mut rates = steady(100, 1000.0);
    for r in &mut rates[40..45] {
        *r = 750.0;
    }
    let a = analyse_throttling(&rates);
    assert_eq!(a.level, ThrottleLevel::Brief);
    assert!(a.drop_pct < 15.0);
}

#[test]
fn single_second_dip_is_noise() {
    let mut rates = steady(100, 1000.0);
    rates[50] = 500.0;
    assert_eq!(analyse_throttling(&rates).level, ThrottleLevel::None);
}

#[test]
fn long_recovered_dip_is_still_brief() {
    let mut rates = steady(120, 1000.0);
    for r in &mut rates[60..85] {
        *r = 800.0;
    }
    assert_eq!(analyse_throttling(&rates).level, ThrottleLevel::Brief);
}

#[test]
fn drop_exactly_at_threshold_is_sustained() {
    let mut rates = vec![1000.0; 20];
    rates.extend(vec![850.0; 40]);
    let a = analyse_throttling(&rates);
    assert_eq!(a.level, ThrottleLevel::Sustained);
    assert!((a.drop_pct - 15.0).abs() < 1e-9);
}

#[test]
fn degenerate_series() {
    let empty = analyse_throttling(&[]);
    assert_eq!(empty.level, ThrottleLevel::None);
    assert_eq!(empty.baseline, 0.0);
    let zeros = analyse_throttling(&[0.0, 0.0, 0.0]);
    assert_eq!(zeros.level, ThrottleLevel::None);
    let two = analyse_throttling(&[1000.0, 990.0]);
    assert_eq!(two.level, ThrottleLevel::None);
}

#[test]
fn real_stress_for_two_seconds() {
    let cancel = AtomicBool::new(false);
    let mut seen = 0;
    let r = run_cpu_stress(Duration::from_secs(2), &cancel, |s| {
        seen += 1;
        assert!(s.iterations_per_sec > 0.0);
    });
    assert!(!r.cancelled);
    assert!(r.threads >= 1);
    assert!(
        (1..=3).contains(&r.samples.len()),
        "{} échantillons",
        r.samples.len()
    );
    assert_eq!(seen, r.samples.len());
    assert!(r.total_iterations > 0);
    assert_eq!(r.computation_errors, 0);
    assert!(r.duration_ms >= 1_900 && r.duration_ms < 10_000);
    assert!(r
        .samples
        .windows(2)
        .all(|w| w[0].elapsed_ms < w[1].elapsed_ms));
    if let Some(t) = r.max_celsius {
        assert!(t > 5.0 && t < 150.0);
    }
    // Le résultat doit passer en JSON (envoyé à l'interface).
    serde_json::to_string(&r).expect("sérialisable");
}

#[test]
fn stress_stops_at_once_when_cancelled() {
    let cancel = AtomicBool::new(true);
    let r = run_cpu_stress(Duration::from_secs(60), &cancel, |_| {});
    assert!(r.cancelled);
    assert!(r.samples.is_empty());
    assert!(r.duration_ms < 5_000);
}
