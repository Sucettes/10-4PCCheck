//! Commandes de l'écran Récupération (PhotoRec).

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use pccheck_recovery::tsk::{recover_selected, SelectedFile, SelectionResult};
use pccheck_recovery::{
    disk_from_smartctl_name, list_deleted, list_found, list_volumes, locate_console_tool,
    locate_photorec, locate_tsk, trim_warning, volume_device, ConsoleTool, DeletedList, FileFamily,
    FoundFile, RecoveryConfig, RecoveryJob, Source, TskJob, Volume, SUPPORT_HELP,
};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::jobs::Jobs;
use crate::{tool_dirs, usb_root, CommandError};

const POLL: Duration = Duration::from_secs(1);

fn tool_err(e: impl std::fmt::Display) -> CommandError {
    CommandError::Tool(e.to_string())
}

#[derive(Serialize)]
pub struct VolumeView {
    #[serde(flatten)]
    volume: Volume,
    /// Le volume est sur le disque `source` demandé.
    on_source: bool,
}

#[derive(Serialize)]
pub struct RecoveryStatus {
    /// Chemin de PhotoRec, `None` s'il est absent de la clé.
    photorec: Option<String>,
    photorec_error: Option<String>,
    /// Dossier proposé par défaut : `recup/` à la racine de la clé.
    default_destination: String,
    help: &'static str,
    volumes: Vec<VolumeView>,
    /// The Sleuth Kit (récupération avec les noms) disponible.
    tsk: bool,
    /// TestDisk disponible pour la console intégrée.
    testdisk: bool,
}

/// État de l'écran Récupération. `source` : nom smartctl du disque choisi, pour marquer les
/// volumes qui sont dessus (destination interdite ; volume à tester pour la capacité réelle).
#[tauri::command]
pub async fn recovery_status(
    source: Option<String>,
    app: AppHandle,
) -> Result<RecoveryStatus, CommandError> {
    crate::blocking(move || {
        let source_disk = source.and_then(|s| disk_from_smartctl_name(&s).ok());
        let (photorec, photorec_error) = match locate_photorec(&tool_dirs(&app)) {
            Ok(p) => (Some(p.display().to_string()), None),
            Err(e) => (None, Some(e.to_string())),
        };
        RecoveryStatus {
            photorec,
            photorec_error,
            default_destination: usb_root().join("recup").display().to_string(),
            help: SUPPORT_HELP,
            tsk: locate_tsk(&tool_dirs(&app)).is_ok(),
            testdisk: locate_console_tool(ConsoleTool::Testdisk, &tool_dirs(&app)).is_some(),
            volumes: list_volumes()
                .into_iter()
                .map(|volume| VolumeView {
                    on_source: source_disk.as_ref().is_some_and(|d| volume.is_on_disk(d)),
                    volume,
                })
                .collect(),
        }
    })
    .await
}

#[tauri::command]
pub fn recovery_trim_warning(is_ssd: bool, trim_supported: Option<bool>) -> Option<String> {
    trim_warning(is_ssd, trim_supported)
}

/// Lance PhotoRec sur le disque entier `disk` (nom smartctl). Identifiant de tâche : `recovery`.
/// La vérification de la destination et le lancement ont lieu tout de suite : une erreur
/// (même disque, PhotoRec absent) revient directement à l'écran.
#[tauri::command]
pub fn start_recovery(
    disk: String,
    destination: String,
    families: Vec<FileFamily>,
    paranoid: bool,
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
) -> Result<String, CommandError> {
    let disk = disk_from_smartctl_name(&disk).map_err(tool_err)?;
    let photorec = locate_photorec(&tool_dirs(&app)).map_err(tool_err)?;
    let config = RecoveryConfig {
        source: Source::Disk { disk },
        destination: PathBuf::from(destination),
        families,
        paranoid,
    };
    let mut job = RecoveryJob::start(config, &photorec).map_err(tool_err)?;
    jobs.start(&app, "recovery".into(), move |ctx| loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            job.stop();
            return Ok::<_, ()>(job.progress());
        }
        let p = job.progress();
        ctx.progress(&p);
        if !p.running {
            return Ok(p);
        }
        std::thread::sleep(POLL);
    })
    .map_err(CommandError::Internal)?;
    Ok("recovery".into())
}

#[tauri::command]
pub async fn recovery_found(
    destination: String,
    limit: usize,
) -> Result<Vec<FoundFile>, CommandError> {
    crate::blocking(move || {
        list_found(&PathBuf::from(destination), limit.min(500)).map_err(tool_err)
    })
    .await?
}

/// Ouvre un dossier (destination de récupération) dans l'explorateur. Dossier seulement :
/// jamais un fichier, qui pourrait être un exécutable récupéré.
#[tauri::command]
pub fn open_folder(path: String) -> Result<(), CommandError> {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err(CommandError::Internal("ce n'est pas un dossier".into()));
    }
    crate::reports::open_with_system(&path)
        .map_err(|e| CommandError::Internal(format!("ouverture impossible : {e}")))
}

/// Liste des fichiers supprimés d'un volume (`volume` : racine, ex. `E:\`), avec leur nom.
/// Peut prendre plusieurs minutes sur un gros volume NTFS.
#[tauri::command]
pub async fn tsk_list(volume: String, app: AppHandle) -> Result<DeletedList, CommandError> {
    crate::blocking(move || {
        let tsk = locate_tsk(&tool_dirs(&app)).map_err(tool_err)?;
        let device = volume_device(&PathBuf::from(&volume)).ok_or_else(|| {
            CommandError::Tool(format!("{volume} n'est pas la racine d'un volume"))
        })?;
        list_deleted(&tsk, &device).map_err(tool_err)
    })
    .await?
}

/// Récupère les fichiers supprimés du volume `volume` vers `destination` (autre disque).
/// Identifiant de tâche : `tsk`.
#[tauri::command]
pub fn start_tsk(
    volume: String,
    destination: String,
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
) -> Result<String, CommandError> {
    let tsk = locate_tsk(&tool_dirs(&app)).map_err(tool_err)?;
    let mut job = TskJob::start(&tsk, &PathBuf::from(volume), &PathBuf::from(destination))
        .map_err(tool_err)?;
    jobs.start(&app, "tsk".into(), move |ctx| loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            job.stop();
            return Ok::<_, ()>(job.progress());
        }
        let p = job.progress();
        ctx.progress(&p);
        if !p.running {
            // Dernière lecture : laisse au fil de sortie le temps de rendre le compte final.
            std::thread::sleep(Duration::from_millis(300));
            return Ok(job.progress());
        }
        std::thread::sleep(POLL);
    })
    .map_err(CommandError::Internal)?;
    Ok("tsk".into())
}

/// Récupère seulement les fichiers cochés (icat), à leur chemin d'origine sous `destination`.
#[tauri::command]
pub async fn tsk_recover_selected(
    volume: String,
    files: Vec<SelectedFile>,
    destination: String,
    app: AppHandle,
) -> Result<SelectionResult, CommandError> {
    crate::blocking(move || {
        let tsk = locate_tsk(&tool_dirs(&app)).map_err(tool_err)?;
        recover_selected(
            &tsk,
            &PathBuf::from(volume),
            &files,
            &PathBuf::from(destination),
        )
        .map_err(tool_err)
    })
    .await?
}
