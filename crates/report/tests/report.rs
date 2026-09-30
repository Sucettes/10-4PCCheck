//! Tests d'intégration : export HTML et PDF, empreinte, stockage. Données inventées seulement.

use std::fs;

use pccheck_report::{
    list, load, report_hash, report_json, sample_report, save, to_html, to_pdf, ChecklistEntry,
    Detail, Item, Level, Report, ReportError, Section, SubjectKind, Table, ToolVersion,
    SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};

/// Rapport dont chaque champ texte contient les caractères spéciaux de Typst et du HTML.
fn nasty_report() -> Report {
    const NASTY: &str = "# $ * _ < > @ \\ [ ] ` ~ = - + / % & \" ' { } Éléphant àçèùœ «guillemets» <script>alert(1)</script>";
    let mut r = sample_report();
    r.title = format!("Titre {NASTY}");
    r.subject.name = NASTY.into();
    r.subject.details.push(Detail::new(NASTY, NASTY));
    let mut s = Section::new(NASTY, NASTY);
    s.items
        .push(Item::new(NASTY, NASTY, Level::Bad).with_detail(NASTY));
    s.tables.push(Table {
        title: NASTY.into(),
        columns: vec![NASTY.into(), "#".into()],
        rows: vec![vec![NASTY.into(), "[".into()], vec!["]".into(), "$".into()]],
    });
    r.sections.push(s);
    r.checklist.push(ChecklistEntry {
        label: NASTY.into(),
        checked: false,
        note: Some(NASTY.into()),
    });
    r.tools.push(ToolVersion {
        name: NASTY.into(),
        version: "#1".into(),
    });
    r.raw = serde_json::json!({ "texte": NASTY });
    r.update_verdict();
    r
}

