//! Rapports construits à partir des sorties de test des autres crates, puis rendus en HTML et PDF.

use std::path::PathBuf;

use pccheck_android::{assemble, evaluate, manual_checklist, verdict, Date, RawCollection};
use pccheck_assemble::{
    build_disk_report, build_machine_report, build_phone_report, InteractiveEntry, PhoneAnalysis,
    Results,
};
use pccheck_core::{parse_disk, DiskEntry, ScanDevice};
use pccheck_inventory::{
    analyse_gpu_test, GpuSample, GpuTestInput, MachineInventory, RamTestResult, StressResult,
    ThrottleAnalysis, ThrottleLevel,
};
use pccheck_report::{to_html, to_pdf, ChecklistEntry, Level};

fn fixture(crate_name: &str, name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn disk(name: &str, fixture_name: &str) -> DiskEntry {
    let dev = ScanDevice {
        name: name.into(),
        info_name: name.into(),
        dev_type: String::new(),
        protocol: String::new(),
    };
    let mut info = parse_disk(&fixture("core", fixture_name), &dev).unwrap();
    info.device = dev.clone();
    DiskEntry {
        device: dev,
        info: Some(info),
        error: None,
    }
}

fn results_with_disks() -> Results {
    Results {
        disks: vec![
            disk("/dev/sda", "sata_samsung_860evo.json"),
            disk("/dev/sdb", "hdd_failing.json"),
            disk("/dev/nvme0", "nvme_generic.json"),
        ],
        ..Results::default()
    }
}

fn render(report: &mut pccheck_report::Report) -> (String, Vec<u8>) {
    report.update_verdict();
    let html = to_html(report);
    let pdf = to_pdf(report).expect("rendu PDF");
    assert!(pdf.starts_with(b"%PDF"));
    (html, pdf)
}

#[test]
fn failing_hdd_report_is_red() {
    let r = results_with_disks();
    let mut rep = build_disk_report(&r, "/dev/sdb", vec![]).unwrap();
    let (html, _) = render(&mut rep);
    assert_eq!(rep.verdict.level, Level::Bad);
    assert!(html.contains("Secteurs réalloués"));
    let items = &rep.sections[0].items;
    let smart = items
        .iter()
        .find(|i| i.label == "État SMART global")
        .unwrap();
    assert_eq!(smart.level, Level::Bad);
}

#[test]
fn healthy_ssd_at_54_degrees_is_to_watch_only() {
    let r = results_with_disks();
    let mut rep = build_disk_report(&r, "/dev/sda", vec![]).unwrap();
    render(&mut rep);
    // Vie 97 % (bon), 0 secteur réalloué, mais 54 °C : entre 50 et 60 °C → à surveiller.
    assert_eq!(rep.verdict.level, Level::Warn);
    assert_eq!(rep.verdict.bad, 0);
    let table = &rep.sections[0].tables[0];
    assert_eq!(table.rows.len(), 14);
    assert_eq!(table.rows[0][0], "05");
}

#[test]
fn unknown_disk_is_an_error() {
    assert!(build_disk_report(&Results::default(), "/dev/sdz", vec![]).is_err());
}

#[test]
fn phone_report_counts_findings_and_keeps_checklist() {
    let raw = RawCollection {
        getprop: fixture("android", "getprop_samsung.txt"),
        battery: Some(fixture("android", "battery_new.txt")),
        accounts: Some(fixture("android", "account_with.txt")),
        owners: Some(fixture("android", "dpm_none.txt")),
        storage: Some(fixture("android", "df_toybox.txt")),
        ..RawCollection::default()
    };
    let report = assemble("R58N00000XX", &raw);
    let findings = evaluate(&report, Date::new(2026, 9, 29).unwrap());
    let r = Results {
        phone: Some(PhoneAnalysis {
            verdict: verdict(&findings),
            findings,
            report,
            checklist: manual_checklist(),
        }),
        ..Results::default()
    };
    let checklist = vec![ChecklistEntry {
        label: "IMEI vérifié".into(),
        checked: true,
        note: None,
    }];
    let mut rep = build_phone_report(&r, checklist).unwrap();
    let (html, _) = render(&mut rep);
    assert!(
        rep.verdict.warn + rep.verdict.bad > 0,
        "compte Google connecté"
    );
    assert!(
        html.contains("R58N00000XX"),
        "série en clair (outil personnel)"
    );
    assert!(!html.contains("@example.com"), "aucune adresse de compte");
    assert_eq!(rep.checklist.len(), 1);
}

#[test]
fn machine_report_with_real_inventory_renders() {
    // Inventaire réel de la machine de test (lecture seule, sans droits admin) : on ne vérifie que
    // la forme, et le rendu n'est écrit que dans le dossier temporaire.
    let mut r = results_with_disks();
    r.inventory = Some(pccheck_inventory::collect());
    r.stress = Some(StressResult {
        threads: 8,
        duration_ms: 300_000,
        total_iterations: 1,
        computation_errors: 0,
        cancelled: false,
        samples: vec![],
        max_celsius: Some(86.0),
        throttling: ThrottleAnalysis {
            baseline: 100.0,
            final_rate: 97.0,
            drop_pct: 3.0,
            level: ThrottleLevel::None,
        },
    });
    r.ram = Some(RamTestResult {
        requested_bytes: 4 << 30,
        tested_bytes: 4 << 30,
        allocation_limited: false,
        passes: 5,
        errors: 0,
        first_error_offset: None,
        duration_ms: 60_000,
        cancelled: false,
    });
    let interactive = vec![InteractiveEntry {
        label: "Clavier".into(),
        status: "fail".into(),
        note: Some("F9, Pause sans réponse".into()),
    }];
    let mut rep = build_machine_report(&r, &interactive, vec![], vec![]).unwrap();
    let (html, pdf) = render(&mut rep);
    assert!(rep.sections.iter().any(|s| s.id == "processeur"));
    assert!(rep.sections.iter().any(|s| s.id == "tests-interactifs"));
    assert!(
        rep.verdict.bad > 0,
        "le disque en échec rend le verdict rouge"
    );

    let dir = std::env::temp_dir().join("pccheck-assemble-exemple");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("machine.html"), html).unwrap();
    std::fs::write(dir.join("machine.pdf"), pdf).unwrap();
    std::fs::write(dir.join("machine.json"), serde_json::to_vec(&rep).unwrap()).unwrap();
}

