//! Données du mode démo de l'interface (app/src/demo/) : un PC, trois disques et un téléphone
//! FICTIFS, passés par le vrai moteur (analyse SMART, âge, échelles, verdict, rapport), pour que
//! les écrans de démonstration et les captures du wiki aient exactement la forme de l'application.
//!
//! Sources : les sorties reconstruites des tests (tests/fixtures), aucune donnée d'une vraie
//! machine. Régénération, depuis la racine du dépôt :
//!   cargo run -q -p pccheck-assemble --example demo_data > app/src/demo/data.json

use std::path::PathBuf;

use pccheck_android::{
    assemble, evaluate, manual_checklist, parse_devices, verdict, Date, RawCollection,
};
use pccheck_assemble::{build_machine_report, InteractiveEntry, PhoneAnalysis, Results};
use pccheck_core::age::estimate_age;
use pccheck_core::pcie::PcieLink;
use pccheck_core::speed::{
    speed_scale_with_link, AccessTime, RateSample, ReadSpeed, SpeedResult, Throughput, WriteSpeed,
    ZoneSpeed,
};
use pccheck_core::{parse_disk, DiskEntry, DiskInfo, ScanDevice};
use pccheck_inventory::{
    analyse_gpu_test, analyse_throttling, BatteryInfo, BiosInfo, BitLockerProtection,
    BitLockerVolume, BoardInfo, ComputerInfo, CpuInfo, DeviceJoin, GpuInfo, GpuSample,
    GpuTestInput, LicenseInfo, MachineInventory, MemoryInfo, MemoryModule, NetworkAdapter,
    NetworkKind, OsInfo, RamTestResult, SecureBootState, SecurityInfo, StressResult, StressSample,
    TpmInfo,
};
use pccheck_report::{ChecklistEntry, ToolVersion};
use serde_json::{json, Value};

/// Année de référence des démonstrations : les âges ne bougent pas d'une régénération à l'autre.
const YEAR: u16 = 2026;
const GIB: u64 = 1 << 30;

