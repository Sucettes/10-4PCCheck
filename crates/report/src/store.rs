//! Stockage des rapports dans un dossier (sur la clé : `rapports/`). Un rapport = trois
//! fichiers de même nom : `.json` (source, empreinte vérifiable), `.html` et `.pdf`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::error::ReportError;
use crate::format::slug;
use crate::html::to_html;
use crate::model::{report_json, Level, Report, SubjectKind, SCHEMA_VERSION};
use crate::pdf::to_pdf;

/// Chemins des fichiers écrits par [`save`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedReport {
    pub json: PathBuf,
    pub html: PathBuf,
    pub pdf: PathBuf,
}

/// Résumé d'un rapport pour l'écran « Rapports ».
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReportSummary {
    /// Nom de fichier sans extension (`2026-09-29_1430_portable-exemple`).
    pub id: String,
    pub title: String,
    pub subject_kind: SubjectKind,
    pub generated_at: DateTime<FixedOffset>,
    pub verdict_level: Level,
    pub json_path: PathBuf,
    pub html_path: Option<PathBuf>,
    pub pdf_path: Option<PathBuf>,
}

/// Écrit le rapport en JSON, HTML et PDF dans `dir` (créé au besoin).
///
/// Nom : `AAAA-MM-JJ_HHMM_<slug-du-titre>`, date du rapport dans son fuseau. Si ce nom existe
/// déjà (deux rapports du même titre dans la même minute), on ajoute `-2`, `-3`... plutôt que
/// d'écraser. Les trois contenus sont produits en mémoire avant toute écriture : une erreur de
/// rendu PDF ne laisse pas de rapport à moitié écrit. Le JSON est écrit en dernier, c'est lui
/// qui fait apparaître le rapport dans [`list`].
pub fn save(report: &Report, dir: &Path) -> Result<SavedReport, ReportError> {
    let json = report_json(report)?;
    let html = to_html(report);
    let pdf = to_pdf(report)?;

    fs::create_dir_all(dir).map_err(|e| ReportError::io(dir, e))?;
    let base = format!(
        "{}_{}",
        report.generated_at.format("%Y-%m-%d_%H%M"),
        slug(&report.title)
    );
    let stem = free_stem(dir, &base);
    let saved = SavedReport {
        json: dir.join(format!("{stem}.json")),
        html: dir.join(format!("{stem}.html")),
        pdf: dir.join(format!("{stem}.pdf")),
    };
    write(&saved.pdf, &pdf)?;
    write(&saved.html, html.as_bytes())?;
    write(&saved.json, &json)?;
    Ok(saved)
}

fn free_stem(dir: &Path, base: &str) -> String {
    let taken = |stem: &str| {
        ["json", "html", "pdf"]
            .iter()
            .any(|ext| dir.join(format!("{stem}.{ext}")).exists())
    };
    if !taken(base) {
        return base.to_string();
    }
    (2u32..)
        .map(|n| format!("{base}-{n}"))
        .find(|stem| !taken(stem))
        .unwrap_or_else(|| base.to_string())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ReportError> {
    fs::write(path, bytes).map_err(|e| ReportError::io(path, e))
}

/// Relit un rapport. Refuse un schéma plus récent que [`SCHEMA_VERSION`] : mieux vaut une
/// erreur claire qu'un rapport lu à moitié par une ancienne version de l'outil.
pub fn load(path: &Path) -> Result<Report, ReportError> {
    let bytes = fs::read(path).map_err(|e| ReportError::io(path, e))?;
    let invalid = |reason: String| ReportError::InvalidReport {
        path: path.to_path_buf(),
        reason,
    };
    // Lecture en deux temps : d'abord la version seule, pour donner la bonne erreur même si
    // le reste du schéma a changé.
    let probe: VersionProbe = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    if probe.schema_version > SCHEMA_VERSION {
        return Err(ReportError::UnsupportedSchema {
            found: probe.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}

#[derive(Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

/// Lecture partielle pour la liste : serde ignore les champs absents de la structure, donc
/// `sections` et `raw` (le gros du fichier) sont parcourus sans être construits en mémoire.
#[derive(Deserialize)]
struct SummaryProbe {
    schema_version: u32,
    title: String,
    subject: SubjectProbe,
    generated_at: DateTime<FixedOffset>,
    verdict: VerdictProbe,
}

#[derive(Deserialize)]
struct SubjectProbe {
    kind: SubjectKind,
}

#[derive(Deserialize)]
struct VerdictProbe {
    level: Level,
}

/// Rapports du dossier, du plus récent au plus ancien. Les JSON illisibles, étrangers ou d'un
/// schéma trop récent sont ignorés ; un dossier absent donne une liste vide.
pub fn list(dir: &Path) -> Vec<ReportSummary> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<ReportSummary> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json") && p.is_file())
        .filter_map(summarize)
        .collect();
    // Comparaison d'instants (les décalages peuvent différer), puis nom pour un ordre stable.
    out.sort_by(|a, b| {
        b.generated_at
            .cmp(&a.generated_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    out
}

fn summarize(json_path: PathBuf) -> Option<ReportSummary> {
    let bytes = fs::read(&json_path).ok()?;
    let probe: SummaryProbe = serde_json::from_slice(&bytes).ok()?;
    if probe.schema_version > SCHEMA_VERSION {
        return None;
    }
    let id = json_path.file_stem()?.to_string_lossy().into_owned();
    let sibling = |ext: &str| Some(json_path.with_extension(ext)).filter(|p| p.is_file());
    Some(ReportSummary {
        html_path: sibling("html"),
        pdf_path: sibling("pdf"),
        id,
        title: probe.title,
        subject_kind: probe.subject.kind,
        generated_at: probe.generated_at,
        verdict_level: probe.verdict.level,
        json_path,
    })
}
