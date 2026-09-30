//! Vérifications de cohérence sur les sorties smartctl de test, puis sur des cas modifiés.

use pccheck_core::{checks, parse_disk, CheckLevel, DiskInfo, ScanDevice};

fn disk(name: &str) -> DiskInfo {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let json = std::fs::read_to_string(&path).unwrap();
    let dev = ScanDevice {
        name: "/dev/sda".into(),
        info_name: "/dev/sda".into(),
        dev_type: String::new(),
        protocol: String::new(),
    };
    parse_disk(&json, &dev).unwrap()
}

fn levels(d: &DiskInfo) -> Vec<CheckLevel> {
    d.checks.iter().map(|c| c.level).collect()
}

fn warns(d: &DiskInfo) -> Vec<&str> {
    d.checks
        .iter()
        .filter(|c| c.level == CheckLevel::Warn)
        .map(|c| c.text.as_str())
        .collect()
}

#[test]
fn healthy_sata_ssd_is_consistent_and_rarely_turned_off() {
    let d = disk("sata_samsung_860evo.json");
    // Erreurs à 0, écritures cohérentes, 8 démarrages pour 3 864 h.
    assert_eq!(
        levels(&d),
        [CheckLevel::Ok, CheckLevel::Ok, CheckLevel::Info]
    );
    assert!(d.checks[2].text.contains("environ 20 jours par démarrage"));
}

#[test]
fn healthy_nvme_has_no_warning() {
    let d = disk("nvme_generic.json");
    assert!(warns(&d).is_empty(), "{:?}", d.checks);
    assert!(d.checks[0].text.contains("Aucune erreur de média"));
}

#[test]
fn failing_hdd_lists_its_error_counters() {
    let d = disk("hdd_failing.json");
    let w = warns(&d);
    assert_eq!(w.len(), 1);
    assert!(w[0].starts_with("3\u{202F}912 secteur(s) réalloué(s), 57 secteur(s) en attente"));
}

#[test]
fn many_short_sessions_are_suspicious() {
    // Cas réel : SSD en boîtier USB, 27 h pour 1 304 démarrages.
    let mut d = disk("nvme_generic.json");
    d.power_on_hours = Some(27);
    d.power_cycles = Some(1304);
    let checks = checks::run(&d);
    assert!(checks
        .iter()
        .any(|c| c.level == CheckLevel::Warn && c.text.contains("moins de 6 minutes")));
}

#[test]
fn many_hours_with_almost_no_writes_suggests_reset() {
    let mut d = disk("sata_samsung_860evo.json");
    d.power_on_hours = Some(5_000);
    d.bytes_written = Some(50_000_000_000);
    let checks = checks::run(&d);
    assert!(checks
        .iter()
        .any(|c| c.level == CheckLevel::Warn && c.text.contains("remis à zéro")));
}

#[test]
fn zero_wear_after_heavy_writes_is_doubtful() {
    let mut d = disk("nvme_generic.json");
    d.life_remaining_pct = Some(100);
    d.bytes_written = d.capacity_bytes.map(|c| c * 200);
    let checks = checks::run(&d);
    assert!(checks
        .iter()
        .any(|c| c.level == CheckLevel::Warn && c.text.contains("indicateur d'usure douteux")));
}