fn fixture(crate_name: &str, name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn disk(name: &str, dev_type: &str, protocol: &str, fixture_name: &str) -> DiskEntry {
    let device = ScanDevice {
        name: name.into(),
        info_name: name.into(),
        dev_type: dev_type.into(),
        protocol: protocol.into(),
    };
    let mut info = parse_disk(&fixture("core", fixture_name), &device).expect("fixture");
    info.device = device.clone();
    DiskEntry {
        device,
        info: Some(info),
        error: None,
    }
}

/// Courbe d'écriture : `fast` Mo/s jusqu'à `cache` octets (cache rapide d'un SSD), puis `slow`.
fn curve(total: u64, fast: f64, cache: u64, slow: f64) -> Vec<RateSample> {
    (1..=40)
        .map(|i| {
            let at = total / 40 * i;
            let wobble = [0.0, 12.0, -8.0, 5.0][i as usize % 4];
            RateSample {
                at_bytes: at,
                mbps: if at <= cache { fast } else { slow } + wobble,
            }
        })
        .collect()
}

fn throughput(samples: Vec<RateSample>) -> Throughput {
    let rates = samples.iter().map(|s| s.mbps);
    let mean = rates.clone().sum::<f64>() / samples.len() as f64;
    Throughput {
        mbps: mean,
        min_mbps: rates.clone().fold(f64::INFINITY, f64::min),
        max_mbps: rates.fold(0.0, f64::max),
        samples,
    }
}

struct Speeds {
    zones: [f64; 3],
    access_ms: f64,
    write_bytes: u64,
    write: Vec<RateSample>,
    readback_mbps: f64,
    volume: &'static str,
}

fn speed(info: &DiskInfo, s: Speeds, pcie: Option<&PcieLink>) -> SpeedResult {
    let write = throughput(s.write);
    SpeedResult {
        read: Some(ReadSpeed {
            zones: [0u8, 50, 100]
                .into_iter()
                .zip(s.zones)
                .map(|(position_pct, mbps)| ZoneSpeed { position_pct, mbps })
                .collect(),
            access: Some(AccessTime {
                avg_ms: s.access_ms,
                max_ms: s.access_ms * 3.2,
                samples: 100,
            }),
        }),
        read_error: None,
        write: Some(WriteSpeed {
            volume: s.volume.into(),
            mbps: write.mbps,
            min_mbps: write.min_mbps,
            max_mbps: write.max_mbps,
            bytes: s.write_bytes,
            samples: write.samples,
            readback: Some(throughput(curve(
                s.write_bytes,
                s.readback_mbps,
                s.write_bytes,
                s.readback_mbps,
            ))),
            readback_errors: 0,
            readback_error: None,
        }),
        write_skipped: None,
        cancelled: false,
        scale: speed_scale_with_link(info, None, pcie),
        usb: None,
        pcie: pcie.copied(),
    }
}

fn inventory() -> MachineInventory {
    let s = |v: &str| Some(v.to_string());
    MachineInventory {
        os: Some(OsInfo {
            name: s("Microsoft Windows 11 Professionnel"),
            version: s("10.0.26100"),
            build: s("26100"),
            display_version: s("24H2"),
        }),
        computer: Some(ComputerInfo {
            manufacturer: s("Dell Inc."),
            model: s("Latitude 7490"),
            serial_number: s("DEMO0001"),
        }),
        bios: Some(BiosInfo {
            vendor: s("Dell Inc."),
            version: s("1.38.0"),
            release_date: s("2024-03-12"),
        }),
        board: Some(BoardInfo {
            manufacturer: s("Dell Inc."),
            product: s("0KP0FT"),
        }),
        cpu: Some(CpuInfo {
            name: s("Intel(R) Core(TM) i5-8350U CPU @ 1.70GHz"),
            cores: Some(4),
            threads: Some(8),
            base_mhz: Some(1700),
        }),
        memory: Some(MemoryInfo {
            total_bytes: Some(16 * GIB),
            modules: (0..2)
                .map(|i| MemoryModule {
                    capacity_bytes: Some(8 * GIB),
                    speed_mhz: Some(2400),
                    manufacturer: s("Kingston"),
                    part_number: s("ACR24D4S7S8MB-8"),
                    slot: Some(format!("DIMM {}", i + 1)),
                    memory_type: s("DDR4"),
                })
                .collect(),
        }),
        gpus: vec![GpuInfo {
            name: "Intel(R) UHD Graphics 620".into(),
            driver_version: s("31.0.101.2127"),
            memory_bytes: Some(GIB),
        }],
        network_adapters: vec![
            NetworkAdapter {
                name: "Intel(R) Dual Band Wireless-AC 8265".into(),
                kind: NetworkKind::Wifi,
            },
            NetworkAdapter {
                name: "Intel(R) Ethernet Connection I219-LM".into(),
                kind: NetworkKind::Ethernet,
            },
        ],
        battery: Some(BatteryInfo {
            count: 1,
            design_capacity_mwh: Some(60_000),
            full_charge_capacity_mwh: Some(46_800),
            cycle_count: Some(312),
            health_pct: Some(78.0),
        }),
        security: Some(SecurityInfo {
            windows_license: Some(LicenseInfo {
                activated: true,
                status: Some(1),
                status_label: s("Activée"),
                description: s("Windows(R) Operating System, OEM_DM channel"),
                channel: s("OEM_DM"),
            }),
            bitlocker: Some(vec![BitLockerVolume {
                drive: s("C:"),
                protection: BitLockerProtection::Off,
            }]),
            secure_boot: Some(SecureBootState::Enabled),
            tpm: Some(TpmInfo {
                present: true,
                version: s("2.0"),
                manufacturer: s("NTC"),
            }),
            device_join: Some(DeviceJoin::default()),
            intune_enrolled: Some(false),
            autopilot: None,
            enterprise_managed: false,
        }),
        temperatures: Vec::new(),
        errors: Vec::new(),
    }
}

fn stress() -> StressResult {
    // Débit stable puis léger tassement après le turbo, température qui plafonne à 88 °C.
    let samples: Vec<StressSample> = (1..=300u64)
        .map(|t| StressSample {
            elapsed_ms: t * 1000,
            iterations_per_sec: if t < 30 { 1_000_000.0 } else { 940_000.0 }
                + [0.0, 6_000.0, -4_000.0][t as usize % 3],
            max_celsius: Some((62.0 + t as f64 * 0.4).min(88.0)),
        })
        .collect();
    let rates: Vec<f64> = samples.iter().map(|s| s.iterations_per_sec).collect();
    StressResult {
        threads: 8,
        duration_ms: 300_000,
        total_iterations: 283_000_000,
        computation_errors: 0,
        cancelled: false,
        max_celsius: Some(88.0),
        throttling: analyse_throttling(&rates),
        samples,
    }
}

fn main() {
    let disks = vec![
        disk("/dev/sda", "nvme", "NVMe", "nvme_generic.json"),
        disk("/dev/sdb", "sat", "ATA", "sata_samsung_860evo.json"),
        disk("/dev/sdc", "sat", "ATA", "hdd_failing.json"),
    ];
    let info = |i: usize| disks[i].info.clone().expect("lu");
    let pcie = PcieLink {
        current_gen: 3,
        current_lanes: 4,
        max_gen: Some(3),
        max_lanes: Some(4),
    };

    let mut r = Results {
        disks: disks.clone(),
        inventory: Some(inventory()),
        stress: Some(stress()),
        ram: Some(RamTestResult {
            requested_bytes: 6 * GIB,
            tested_bytes: 6 * GIB,
            allocation_limited: false,
            passes: 5,
            errors: 0,
            first_error_offset: None,
            duration_ms: 64_000,
            cancelled: false,
        }),
        gpu: Some(analyse_gpu_test(GpuTestInput {
            renderer: Some("ANGLE (Intel, Intel(R) UHD Graphics 620 Direct3D11)".into()),
            samples: (1..=120)
                .map(|t| GpuSample {
                    t_s: f64::from(t),
                    passes_per_s: 410.0 + f64::from(t % 5),
                    temperature_c: None,
                })
                .collect(),
            render_errors: 0,
            checks: 12,
            cancelled: false,
        })),
        ..Results::default()
    };
    r.speed.insert(
        "/dev/sda".into(),
        speed(
            &info(0),
            Speeds {
                zones: [3120.0, 3080.0, 3150.0],
                access_ms: 0.08,
                write_bytes: 10 * GIB,
                write: curve(10 * GIB, 2950.0, 4 * GIB, 1150.0),
                readback_mbps: 3050.0,
                volume: r"C:\",
            },
            Some(&pcie),
        ),
    );
    r.speed.insert(
        "/dev/sdb".into(),
        speed(
            &info(1),
            Speeds {
                zones: [535.0, 531.0, 528.0],
                access_ms: 0.11,
                write_bytes: GIB,
                write: curve(GIB, 505.0, GIB, 505.0),
                readback_mbps: 540.0,
                volume: r"D:\",
            },
            None,
        ),
    );
    r.speed.insert(
        "/dev/sdc".into(),
        speed(
            &info(2),
            Speeds {
                zones: [148.0, 121.0, 76.0],
                access_ms: 19.6,
                write_bytes: GIB,
                write: curve(GIB, 139.0, GIB, 139.0),
                readback_mbps: 144.0,
                volume: r"E:\",
            },
            None,
        ),
    );
    r.label_years.insert("/dev/sdc".into(), 2012);

    let interactive = vec![
        InteractiveEntry {
            label: "Clavier".into(),
            status: "pass".into(),
            note: None,
        },
        InteractiveEntry {
            label: "Pixels morts".into(),
            status: "pass".into(),
            note: None,
        },
    ];
    let checklist = vec![ChecklistEntry {
        label: "Aucun mot de passe BIOS / UEFI".into(),
        checked: true,
        note: None,
    }];
    let tools = vec![ToolVersion {
        name: "smartctl".into(),
        version: "smartctl 7.5 2025-04-30 r5714".into(),
    }];
    let mut report =
        build_machine_report(&r, &interactive, checklist, tools).expect("rapport de démonstration");
    report.update_verdict();

    // Téléphone : sortie adb reconstruite (fixtures du crate android), numéro de série inventé.
    let raw = RawCollection {
        getprop: fixture("android", "getprop_samsung.txt"),
        battery: Some(fixture("android", "battery_old.txt")),
        accounts: Some(fixture("android", "account_with.txt")),
        owners: Some(fixture("android", "dpm_none.txt")),
        storage: Some(fixture("android", "df_toybox.txt")),
        ..RawCollection::default()
    };
    let phone_report = assemble("R58N00000XX", &raw);
    let findings = evaluate(
        &phone_report,
        Date::new(i32::from(YEAR), 9, 30).expect("date"),
    );
    let phone = PhoneAnalysis {
        verdict: verdict(&findings),
        findings,
        report: phone_report,
        checklist: manual_checklist(),
    };
    let phone_devices: Vec<Value> = parse_devices(&fixture("android", "devices_mixed.txt"))
        .into_iter()
        .take(1)
        .map(|d| serde_json::to_value(d).expect("appareil"))
        .collect();

    let volumes = [
        (r"C:\", "Système"),
        (r"D:\", "Données"),
        (r"E:\", "Archives"),
    ];
    let disk_views: Vec<Value> = disks
        .iter()
        .zip(volumes)
        .map(|(entry, (path, label))| {
            let mut v = serde_json::to_value(entry).expect("disque");
            v["volumes"] = json!([{ "path": path, "label": label }]);
            v
        })
        .collect();
    let ages: serde_json::Map<String, Value> = disks
        .iter()
        .filter_map(|e| e.info.as_ref())
        .map(|d| {
            let year = r.label_years.get(&d.device.name).copied();
            (
                d.device.name.clone(),
                serde_json::to_value(estimate_age(d, year, YEAR)).expect("âge"),
            )
        })
        .collect();

    // Date fixe en UTC : fichier identique d'une régénération à l'autre, sans le fuseau horaire
    // de la machine qui le génère.
    let mut report_json = serde_json::to_value(&report).expect("rapport");
    report_json["generated_at"] = json!(format!("{YEAR}-09-30T12:00:00+00:00"));
    let bundle = json!({
        "_source": "Généré par crates/assemble/examples/demo_data.rs : données fictives.",
        "disks": disk_views,
        "inventory": r.inventory,
        "stress": r.stress,
        "ram": r.ram,
        "machine_report": report_json,
        "speed": r.speed,
        "age": ages,
        "pcie": { "/dev/sda": pcie },
        "phone_devices": phone_devices,
        "phone": phone,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&bundle).expect("sérialisation")
    );
}
