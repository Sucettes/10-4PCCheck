// Pas de console en plus de la fenêtre sous Windows (en version finale).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod elevation;
mod self_test;

use std::path::{Path, PathBuf};

use pccheck_core::{DiskEntry, Smartctl, SmartctlError};
use serde::Serialize;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, State, WebviewWindow};

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

/// Liste et lit tous les disques (voir `Smartctl::scan_all`).
#[tauri::command]
async fn scan_disks(state: State<'_, AppState>) -> Result<Vec<DiskEntry>, CommandError> {
    let smartctl = state.smartctl.clone()?;
    Ok(blocking(move || smartctl.scan_all()).await??)
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

/// Taille voulue de la fenêtre, réduite si l'écran est plus petit (portables en 1366 x 768,
/// vieux écrans en 1024 x 768), puis centrée.
fn fit_to_screen(window: &WebviewWindow) -> tauri::Result<()> {
    const WIDTH: f64 = 1280.0;
    const HEIGHT: f64 = 720.0;
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);
    let width = WIDTH.min(screen.width * 0.95);
    let height = HEIGHT.min(screen.height * 0.9);
    window.set_size(LogicalSize::new(width, height))?;
    // Position calculée ici : `center()` peut lire l'ancienne taille, le redimensionnement
    // étant appliqué de façon asynchrone par certains gestionnaires de fenêtres.
    window.set_position(LogicalPosition::new(
        origin.x + (screen.width - width) / 2.0,
        origin.y + (screen.height - height) / 2.0,
    ))
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
            if let Some(window) = app.get_webview_window("main") {
                // Non bloquant : une fenêtre mal dimensionnée reste utilisable.
                if let Err(e) = fit_to_screen(&window) {
                    eprintln!("ajustement de la fenêtre à l'écran impossible : {e}");
                }
            }
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
