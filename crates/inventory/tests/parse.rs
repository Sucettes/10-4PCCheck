//! Analyse des sorties texte sur des fixtures inventées (voir `fixtures/README.md`).

use pccheck_inventory::linux_parse::{
    meminfo_bytes, parse_cpuinfo, parse_dmidecode_memory, parse_lspci_mm, parse_os_release,
    parse_power_supply_uevent,
};
use pccheck_inventory::parse::{is_enterprise_managed, merge_batteries, parse_dsregcmd};
use pccheck_inventory::SecurityInfo;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path} : {e}"))
}

#[test]
fn dsregcmd_azure_ad_with_intune() {
    let j = parse_dsregcmd(&fixture("dsregcmd_azure_intune.txt"));
    assert_eq!(j.azure_ad_joined, Some(true));
    assert_eq!(j.domain_joined, Some(false));
    assert_eq!(j.enterprise_joined, Some(false));
    assert_eq!(j.workplace_joined, Some(false));
    assert_eq!(j.tenant_name.as_deref(), Some("Entreprise Exemple inc."));
    assert_eq!(
        j.mdm_url.as_deref(),
        Some("https://enrollment.manage.microsoft.com/enrollmentserver/discovery.svc")
    );
    let s = SecurityInfo {
        device_join: Some(j),
        ..SecurityInfo::default()
    };
    assert!(is_enterprise_managed(&s));
}

#[test]
fn dsregcmd_personal_machine_is_not_managed() {
    let j = parse_dsregcmd(&fixture("dsregcmd_personal.txt"));
    assert_eq!(j.azure_ad_joined, Some(false));
    assert_eq!(j.workplace_joined, Some(true));
    assert_eq!(j.tenant_name, None);
    assert_eq!(j.mdm_url, None);
    let s = SecurityInfo {
        device_join: Some(j),
        ..SecurityInfo::default()
    };
    assert!(!is_enterprise_managed(&s));
}

#[test]
fn dsregcmd_domain_joined() {
    let j = parse_dsregcmd(&fixture("dsregcmd_domain.txt"));
    assert_eq!(j.domain_joined, Some(true));
    assert_eq!(j.domain_name.as_deref(), Some("EXEMPLE"));
    let s = SecurityInfo {
        device_join: Some(j),
        ..SecurityInfo::default()
    };
    assert!(is_enterprise_managed(&s));
}

#[test]
fn dsregcmd_accepts_crlf_and_garbage() {
    let crlf = fixture("dsregcmd_azure_intune.txt").replace('\n', "\r\n");
    assert_eq!(parse_dsregcmd(&crlf).azure_ad_joined, Some(true));
    let empty = parse_dsregcmd("rien d'utile ici");
    assert_eq!(empty.azure_ad_joined, None);
}

#[test]
fn cpuinfo_counts_cores_and_threads() {
    let cpu = parse_cpuinfo(&fixture("cpuinfo_intel_4c8t.txt"));
    assert_eq!(
        cpu.name.as_deref(),
        Some("Intel(R) Core(TM) i5-8250U CPU @ 1.60GHz")
    );
    assert_eq!(cpu.threads, Some(8));
    assert_eq!(cpu.cores, Some(4));
    assert_eq!(cpu.base_mhz, Some(1600));
    // Même résultat avec des fins de ligne Windows.
    let crlf = parse_cpuinfo(&fixture("cpuinfo_intel_4c8t.txt").replace('\n', "\r\n"));
    assert_eq!(crlf, cpu);
}

#[test]
fn meminfo_values_in_bytes() {
    let m = fixture("meminfo.txt");
    assert_eq!(meminfo_bytes(&m, "MemTotal"), Some(7_978_440 * 1024));
    assert_eq!(meminfo_bytes(&m, "MemAvailable"), Some(4_213_876 * 1024));
    assert_eq!(meminfo_bytes(&m, "HugePages_Total"), Some(0));
    assert_eq!(meminfo_bytes(&m, "Absent"), None);
}

#[test]
fn os_release() {
    let os = parse_os_release(&fixture("os_release_ubuntu.txt"));
    assert_eq!(os.name.as_deref(), Some("Ubuntu 24.04.1 LTS"));
    assert_eq!(os.version.as_deref(), Some("24.04"));
}

#[test]
fn dmidecode_skips_empty_slots() {
    let modules = parse_dmidecode_memory(&fixture("dmidecode_memory.txt"));
    assert_eq!(modules.len(), 2);
    let a = &modules[0];
    assert_eq!(a.capacity_bytes, Some(8 << 30));
    assert_eq!(a.speed_mhz, Some(2133), "vitesse configurée prioritaire");
    assert_eq!(a.manufacturer.as_deref(), Some("Samsung"));
    assert_eq!(a.part_number.as_deref(), Some("M471A1K43CB1-CRC"));
    assert_eq!(a.slot.as_deref(), Some("ChannelA-DIMM0"));
    assert_eq!(a.memory_type.as_deref(), Some("DDR4"));
    let b = &modules[1];
    assert_eq!(b.capacity_bytes, Some(4 << 30));
    assert_eq!(b.speed_mhz, Some(2667), "repli sur la vitesse maximale");
    assert_eq!(b.slot.as_deref(), Some("ChannelB-DIMM1"));
}

#[test]
fn lspci_keeps_only_display_controllers() {
    let gpus = parse_lspci_mm(&fixture("lspci_mm.txt"));
    let names: Vec<&str> = gpus.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Intel Corporation UHD Graphics 620",
            "NVIDIA Corporation GP108M [GeForce MX150]"
        ]
    );
}

#[test]
fn battery_in_energy_units() {
    let r = parse_power_supply_uevent(&fixture("uevent_bat_energy.txt")).unwrap();
    assert_eq!(r.design_mwh, Some(57_000));
    assert_eq!(r.full_mwh, Some(41_040));
    assert_eq!(r.health_pct, Some(72.0));
    assert_eq!(r.cycle_count, Some(412));
}

#[test]
fn battery_in_charge_units_converted_with_design_voltage() {
    let r = parse_power_supply_uevent(&fixture("uevent_bat_charge.txt")).unwrap();
    // 5 000 000 µAh × 7,6 V = 38 000 mWh.
    assert_eq!(r.design_mwh, Some(38_000));
    assert_eq!(r.full_mwh, Some(33_820));
    assert_eq!(r.health_pct, Some(89.0));
    assert_eq!(r.cycle_count, Some(0));
}

#[test]
fn peripheral_batteries_and_mains_are_ignored() {
    assert_eq!(
        parse_power_supply_uevent(&fixture("uevent_mouse.txt")),
        None
    );
    assert_eq!(parse_power_supply_uevent(&fixture("uevent_ac.txt")), None);
}

#[test]
fn two_batteries_are_merged() {
    let readings: Vec<_> = ["uevent_bat_energy.txt", "uevent_bat_charge.txt"]
        .iter()
        .filter_map(|f| parse_power_supply_uevent(&fixture(f)))
        .collect();
    let b = merge_batteries(&readings).unwrap();
    assert_eq!(b.count, 2);
    assert_eq!(b.design_capacity_mwh, Some(95_000));
    assert_eq!(b.full_charge_capacity_mwh, Some(74_860));
    assert_eq!(b.cycle_count, Some(412));
    assert_eq!(b.health_pct, Some(78.8));
}
