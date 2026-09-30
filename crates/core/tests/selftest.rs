//! Lecture de l'état des auto-tests SMART sur des sorties `smartctl -c -l selftest -j` reconstruites.

use pccheck_core::{parse_self_test_status, SmartctlError};

fn status(name: &str) -> Result<pccheck_core::SelfTestStatus, SmartctlError> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    parse_self_test_status(&std::fs::read_to_string(&path).unwrap())
}

#[test]
fn ata_test_in_progress_reports_remaining_and_durations() {
    let s = status("selftest_ata_running.json").unwrap();
    assert_eq!(s.supported, Some(true));
    assert!(s.running);
    assert_eq!(s.remaining_pct, Some(70));
    assert_eq!((s.short_minutes, s.long_minutes), (Some(2), Some(85)));
    assert_eq!(s.history.len(), 1);
    assert_eq!(s.history[0].passed, Some(true));
    assert_eq!(s.history[0].power_on_hours, Some(3860));
}

#[test]
fn ata_history_keeps_failures_and_aborts_apart() {
    let s = status("selftest_ata_failed.json").unwrap();
    assert!(
        !s.running,
        "0x79 : terminé (échec de lecture), pas en cours"
    );
    assert_eq!(s.remaining_pct, None);
    let passed: Vec<_> = s.history.iter().map(|r| r.passed).collect();
    // Échec de lecture, réussite, interruption (pas de verdict).
    assert_eq!(passed, [Some(false), Some(true), None]);
    assert_eq!(s.history[0].kind, "Extended offline");
}

#[test]
fn nvme_test_in_progress_converts_completion_to_remaining() {
    let s = status("selftest_nvme_running.json").unwrap();
    assert_eq!(s.supported, Some(true));
    assert!(s.running);
    assert_eq!(s.remaining_pct, Some(65), "35 % fait, donc 65 % restant");
    assert_eq!(s.long_minutes, None, "NVMe ne donne pas de durée estimée");
}

#[test]
fn nvme_history_skips_unused_entries_and_classifies_results() {
    let s = status("selftest_nvme_history.json").unwrap();
    assert!(!s.running);
    let passed: Vec<_> = s.history.iter().map(|r| r.passed).collect();
    assert_eq!(passed, [Some(false), None, Some(true)]);
}

#[test]
fn command_failure_is_an_error() {
    let json = r#"{"json_format_version":[1,0],"smartctl":{"exit_status":2,"messages":[{"string":"Unknown USB bridge"}]}}"#;
    assert!(matches!(
        parse_self_test_status(json),
        Err(SmartctlError::CommandFailed { exit_status: 2, .. })
    ));
}
