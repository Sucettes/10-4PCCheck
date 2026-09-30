//! Rapport de session de récupération : ce qui a été lu, par quelle méthode, ce qui a été retrouvé
//! et où c'est rangé. Sert de preuve (« voici ce qu'on a pu sauver ») et d'aide pour retrouver les
//! fichiers sur la destination.

use std::collections::BTreeMap;

use pccheck_core::checks::{fmt_bytes, fmt_int};
use pccheck_report::{
    Detail, Item, Level, Report, Section, Subject, SubjectKind, Table, ToolVersion,
};
use serde::{Deserialize, Serialize};

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Lignes du tableau des fichiers illisibles dans le rapport.
const FAILED_ROWS_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMethod {
    /// PhotoRec : par signatures, noms perdus.
    Photorec,
    /// The Sleuth Kit, tous les fichiers supprimés du volume.
    SleuthKitAll,
    /// The Sleuth Kit, fichiers choisis un par un (icat).
    SleuthKitSelection,
}

impl RecoveryMethod {
    fn label(self) -> &'static str {
        match self {
            RecoveryMethod::Photorec => "PhotoRec (recherche par signatures, noms perdus)",
            RecoveryMethod::SleuthKitAll => "The Sleuth Kit (système de fichiers, noms conservés)",
            RecoveryMethod::SleuthKitSelection => {
                "The Sleuth Kit, fichiers choisis (système de fichiers, noms conservés)"
            }
        }
    }
}

/// Une récupération terminée (ou arrêtée), telle que l'application l'a suivie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySession {
    pub method: RecoveryMethod,
    /// Ce qui a été lu : modèle du disque, ou volume (`E:\ CLE`).
    pub source: String,
    pub destination: String,
    pub duration_s: u64,
    pub files: u64,
    pub bytes: u64,
    /// Fichiers retrouvés par extension.
    pub by_extension: BTreeMap<String, u64>,
    pub stopped_by_user: bool,
    /// Problème signalé par l'outil (droits, disque illisible...).
    pub problem: Option<String>,
    /// Fichiers demandés mais illisibles (récupération ciblée).
    pub failed: Vec<String>,
    /// Types de fichiers demandés (PhotoRec).
    pub families: Vec<String>,
    pub tool: Option<ToolVersion>,
}

