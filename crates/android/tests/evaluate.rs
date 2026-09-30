//! Assemblage d'un rapport complet et évaluation avec une date fixe.

use pccheck_android::{
    assemble, evaluate, manual_checklist, verdict, CollectStep, Date, Finding, FindingLevel,
    PhoneReport, RawCollection,
};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path} : {e}"))
}

/// Date fixe des tests : tout ce qui dépend du jour reste reproductible.
fn today() -> Date {
    Date::new(2026, 9, 29).unwrap()
}

/// Pixel 7 sain : correctif récent, aucun compte, aucun propriétaire.
fn clean_pixel() -> RawCollection {
    RawCollection {
        getprop: fixture("getprop_pixel.txt"),
        which_su: Some(String::new()),
        battery: Some(fixture("battery_new.txt")),
        cycle_count: Some("210\n".into()),
        charge_full: Some("4020000\n".into()),
        charge_full_design: Some("4270000\n".into()),
        accounts: Some(fixture("account_none.txt")),
        owners: Some(fixture("dpm_none.txt")),
        device_policy: None,
        storage: Some(fixture("df_toybox.txt")),
    }
}

/// Galaxy S10 à problèmes : vieux correctif, comptes connectés, gestion d'entreprise.
fn samsung_with_problems() -> RawCollection {
    RawCollection {
        getprop: fixture("getprop_samsung.txt"),
        which_su: Some(String::new()),
        battery: Some(fixture("battery_samsung_stopped.txt")),
        cycle_count: Some(
            "cat: /sys/class/power_supply/battery/cycle_count: Permission denied\n".into(),
        ),
        charge_full: None,
        charge_full_design: None,
        accounts: Some(fixture("account_with.txt")),
        owners: Some(fixture("dpm_device_owner.txt")),
        device_policy: None,
        storage: Some(fixture("df_toybox.txt")),
    }
}

fn find<'a>(findings: &'a [Finding], label: &str) -> &'a Finding {
    findings
        .iter()
        .find(|f| f.label == label)
        .unwrap_or_else(|| panic!("constat « {label} » absent : {findings:#?}"))
}

fn with_patch(date: &str) -> PhoneReport {
    let mut raw = clean_pixel();
    raw.getprop = raw.getprop.replace(
        "[ro.build.version.security_patch]: [2026-08-05]",
        &format!("[ro.build.version.security_patch]: [{date}]"),
    );
    assemble("2A000000XX000", &raw)
}

#[test]
fn clean_phone_is_green() {
    let report = assemble("2A000000XX000", &clean_pixel());
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.serial, "2A000000XX000");
    assert_eq!(report.security.root.su_found, Some(false));
    let battery = report.battery.as_ref().unwrap();
    assert_eq!(battery.capacity_pct, Some(94));
    assert_eq!(battery.cycle_count, Some(210));

    let f = evaluate(&report, today());
    assert_eq!(find(&f, "Correctif de sécurité").level, FindingLevel::Ok);
    assert!(find(&f, "Correctif de sécurité")
        .detail
        .contains("2026-08-05"));
    assert_eq!(find(&f, "Comptes").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Gestion d'entreprise").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Démarrage vérifié").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Chargeur de démarrage").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Root").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Santé de la batterie").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Capacité de la batterie").level, FindingLevel::Ok);
    assert_eq!(find(&f, "Cycles de charge").level, FindingLevel::Info);
    assert!(find(&f, "Stockage").detail.contains("113 Go"));
    assert!(find(&f, "IMEI").detail.contains("*#06#"));
    assert_eq!(verdict(&f), FindingLevel::Ok);
}

#[test]
fn problem_phone_is_red() {
    let report = assemble("R58N00000XX", &samsung_with_problems());
    let f = evaluate(&report, today());

    let patch = find(&f, "Correctif de sécurité");
    assert_eq!(patch.level, FindingLevel::Bad);
    assert!(patch.detail.contains("44 mois"), "{}", patch.detail);

    assert_eq!(find(&f, "Compte Google connecté").level, FindingLevel::Warn);
    assert!(find(&f, "Compte Google connecté").detail.contains("FRP"));
    assert_eq!(
        find(&f, "Compte Samsung connecté").level,
        FindingLevel::Warn
    );
    let others = find(&f, "Autres comptes");
    assert_eq!(others.level, FindingLevel::Info);
    assert!(others.detail.contains("1 WhatsApp"));

    let mdm = find(&f, "Gestion d'entreprise");
    assert_eq!(mdm.level, FindingLevel::Bad);
    assert!(mdm.detail.contains("com.example.mdm"));

    assert_eq!(
        find(&f, "Valeurs de batterie figées").level,
        FindingLevel::Warn
    );
    assert_eq!(find(&f, "Santé de la batterie").level, FindingLevel::Warn);
    assert_eq!(
        find(&f, "Capacité de la batterie").level,
        FindingLevel::Info
    );
    assert!(find(&f, "Température de la batterie")
        .detail
        .contains("47,1 °C"));
    assert!(f.iter().all(|x| x.label != "Cycles de charge"));

    assert_eq!(verdict(&f), FindingLevel::Bad);
}

