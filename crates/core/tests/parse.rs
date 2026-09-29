use pccheck_core::{
    dedupe_disks, parse_disk, parse_scan, DiskEntry, MediaKind, Protocol, ScanDevice, SmartctlError,
};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path} : {e}"))
}

fn fallback(name: &str) -> ScanDevice {
    ScanDevice {
        name: name.into(),
        info_name: name.into(),
        dev_type: String::new(),
        protocol: String::new(),
    }
}

#[test]
fn sata_ssd_is_parsed_like_crystaldiskinfo() {
    let d = parse_disk(&fixture("sata_samsung_860evo.json"), &fallback("/dev/sda")).unwrap();
    assert_eq!(d.model.as_deref(), Some("Samsung SSD 860 EVO 500GB"));
    assert_eq!(d.firmware.as_deref(), Some("RVT04B6Q"));
    assert_eq!(d.capacity_bytes, Some(500_107_862_016));
    assert_eq!(d.protocol, Protocol::Ata);
    assert_eq!(d.media, MediaKind::Ssd);
    assert_eq!(d.smart_passed, Some(true));
    assert_eq!(d.temperature_c, Some(54));
    assert_eq!(d.power_on_hours, Some(3864));
    assert_eq!(d.power_cycles, Some(8));
    assert_eq!(d.ata_attributes.len(), 14);
    let wear = d.ata_attributes.iter().find(|a| a.id == 177).unwrap();
    assert_eq!(
        (wear.value, wear.raw_value, wear.prefailure),
        (97, 38, true)
    );
    let lbas = d.ata_attributes.iter().find(|a| a.id == 241).unwrap();
    assert_eq!(lbas.raw_value, 13_994_098_713);
    assert!(d.warnings.is_empty());
    assert!(d.nvme_health.is_none());
    assert_eq!(d.life_remaining_pct, Some(97), "attribut 177 de Samsung");
}

#[test]
fn nvme_uses_total_capacity_and_health_log() {
    let d = parse_disk(&fixture("nvme_generic.json"), &fallback("/dev/nvme0")).unwrap();
    assert_eq!(d.protocol, Protocol::Nvme);
    assert_eq!(d.media, MediaKind::Ssd);
    assert_eq!(d.capacity_bytes, Some(1_000_204_886_016));
    let h = d.nvme_health.as_ref().unwrap();
    assert_eq!(h.percentage_used, Some(2));
    assert_eq!(h.bytes_written(), Some(9_876_543 * 512_000));
    assert!(d.ata_attributes.is_empty());
    assert_eq!(d.life_remaining_pct, Some(98), "100 - percentage_used");
}

#[test]
fn failing_hdd_keeps_data_and_lists_warnings() {
    let d = parse_disk(&fixture("hdd_failing.json"), &fallback("/dev/sdb")).unwrap();
    assert_eq!(d.media, MediaKind::Hdd { rpm: 7200 });
    assert_eq!(d.smart_passed, Some(false));
    assert_eq!(d.exit_status, 88);
    // 88 = bits 3, 4 et 6 : DISK FAILING, attribut critique sous le seuil, erreurs au journal.
    assert_eq!(d.warnings.len(), 3);
    assert!(d.warnings[0].contains("DISK FAILING"));
    let realloc = d.ata_attributes.iter().find(|a| a.id == 5).unwrap();
    assert_eq!(realloc.when_failed.as_deref(), Some("now"));
    let hours = d.ata_attributes.iter().find(|a| a.id == 9).unwrap();
    assert_eq!(
        hours.when_failed, None,
        "une chaîne vide ne doit pas compter comme un échec"
    );
    assert_eq!(
        d.life_remaining_pct, None,
        "pas d'usure mesurable sur un disque dur"
    );
}

#[test]
fn unsupported_usb_bridge_is_an_explicit_error() {
    match parse_disk(
        &fixture("usb_bridge_unsupported.json"),
        &fallback("/dev/sdc"),
    ) {
        Err(SmartctlError::CommandFailed {
            exit_status,
            messages,
        }) => {
            assert_eq!(exit_status, 1);
            assert!(messages[0].contains("Unknown USB bridge"));
        }
        other => panic!("attendu CommandFailed, obtenu {other:?}"),
    }
}

#[test]
fn scan_lists_devices_with_their_type() {
    let devices = parse_scan(&fixture("scan.json")).unwrap();
    assert_eq!(devices.len(), 3);
    assert_eq!(devices[2].dev_type, "sntrealtek");
    assert_eq!(devices[0].info_name, "/dev/sda [SAT]");
}

