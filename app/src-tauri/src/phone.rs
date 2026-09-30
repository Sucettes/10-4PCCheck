//! Commandes de l'écran Téléphone : appareils Android branchés, puis analyse de l'un d'eux.

use pccheck_android::{
    evaluate, manual_checklist, no_device_guidance, today, verdict, Adb, AdbDevice, AdbError,
};
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
        match adb.devices() {
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
