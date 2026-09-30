//! Commandes de l'écran Téléphone : appareils Android branchés, puis analyse de l'un d'eux.

use pccheck_android::{
    evaluate, manual_checklist, no_device_guidance, today, verdict, Adb, AdbDevice, AdbError,
};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use pccheck_assemble::PhoneAnalysis;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::cache::Cache;
use crate::{blocking, tool_dirs, CommandError};

#[derive(Serialize)]
pub struct PhoneDevices {
    /// Version d'adb, `None` s'il est introuvable ou ne répond pas.
    adb_version: Option<String>,
    /// Pourquoi adb n'est pas utilisable (introuvable, serveur qui ne démarre pas...).
    adb_error: Option<String>,
    devices: Vec<AdbDevice>,
    /// Conseil à afficher quand aucun téléphone n'est prêt.
    guidance: String,
}

fn adb(app: &AppHandle) -> Result<Adb, AdbError> {
    Adb::locate(&tool_dirs(app))
}

/// adb qui a démarré le serveur, si c'est l'outil. Arrêté à la fermeture : sinon `adb.exe`
/// reste lancé depuis la clé (impossible à éjecter) et écoute sur le PC du vendeur. Un serveur
/// qui tournait déjà (Android Studio) n'est jamais arrêté.
static STARTED_SERVER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Port par défaut du serveur adb.
const ADB_PORT: u16 = 5037;

fn server_running() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], ADB_PORT));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Arrête le serveur adb s'il a été démarré par l'outil.
pub fn stop_adb_server() {
    let started = STARTED_SERVER
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take();
    if let Some(path) = started {
        let _ = Adb::new(path).kill_server();
    }
}

#[tauri::command]
pub async fn phone_devices(app: AppHandle) -> Result<PhoneDevices, CommandError> {
    blocking(move || {
        let guidance = no_device_guidance();
        let adb = match adb(&app) {
            Ok(a) => a,
            Err(e) => {
                return PhoneDevices {
                    adb_version: None,
                    adb_error: Some(e.to_string()),
                    devices: Vec::new(),
                    guidance,
                }
            }
        };
        let version = adb.version().ok().map(|v| v.release.unwrap_or(v.protocol));
        let was_running = server_running();
        let devices = adb.devices();
        if !was_running && server_running() {
            *STARTED_SERVER.lock().unwrap_or_else(|p| p.into_inner()) =
                Some(adb.path().to_path_buf());
        }
        match devices {
            Ok(devices) => PhoneDevices {
                adb_version: version,
                adb_error: None,
                devices,
                guidance,
            },
            Err(e) => PhoneDevices {
                adb_version: version,
                adb_error: Some(e.to_string()),
                devices: Vec::new(),
                guidance,
            },
        }
    })
    .await
}

#[tauri::command]
pub async fn phone_collect(
    serial: String,
    app: AppHandle,
    cache: State<'_, Cache>,
) -> Result<PhoneAnalysis, CommandError> {
    let cache = cache.inner().clone();
    blocking(move || -> Result<PhoneAnalysis, CommandError> {
        let adb = adb(&app).map_err(|e| CommandError::Tool(e.to_string()))?;
        let report = adb
            .collect(&serial)
            .map_err(|e| CommandError::Tool(e.to_string()))?;
        let findings = evaluate(&report, today());
        let level = verdict(&findings);
        let analysis = PhoneAnalysis {
            report,
            findings,
            verdict: level,
            checklist: manual_checklist(),
        };
        cache.lock().phone = Some(analysis.clone());
        Ok(analysis)
    })
    .await?
}
