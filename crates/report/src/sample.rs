//! Rapport d'exemple, entièrement inventé (modèle, numéro de série et valeurs fictifs).
//! Sert aux tests et à regarder le rendu HTML et PDF sans matériel.

use chrono::{DateTime, FixedOffset, TimeZone};
use serde_json::json;

use crate::model::{
    ChecklistEntry, Detail, Item, Level, Report, Section, Subject, SubjectKind, Table, ToolVersion,
    SCHEMA_VERSION,
};
use crate::verdict::{compute_verdict, Thresholds};

/// Date fixe pour des sorties reproductibles : 29 septembre 2026, 14 h 30, UTC−4.
fn sample_date() -> DateTime<FixedOffset> {
    FixedOffset::west_opt(4 * 3600)
        .and_then(|tz| tz.with_ymd_and_hms(2026, 9, 29, 14, 30, 0).single())
        .unwrap_or_default()
}

pub fn sample_report() -> Report {
    let t = Thresholds::default();

    let mut disk = Section::new("disk", "Disque · SSD SATA Exemple 512 Go");
    disk.items = vec![
        Item::new("Vie restante", "86 %", t.ssd_life(86))
            .with_detail("Attribut 177 (usure), seuil vert à 90 %."),
        Item::new("Secteurs réalloués", "0", t.reallocated_sectors(0, false)),
        Item::new("Secteurs en attente", "0", t.pending_sectors(0)),
        Item::new("Température au repos", "38 °C", t.disk_temperature(38)),
        Item::new("Heures d'utilisation", "3\u{202F}864 h", Level::Info)
            .with_detail("Environ 161 jours allumé."),
        Item::new("Données écrites", "18,4 To", Level::Info),
    ];
    disk.tables = vec![Table {
        title: "Attributs SMART".into(),
        columns: ["ID", "Attribut", "Valeur", "Pire", "Seuil", "Brut", "État"]
            .map(String::from)
            .to_vec(),
        rows: [
            ["5", "Secteurs réalloués", "100", "100", "10", "0", "Bon"],
            ["9", "Heures d'utilisation", "99", "99", "0", "3864", "Bon"],
            ["12", "Démarrages", "99", "99", "0", "1\u{202F}247", "Bon"],
            [
                "177",
                "Usure (Wear Leveling Count)",
                "86",
                "86",
                "0",
                "212",
                "À surveiller",
            ],
            [
                "190",
                "Température du flux d'air",
                "62",
                "48",
                "0",
                "38",
                "Bon",
            ],
            [
                "194",
                "Température",
                "62",
                "48",
                "0",
                "38 (min/max 21/52)",
                "Bon",
            ],
            ["199", "Erreurs CRC du câble", "100", "100", "0", "0", "Bon"],
            [
                "241",
                "LBA écrits au total",
                "99",
                "99",
                "0",
                "39\u{202F}504\u{202F}122\u{202F}880",
                "Bon",
            ],
        ]
        .iter()
        .map(|row| row.iter().map(|c| c.to_string()).collect())
        .collect(),
    }];

    let mut battery = Section::new("battery", "Batterie");
    battery.items = vec![
        Item::new("Santé de la batterie", "72 %", t.battery_health(72.0))
            .with_detail("Capacité actuelle / capacité d'origine : 41,0 Wh / 57,0 Wh."),
        Item::new("Cycles de charge", "412", Level::Info),
        Item::new("Fabricant", "Exemple Power", Level::Neutral),
    ];

    let mut keyboard = Section::new("keyboard", "Clavier");
    keyboard.items = vec![Item::new("Clavier", "2 touches sans réponse", Level::Warn)
        .with_detail("É et F7 n'ont produit aucun événement.")];

    let sections = vec![disk, battery, keyboard];
    Report {
        schema_version: SCHEMA_VERSION,
        tool_version: "0.1.0".into(),
        generated_at: sample_date(),
        title: "Portable Exemple X1 (vendeur fictif)".into(),
        subject: Subject {
            kind: SubjectKind::Machine,
            name: "Exemple X1, 14 pouces".into(),
            details: vec![
                Detail::new("Processeur", "Exemple Core i5-0000U"),
                Detail::new("Mémoire", "16 Go"),
                Detail::new("Numéro de série", "EXEMPLE-0000"),
                Detail::new("Système", "Windows 11 Pro"),
            ],
        },
        verdict: compute_verdict(&sections),
        sections,
        checklist: vec![
            ChecklistEntry {
                label: "Écran : aucun pixel mort sur les couleurs pleines".into(),
                checked: true,
                note: None,
            },
            ChecklistEntry {
                label: "Clavier : toutes les touches répondent".into(),
                checked: false,
                note: Some("É et F7 sans réponse.".into()),
            },
            ChecklistEntry {
                label: "Compte Microsoft du vendeur retiré".into(),
                checked: true,
                note: None,
            },
        ],
        tools: vec![ToolVersion {
            name: "smartctl".into(),
            version: "7.5".into(),
        }],
        raw: json!({
            "smartctl": {
                "json_format_version": [1, 0],
                "model_name": "SSD SATA Exemple 512 Go",
                "serial_number": "EXEMPLE-0000",
                "power_on_time": { "hours": 3864 },
                "temperature": { "current": 38 }
            },
            "battery": { "design_capacity_mwh": 57000, "full_charge_capacity_mwh": 41040, "cycle_count": 412 }
        }),
    }
}