#[test]
fn scan_without_devices_is_empty_not_an_error() {
    let devices =
        parse_scan(r#"{"json_format_version":[1,0],"smartctl":{"exit_status":0}}"#).unwrap();
    assert!(devices.is_empty());
}

#[test]
fn fallback_device_is_used_when_json_has_none() {
    let d = parse_disk(r#"{"json_format_version":[1,0]}"#, &fallback("/dev/sdz")).unwrap();
    assert_eq!(d.device.name, "/dev/sdz");
    assert_eq!(d.media, MediaKind::Unknown);
}

#[test]
fn invalid_inputs_are_rejected() {
    assert!(matches!(
        parse_scan(""),
        Err(SmartctlError::InvalidJson { .. })
    ));
    assert!(matches!(
        parse_scan("pas du json"),
        Err(SmartctlError::InvalidJson { .. })
    ));
    assert!(matches!(
        parse_scan("{}"),
        Err(SmartctlError::InvalidJson { .. })
    ));
    assert!(matches!(
        parse_scan(r#"{"json_format_version":[2,0]}"#),
        Err(SmartctlError::UnsupportedJsonVersion(v)) if v == vec![2, 0]
    ));
}

#[test]
fn raw_value_beyond_u64_is_rejected_not_truncated() {
    let json = r#"{"json_format_version":[1,0],"ata_smart_attributes":{"table":[
        {"id":241,"name":"X","value":1,"worst":1,"thresh":0,"raw":{"value":18446744073709551616,"string":"x"}}]}}"#;
    assert!(matches!(
        parse_disk(json, &fallback("/dev/sda")),
        Err(SmartctlError::InvalidJson { .. })
    ));
}

#[test]
fn real_virtio_scan_finds_no_disk() {
    assert!(parse_scan(&fixture("real_scan_virtio_empty.json"))
        .unwrap()
        .is_empty());
}

#[test]
fn real_virtio_disk_without_smart_is_a_command_failure() {
    match parse_disk(
        &fixture("real_virtio_unknown_type.json"),
        &fallback("/dev/vda"),
    ) {
        Err(SmartctlError::CommandFailed {
            exit_status: 1,
            messages,
        }) => {
            assert_eq!(messages, vec!["/dev/vda: Unable to detect device type"]);
        }
        other => panic!("attendu CommandFailed, obtenu {other:?}"),
    }
}

#[test]
fn zero_temperature_means_not_reported() {
    let json = r#"{"json_format_version":[1,0],"device":{"name":"/dev/sda","type":"scsi","protocol":"SCSI"},
        "model_name":"Msft Virtual Disk","temperature":{"current":0},"smartctl":{"exit_status":4}}"#;
    let d = parse_disk(json, &fallback("/dev/sda")).unwrap();
    assert_eq!(d.temperature_c, None);
    assert_eq!(d.protocol, Protocol::Scsi);
    assert_eq!(
        d.warnings.len(),
        1,
        "le bit 2 du code de sortie reste signalé"
    );
}

#[test]
fn negative_temperature_is_kept() {
    let json = r#"{"json_format_version":[1,0],"temperature":{"current":-5}}"#;
    assert_eq!(
        parse_disk(json, &fallback("/dev/sda"))
            .unwrap()
            .temperature_c,
        Some(-5)
    );
}

/// Entrée lue depuis un fichier de test, placée sur le chemin `name`.
fn entry(name: &str, fixture_name: &str) -> DiskEntry {
    let mut info = parse_disk(&fixture(fixture_name), &fallback(name)).unwrap();
    info.device = fallback(name);
    DiskEntry {
        device: fallback(name),
        info: Some(info),
        error: None,
    }
}

fn names(entries: &[DiskEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.device.name.as_str()).collect()
}

#[test]
fn same_disk_via_intel_rst_is_listed_once_on_standard_path() {
    // Cas réel : un disque derrière le pilote Intel RST vu en /dev/sda et en /dev/csmi0,4.
    let entries = vec![
        entry("/dev/csmi0,4", "sata_samsung_860evo.json"),
        entry("/dev/sdc", "nvme_generic.json"),
        entry("/dev/sda", "sata_samsung_860evo.json"),
    ];
    assert_eq!(names(&dedupe_disks(entries)), ["/dev/sda", "/dev/sdc"]);
}

#[test]
fn csmi_duplicate_after_standard_path_is_dropped() {
    let entries = vec![
        entry("/dev/sda", "sata_samsung_860evo.json"),
        entry("/dev/csmi0,4", "sata_samsung_860evo.json"),
    ];
    assert_eq!(names(&dedupe_disks(entries)), ["/dev/sda"]);
}

#[test]
fn disks_without_identity_are_never_merged() {
    let mut a = entry("/dev/sda", "sata_samsung_860evo.json");
    let mut b = entry("/dev/sdb", "sata_samsung_860evo.json");
    a.info.as_mut().unwrap().serial = None;
    b.info.as_mut().unwrap().serial = Some("  ".into());
    let unreadable = |name: &str| DiskEntry {
        device: fallback(name),
        info: None,
        error: None,
    };
    let entries = vec![a, b, unreadable("/dev/sdc"), unreadable("/dev/sdd")];
    assert_eq!(dedupe_disks(entries).len(), 4);
}