#[test]
fn report_shows_serial_but_never_account_names() {
    let report = assemble("R58N00000XX", &samsung_with_problems());
    let json = serde_json::to_string(&report).unwrap();
    // Outil personnel : le numéro de série est affiché tel quel.
    assert!(json.contains("R58N00000XX"));
    assert!(
        !json.contains("example.com"),
        "adresse de compte dans le rapport"
    );
    assert!(!json.contains("Propriétaire"), "nom d'utilisateur Android");
    let findings = serde_json::to_string(&evaluate(&report, today())).unwrap();
    assert!(!findings.contains("example.com"));
}

#[test]
fn patch_age_thresholds() {
    let level = |date: &str| {
        let report = with_patch(date);
        find(&evaluate(&report, today()), "Correctif de sécurité").level
    };
    assert_eq!(level("2026-09-01"), FindingLevel::Ok);
    assert_eq!(level("2026-06-30"), FindingLevel::Ok, "2 mois entiers");
    assert_eq!(level("2026-06-29"), FindingLevel::Warn, "3 mois pile");
    assert_eq!(level("2026-03-01"), FindingLevel::Warn);
    assert_eq!(level("2025-09-29"), FindingLevel::Warn, "12 mois pile");
    assert_eq!(level("2025-08-29"), FindingLevel::Bad, "13 mois");
    assert_eq!(
        level("2026-12-01"),
        FindingLevel::Ok,
        "horloge du PC en retard : pas d'âge négatif"
    );
    assert_eq!(level("inconnu"), FindingLevel::Info);
}

#[test]
fn unlocked_orange_rooted_is_yellow() {
    let mut raw = clean_pixel();
    raw.getprop = raw
        .getprop
        .replace("[ro.boot.flash.locked]: [1]", "[ro.boot.flash.locked]: [0]")
        .replace(
            "[ro.boot.verifiedbootstate]: [green]",
            "[ro.boot.verifiedbootstate]: [orange]",
        );
    raw.which_su = Some("/system/xbin/su\n".into());
    let f = evaluate(&assemble("2A000000XX000", &raw), today());
    assert_eq!(find(&f, "Chargeur de démarrage").level, FindingLevel::Warn);
    assert_eq!(find(&f, "Démarrage vérifié").level, FindingLevel::Warn);
    assert!(find(&f, "Indices de root")
        .detail
        .starts_with("Commande « su »"));
    assert_eq!(verdict(&f), FindingLevel::Warn);
}

#[test]
fn battery_capacity_thresholds() {
    let level = |full: &str| {
        let mut raw = clean_pixel();
        raw.charge_full = Some(full.into());
        raw.charge_full_design = Some("4000000".into());
        let report = assemble("2A000000XX000", &raw);
        find(&evaluate(&report, today()), "Capacité de la batterie").level
    };
    assert_eq!(level("3200000"), FindingLevel::Ok, "80 %");
    assert_eq!(
        level("3180000"),
        FindingLevel::Warn,
        "79,5 % arrondi vers le bas"
    );
    assert_eq!(level("3000000"), FindingLevel::Warn, "75 %");
    assert_eq!(level("2400000"), FindingLevel::Warn, "60 %");
    assert_eq!(level("2300000"), FindingLevel::Bad, "57,5 %");
}

#[test]
fn managed_profile_is_yellow_and_old_android_uses_dumpsys() {
    let mut raw = clean_pixel();
    raw.owners = Some(fixture("dpm_profile_owner.txt"));
    let f = evaluate(&assemble("2A000000XX000", &raw), today());
    assert_eq!(find(&f, "Profil professionnel").level, FindingLevel::Warn);
    assert!(f.iter().all(|x| x.label != "Gestion d'entreprise"));

    raw.owners = Some(fixture("dpm_unknown_old.txt"));
    raw.device_policy = Some(fixture("device_policy_old.txt"));
    let report = assemble("2A000000XX000", &raw);
    assert_eq!(
        report.owners.as_ref().unwrap().device_owner.as_deref(),
        Some("com.example.mdm")
    );
    assert_eq!(
        find(&evaluate(&report, today()), "Gestion d'entreprise").level,
        FindingLevel::Bad
    );
}

#[test]
fn unreadable_outputs_become_issues_and_info() {
    let mut raw = clean_pixel();
    raw.battery = Some("Can't find service: battery\n".into());
    raw.accounts = Some("Permission Denial: can't dump AccountsManager\n".into());
    raw.owners = Some(fixture("dpm_unknown_old.txt"));
    raw.storage = None;
    let report = assemble("2A000000XX000", &raw);
    let steps: Vec<CollectStep> = report.issues.iter().map(|i| i.step).collect();
    assert_eq!(
        steps,
        vec![
            CollectStep::Battery,
            CollectStep::Accounts,
            CollectStep::Owners
        ]
    );
    assert!(report.accounts.is_none());
    let f = evaluate(&report, today());
    assert_eq!(find(&f, "Comptes").level, FindingLevel::Info);
    assert_eq!(find(&f, "Gestion d'entreprise").level, FindingLevel::Info);
    assert_eq!(find(&f, "Batterie").level, FindingLevel::Info);
    assert!(f.iter().all(|x| x.label != "Stockage"));
}

#[test]
fn findings_serialize_for_the_interface() {
    let f = evaluate(&assemble("2A000000XX000", &clean_pixel()), today());
    let json = serde_json::to_value(&f).unwrap();
    assert_eq!(json[0]["level"], "ok");
    assert!(json[0]["label"].is_string());
    let items = serde_json::to_value(manual_checklist()).unwrap();
    assert_eq!(items[0]["id"], "imei_blacklist");
}
