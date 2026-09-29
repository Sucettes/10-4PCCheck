// Pas de console en plus de la fenêtre sous Windows (en version finale).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod elevation;
mod self_test;

use std::path::{Path, PathBuf};

use pccheck_core::{DiskInfo, ScanDevice, Smartctl, SmartctlError};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

struct AppState {
    smartctl: Result<Smartctl, SmartctlError>,
    self_test: Option<PathBuf>,
}

/// Erreur renvoyée à l'interface, sérialisée avec un code stable.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
enum CommandError {
    Smartctl(SmartctlError),
    Internal(String),
}

impl From<SmartctlError> for CommandError {
    fn from(e: SmartctlError) -> Self {
        CommandError::Smartctl(e)
    }
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum SmartctlStatus {
    Ready { path: String, version: String },
    Unavailable { error: SmartctlError },
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    os: &'static str,
    elevated: bool,
    self_test: bool,
    smartctl: SmartctlStatus,
}

#[derive(Serialize)]
struct DiskEntry {
    device: ScanDevice,
    info: Option<DiskInfo>,
    error: Option<SmartctlError>,
}

#[tauri::command]
async fn app_info(state: State<'_, AppState>) -> Result<AppInfo, CommandError> {
    let smartctl = match state.smartctl.clone() {
        Ok(s) => {
            let path = s.path().display().to_string();
            match blocking(move || s.version()).await? {
                Ok(version) => SmartctlStatus::Ready { path, version },
                Err(error) => SmartctlStatus::Unavailable { error },
            }
        }
        Err(error) => SmartctlStatus::Unavailable { error },
    };
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        elevated: elevation::is_elevated(),
        self_test: state.self_test.is_some(),
        smartctl,
    })
}

/// Liste les disques puis lit chacun. Un disque illisible (pont USB sans SMART, par exemple)
/// n'empêche pas la lecture des autres : son erreur est rendue avec lui.
#[tauri::command]
async fn scan_disks(state: State<'_, AppState>) -> Result<Vec<DiskEntry>, CommandError> {
    let smartctl = state.smartctl.clone()?;
    let entries = blocking(move || -> Result<Vec<DiskEntry>, SmartctlError> {
        let devices = smartctl.scan()?;
        Ok(devices
            .into_iter()
            .map(|device| match smartctl.info(&device) {
                Ok(info) => DiskEntry {
                    device,
                    info: Some(info),
                    error: None,
                },
                Err(error) => DiskEntry {
                    device,
                    info: None,
                    error: Some(error),
                },
            })
            .collect())
    })
    .await??;
    Ok(entries)
}

#[tauri::command]
fn self_test_report(
    content: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let path = state
        .self_test
        .as_ref()
        .ok_or_else(|| CommandError::Internal("autotest inactif".into()))?;
    std::fs::write(path, content).map_err(|e| {
        CommandError::Internal(format!("écriture de {} impossible : {e}", path.display()))
    })?;
    app.exit(0);
    Ok(())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::Internal(format!("tâche interrompue : {e}")))
}

/// Dossiers où chercher les outils embarqués, du plus spécifique au plus général :
/// à côté de l'AppImage (clé USB), à côté de l'exécutable, puis dans les ressources de l'app.
fn tool_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        if let Some(parent) = Path::new(&appimage).parent() {
            dirs.push(parent.join("tools"));
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(Path::parent)
    {
        dirs.push(parent.join("tools"));
    }
    if let Ok(resources) = app.path().resource_dir() {
        dirs.push(resources.join("tools"));
    }
    dirs
}

/// Sur une machine sans WebView2, un runtime « version fixe » copié dans `webview2/`
/// à côté de l'exécutable est utilisé à la place.
#[cfg(windows)]
fn use_bundled_webview2() {
    const VAR: &str = "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER";
    if std::env::var_os(VAR).is_some() {
        return;
    }
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("webview2")));
    if let Some(dir) = bundled.filter(|d| d.is_dir()) {
        // Appelé avant la création de tout fil : pas de lecture concurrente de l'environnement.
        std::env::set_var(VAR, dir);
    }
}

fn main() {
    let self_test = match self_test::parse_args(std::env::args()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    #[cfg(windows)]
    use_bundled_webview2();

    if let Some(output) = &self_test {
        self_test::arm_deadline(output.clone());
    }

    let result = tauri::Builder::default()
        .setup(move |app| {
            let smartctl = Smartctl::locate(&tool_dirs(app.handle()));
            app.manage(AppState {
                smartctl,
                self_test,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            scan_disks,
            self_test_report
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("échec du démarrage de l'application : {e}");
        std::process::exit(1);
    }
}