/// Verdict : réussie (fichiers retrouvés, rien d'illisible), partielle (arrêt, fichiers illisibles),
/// sans résultat (aucun fichier).
pub fn build_recovery_report(s: &RecoverySession) -> Report {
    let mut details = vec![
        Detail::new("Méthode", s.method.label()),
        Detail::new("Destination", s.destination.clone()),
    ];
    if !s.families.is_empty() {
        details.push(Detail::new("Types demandés", s.families.join(", ")));
    }
    let mut rep = Report::new(
        TOOL_VERSION,
        format!("Récupération · {}", s.source),
        Subject {
            kind: SubjectKind::Recovery,
            name: s.source.clone(),
            details,
        },
    );

    let mut res = Section::new("resultat", "Résultat");
    let found = if s.files == 0 {
        Item::new("Fichiers retrouvés", "Aucun", Level::Bad).with_detail(
            "Rien de lisible : données déjà écrasées, SSD avec TRIM, ou mauvais volume. Essaie une autre méthode.",
        )
    } else {
        Item::new("Fichiers retrouvés", fmt_int(s.files), Level::Ok)
            .with_detail(format!("{} au total.", fmt_bytes(s.bytes)))
    };
    res.items.push(found);
    if s.stopped_by_user {
        res.items.push(
            Item::new("Fin", "Arrêtée avant la fin", Level::Warn)
                .with_detail("Relancer la récupération peut retrouver d'autres fichiers."),
        );
    }
    if !s.failed.is_empty() {
        res.items.push(
            Item::new(
                "Fichiers illisibles",
                fmt_int(s.failed.len() as u64),
                Level::Warn,
            )
            .with_detail(
                "Leur emplacement a déjà été réutilisé : leur contenu n'existe plus sur le disque.",
            ),
        );
    }
    if let Some(p) = &s.problem {
        res.items
            .push(Item::new("Problème signalé", "", Level::Warn).with_detail(p.clone()));
    }
    res.items.push(Item::new(
        "Durée",
        format!("{} min {} s", s.duration_s / 60, s.duration_s % 60),
        Level::Neutral,
    ));
    rep.sections.push(res);

    if !s.by_extension.is_empty() {
        let mut by_ext: Vec<(&String, &u64)> = s.by_extension.iter().collect();
        by_ext.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let mut types = Section::new("types", "Par type de fichier");
        types.tables.push(Table {
            title: "Fichiers retrouvés par extension".into(),
            columns: vec!["Extension".into(), "Fichiers".into()],
            rows: by_ext
                .into_iter()
                .map(|(ext, n)| vec![ext.clone(), fmt_int(*n)])
                .collect(),
        });
        rep.sections.push(types);
    }
    if !s.failed.is_empty() {
        let mut failed = Section::new("illisibles", "Fichiers illisibles");
        // Des milliers de lignes feraient un PDF de centaines de pages : la liste complète
        // reste dans les données brutes (JSON).
        let mut rows: Vec<Vec<String>> = s
            .failed
            .iter()
            .take(FAILED_ROWS_MAX)
            .map(|f| vec![f.clone()])
            .collect();
        if s.failed.len() > FAILED_ROWS_MAX {
            rows.push(vec![format!(
                "… et {} autres (liste complète dans le fichier JSON)",
                fmt_int((s.failed.len() - FAILED_ROWS_MAX) as u64)
            )]);
        }
        failed.tables.push(Table {
            title: "Chemins d'origine".into(),
            columns: vec!["Fichier".into()],
            rows,
        });
        rep.sections.push(failed);
    }
    rep.tools = s.tool.clone().into_iter().collect();
    rep.raw = serde_json::to_value(s).unwrap_or_default();
    rep.update_verdict();
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(files: u64) -> RecoverySession {
        RecoverySession {
            method: RecoveryMethod::Photorec,
            source: "SanDisk Ultra 64 Go".into(),
            destination: r"F:\recup\2026-09-29_2210".into(),
            duration_s: 754,
            files,
            bytes: files * 2_000_000,
            by_extension: BTreeMap::from([
                ("jpg".into(), files.saturating_sub(3)),
                ("pdf".into(), 3.min(files)),
            ]),
            stopped_by_user: false,
            problem: None,
            failed: vec![],
            families: vec!["Photos".into(), "Documents".into()],
            tool: None,
        }
    }

    #[test]
    fn successful_session_is_green() {
        let r = build_recovery_report(&session(1287));
        assert_eq!(r.verdict.level, Level::Ok);
        assert_eq!(r.verdict.label_for(r.subject.kind), "Récupération réussie");
        assert_eq!(
            r.sections[1].tables[0].rows[0],
            vec!["jpg".to_string(), "1\u{202F}284".to_string()]
        );
    }

    #[test]
    fn empty_session_is_red_and_partial_is_yellow() {
        let r = build_recovery_report(&session(0));
        assert_eq!(
            r.verdict.label_for(r.subject.kind),
            "Récupération sans résultat"
        );
        let mut s = session(40);
        s.stopped_by_user = true;
        s.failed = vec!["DCIM/_MG_0002.JPG".into()];
        let r = build_recovery_report(&s);
        assert_eq!(
            r.verdict.label_for(r.subject.kind),
            "Récupération partielle"
        );
        assert!(r.sections.iter().any(|x| x.id == "illisibles"));
    }

    #[test]
    fn recovery_report_renders_to_pdf() {
        let r = build_recovery_report(&session(12));
        assert!(pccheck_report::to_pdf(&r).unwrap().starts_with(b"%PDF"));
        assert!(pccheck_report::to_html(&r).contains("Récupération réussie"));
    }
}