#[test]
fn gpu_render_errors_make_the_machine_red() {
    let samples = (1..=60)
        .map(|i| GpuSample {
            t_s: f64::from(i),
            passes_per_s: 50.0,
            temperature_c: Some(70.0),
        })
        .collect();
    let r = Results {
        inventory: Some(MachineInventory::default()),
        gpu: Some(analyse_gpu_test(GpuTestInput {
            renderer: Some("Carte de test".into()),
            samples,
            render_errors: 2,
            checks: 6,
            cancelled: false,
        })),
        ..Results::default()
    };
    let mut rep = build_machine_report(&r, &[], vec![], vec![]).unwrap();
    let (html, _) = render(&mut rep);
    let gpu = rep.sections.iter().find(|s| s.id == "graphique").unwrap();
    let errors = gpu
        .items
        .iter()
        .find(|i| i.label == "Erreurs de rendu")
        .unwrap();
    assert_eq!(errors.level, Level::Bad);
    assert_eq!(rep.verdict.level, Level::Bad);
    assert!(html.contains("Aucun bridage"));
    assert!(html.contains("70 °C"));
}

#[test]
fn machine_report_needs_inventory() {
    let r = Results {
        inventory: None,
        ..Results::default()
    };
    assert!(build_machine_report(&r, &[], vec![], vec![]).is_err());
    let _ = MachineInventory::default();
}