#[test]
fn html_is_self_contained_and_escaped() {
    let r = nasty_report();
    let html = to_html(&r);
    assert!(html.starts_with("<!doctype html>"));
    // Aucun script injecté : le seul <script> est celui de l'outil.
    assert_eq!(html.matches("<script>").count(), 1);
    assert!(!html.contains("<script>alert"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(html.contains("&quot;"));
    assert!(html.contains("&amp;"));
    // Aucune ressource externe.
    for forbidden in ["http://", "https://", "<link", "src=", "@import", "url("] {
        assert!(!html.contains(forbidden), "{forbidden} trouvé dans le HTML");
    }
    assert!(html.contains("À éviter"));
    assert!(html.contains(&report_hash(&r)));
    assert!(html.contains("@media print"));
}

#[test]
fn html_shows_labels_with_colors() {
    let html = to_html(&sample_report());
    assert!(html.contains("À négocier"));
    assert!(html.contains("class=\"badge lvl-warn\">À surveiller</span>"));
    assert!(html.contains("class=\"badge lvl-ok\">Bon</span>"));
    assert!(html.contains("aria-sort"));
    assert!(html.contains("<details>"));
}

#[test]
fn pdf_starts_with_header_and_handles_special_characters() {
    let pdf = to_pdf(&sample_report()).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(pdf.windows(5).any(|w| w == b"%%EOF"));

    let pdf = to_pdf(&nasty_report()).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn pdf_of_empty_report() {
    let r = Report::new(
        "0.0.0",
        "",
        pccheck_report::Subject {
            kind: SubjectKind::Phone,
            name: String::new(),
            details: vec![],
        },
    );
    assert_eq!(r.verdict.level, Level::Neutral);
    assert!(to_pdf(&r).unwrap().starts_with(b"%PDF"));
    assert!(to_html(&r).contains("Sans verdict"));
}

#[test]
fn hash_is_sha256_of_saved_json_and_survives_reload() {
    let dir = tempfile::tempdir().unwrap();
    let r = nasty_report();
    let saved = save(&r, dir.path()).unwrap();

    let bytes = fs::read(&saved.json).unwrap();
    assert_eq!(bytes, report_json(&r).unwrap());
    let file_hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(file_hash, report_hash(&r));
    assert_eq!(file_hash.len(), 64);

    let loaded = load(&saved.json).unwrap();
    assert_eq!(loaded, r);
    assert_eq!(report_hash(&loaded), file_hash);

    // Une modification d'un seul caractère change l'empreinte.
    let mut tampered = loaded;
    tampered.verdict.summary.push(' ');
    assert_ne!(report_hash(&tampered), file_hash);
}

#[test]
fn save_names_files_and_avoids_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("rapports").join("sous-dossier");
    let r = sample_report();
    let first = save(&r, &target).unwrap();
    assert_eq!(
        first.json.file_name().unwrap().to_string_lossy(),
        "2026-09-29_1430_portable-exemple-x1-vendeur-fictif.json"
    );
    for p in [&first.json, &first.html, &first.pdf] {
        assert!(fs::metadata(p).unwrap().len() > 0, "{p:?} vide");
    }
    let second = save(&r, &target).unwrap();
    assert_eq!(
        second.json.file_name().unwrap().to_string_lossy(),
        "2026-09-29_1430_portable-exemple-x1-vendeur-fictif-2.json"
    );
    assert_eq!(
        fs::read(&first.json).unwrap(),
        fs::read(&second.json).unwrap()
    );
}

#[test]
fn list_sorts_newest_first_and_ignores_invalid_files() {
    let dir = tempfile::tempdir().unwrap();
    assert!(list(&dir.path().join("absent")).is_empty());

    let old = sample_report();
    let mut newer = sample_report();
    newer.title = "Téléphone plus récent".into();
    newer.subject.kind = SubjectKind::Phone;
    // 20 h 29 à UTC+2 = 14 h 29 à UTC-4 : une minute AVANT l'exemple (14 h 30 à UTC-4), bien
    // que l'heure affichée soit plus tardive. Le tri doit comparer des instants.
    newer.generated_at = chrono::DateTime::parse_from_rfc3339("2026-09-29T20:29:00+02:00").unwrap();
    let mut newest = sample_report();
    newest.title = "Disque le plus récent".into();
    newest.generated_at =
        chrono::DateTime::parse_from_rfc3339("2026-10-01T09:00:00-04:00").unwrap();

    save(&old, dir.path()).unwrap();
    let saved_newer = save(&newer, dir.path()).unwrap();
    save(&newest, dir.path()).unwrap();
    fs::remove_file(&saved_newer.pdf).unwrap();

    // Fichiers à ignorer : JSON invalide, JSON étranger, schéma trop récent, autre extension.
    fs::write(dir.path().join("casse.json"), b"{ pas du json").unwrap();
    fs::write(dir.path().join("autre.json"), br#"{"nom": "x"}"#).unwrap();
    let mut future = serde_json::to_value(&old).unwrap();
    future["schema_version"] = serde_json::json!(SCHEMA_VERSION + 1);
    let future_path = dir.path().join("futur.json");
    fs::write(&future_path, serde_json::to_vec(&future).unwrap()).unwrap();
    fs::write(dir.path().join("notes.txt"), b"x").unwrap();

    let items = list(dir.path());
    let titles: Vec<&str> = items.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Disque le plus récent",
            "Portable Exemple X1 (vendeur fictif)",
            "Téléphone plus récent",
        ]
    );
    let phone = items
        .iter()
        .find(|s| s.subject_kind == SubjectKind::Phone)
        .unwrap();
    assert!(phone.pdf_path.is_none());
    assert!(phone.html_path.is_some());
    assert_eq!(phone.verdict_level, Level::Warn);

    match load(&future_path) {
        Err(ReportError::UnsupportedSchema { found, supported }) => {
            assert_eq!((found, supported), (SCHEMA_VERSION + 1, SCHEMA_VERSION));
        }
        other => panic!("attendu UnsupportedSchema, obtenu {other:?}"),
    }
    assert!(matches!(
        load(&dir.path().join("casse.json")),
        Err(ReportError::InvalidReport { .. })
    ));
    assert!(matches!(
        load(&dir.path().join("inexistant.json")),
        Err(ReportError::Io { .. })
    ));
}

#[test]
fn sample_written_to_temp_dir() {
    let dir = tempfile::tempdir().unwrap();
    let r = sample_report();
    let html_path = dir.path().join("exemple.html");
    let pdf_path = dir.path().join("exemple.pdf");
    fs::write(&html_path, to_html(&r)).unwrap();
    fs::write(&pdf_path, to_pdf(&r).unwrap()).unwrap();
    assert!(fs::metadata(&html_path).unwrap().len() > 0);
    assert!(fs::metadata(&pdf_path).unwrap().len() > 0);
}

#[test]
fn errors_serialize_with_code_and_detail() {
    let e = ReportError::UnsupportedSchema {
        found: 9,
        supported: 1,
    };
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(v["code"], "unsupported_schema");
    assert_eq!(v["detail"]["found"], 9);
}
