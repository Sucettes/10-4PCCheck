//! Commandes des rapports : construction (crate `pccheck-assemble`), enregistrement en JSON +
//! HTML + PDF dans `rapports/` à la racine de la clé, liste et ouverture.

use std::path::{Path, PathBuf};

use pccheck_assemble::{
    build_disk_report, build_machine_report, build_phone_report, build_recovery_report,
    InteractiveEntry,
};
use pccheck_report::{
    self as report, ChecklistEntry, Report, ReportSummary, SavedReport, ToolVersion,
};
use tauri::State;

use crate::cache::Cache;
use crate::{blocking, reports_dir, AppState, CommandError};

fn smartctl_tool(state: &AppState) -> Vec<ToolVersion> {
    match state.smartctl.as_ref().ok().and_then(|s| s.version().ok()) {
        Some(v) => vec![ToolVersion {
            name: "smartctl".into(),
            version: v,
        }],
        None => Vec::new(),
    }
}

fn save(report: &mut Report) -> Result<SavedReport, CommandError> {
    report.update_verdict();
    report::save(report, &reports_dir()).map_err(|e| CommandError::Tool(e.to_string()))
}

#[tauri::command]
pub async fn save_disk_report(
    device: String,
    cache: State<'_, Cache>,
    state: State<'_, AppState>,
) -> Result<SavedReport, CommandError> {
    let tools = smartctl_tool(&state);
    let cache = cache.inner().clone();
    blocking(move || {
        let mut rep =
            build_disk_report(&cache.lock(), &device, tools).map_err(CommandError::Internal)?;
        save(&mut rep)
    })
    .await?
}

#[tauri::command]
pub async fn save_phone_report(
    checklist: Vec<ChecklistEntry>,
    cache: State<'_, Cache>,
) -> Result<SavedReport, CommandError> {
    let cache = cache.inner().clone();
    blocking(move || {
        let mut rep =
            build_phone_report(&cache.lock(), checklist).map_err(CommandError::Internal)?;
        save(&mut rep)
    })
    .await?
}

#[tauri::command]
pub async fn save_machine_report(
    interactive: Vec<InteractiveEntry>,
    checklist: Vec<ChecklistEntry>,
    cache: State<'_, Cache>,
    state: State<'_, AppState>,
) -> Result<SavedReport, CommandError> {
    let tools = smartctl_tool(&state);
    let cache = cache.inner().clone();
    blocking(move || {
        let mut rep = build_machine_report(&cache.lock(), &interactive, checklist, tools)
            .map_err(CommandError::Internal)?;
        save(&mut rep)
    })
    .await?
}

#[tauri::command]
pub async fn list_reports() -> Result<Vec<ReportSummary>, CommandError> {
    blocking(|| report::list(&reports_dir())).await
}

/// Ouvre un fichier du dossier des rapports (ou le dossier lui-même) avec l'application par
/// défaut du système. Refuse tout chemin hors de ce dossier.
#[tauri::command]
pub fn open_report_file(path: Option<String>) -> Result<(), CommandError> {
    let dir = reports_dir();
    let _ = std::fs::create_dir_all(&dir);
    let target = match path {
        Some(p) => PathBuf::from(p),
        None => dir.clone(),
    };
    if !is_inside(&target, &dir) {
        return Err(CommandError::Internal(
            "chemin hors du dossier des rapports".into(),
        ));
    }
    open_with_system(&target)
        .map_err(|e| CommandError::Internal(format!("ouverture impossible : {e}")))
}

/// Aperçu du rapport machine, sans l'enregistrer : l'écran « Analyse complète » affiche
/// exactement le verdict et les sections que contiendra le PDF.
#[tauri::command]
pub async fn preview_machine_report(
    interactive: Vec<InteractiveEntry>,
    checklist: Vec<ChecklistEntry>,
    cache: State<'_, Cache>,
    state: State<'_, AppState>,
) -> Result<Report, CommandError> {
    let tools = smartctl_tool(&state);
    let cache = cache.inner().clone();
    blocking(move || {
        let mut rep = build_machine_report(&cache.lock(), &interactive, checklist, tools)
            .map_err(CommandError::Internal)?;
        rep.update_verdict();
        Ok(rep)
    })
    .await?
}

fn is_inside(path: &Path, dir: &Path) -> bool {
    match (path.canonicalize(), dir.canonicalize()) {
        (Ok(p), Ok(d)) => p.starts_with(d),
        _ => false,
    }
}

pub(crate) fn open_with_system(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let mut cmd = std::process::Command::new("explorer.exe");
    #[cfg(not(windows))]
    let mut cmd = std::process::Command::new("xdg-open");
    pccheck_core::process::detach_from_job(&mut cmd);
    // L'explorateur renvoie souvent un code non nul même quand il a ouvert le fichier : on ne
    // vérifie que le lancement.
    cmd.arg(path).spawn().map(|_| ())
}

/// Rapport de la dernière récupération terminée.
#[tauri::command]
pub async fn save_recovery_report(cache: State<'_, Cache>) -> Result<SavedReport, CommandError> {
    let cache = cache.inner().clone();
    blocking(move || {
        let session = cache.lock().recovery.clone().ok_or_else(|| {
            CommandError::Internal("aucune récupération terminée dans cette session".into())
        })?;
        let mut rep = build_recovery_report(&session);
        save(&mut rep)
    })
    .await?
}
