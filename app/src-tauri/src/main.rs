// Pas de console en plus de la fenêtre sous Windows (en version finale).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cache;
mod elevation;
mod jobs;
mod machine;
mod phone;
mod recover;
mod reports;
mod self_test;
mod terminal;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cache::Cache;
use jobs::{JobState, Jobs};
use pccheck_core::{capacity, surface};
use pccheck_core::{DiskEntry, ScanDevice, SelfTestKind, SelfTestStatus, Smartctl, SmartctlError};
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
    /// Erreur d'un autre outil (adb, PhotoRec...), déjà formulée pour l'utilisateur.
    Tool(String),
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
async fn scan_disks(
    state: State<'_, AppState>,
    cache: State<'_, Cache>,
) -> Result<Vec<DiskView>, CommandError> {
    let smartctl = state.smartctl.clone()?;
    let (disks, volumes) = blocking(move || {
        smartctl
            .scan_all()
            .map(|d| (d, pccheck_recovery::list_volumes()))
    })
    .await??;
    cache.lock().disks = disks.clone();
    Ok(disks
        .into_iter()
        .map(|entry| {
            let disk = pccheck_recovery::disk_from_smartctl_name(&entry.device.name).ok();
            let volumes = volumes
                .iter()
                .filter(|v| disk.is_some() && v.disk == disk)
                .map(|v| VolumeTag {
                    path: v.path.display().to_string(),
                    label: v.label.clone(),
                })
                .collect();
            DiskView { entry, volumes }
        })
        .collect())
}

/// Disque tel qu'affiché : ses données, plus les volumes (lettres) qu'il porte, pour distinguer
/// deux disques du même modèle.
#[derive(Serialize)]
struct DiskView {
    #[serde(flatten)]
    entry: DiskEntry,
    volumes: Vec<VolumeTag>,
}

#[derive(Serialize)]
struct VolumeTag {
    /// `C:\` sous Windows, point de montage sous Linux.
    path: String,
    /// Nom du volume, vide s'il n'en a pas.
    label: String,
}

/// Lance un auto-test SMART. `device` vient de `scan_disks` : chemin et type passés en arguments
/// séparés à smartctl, jamais à un shell.
#[tauri::command]
async fn start_smart_test(
    device: ScanDevice,
    kind: SelfTestKind,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let smartctl = state.smartctl.clone()?;
    Ok(blocking(move || smartctl.start_self_test(&device, kind)).await??)
}

#[tauri::command]
async fn abort_smart_test(
    device: ScanDevice,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let smartctl = state.smartctl.clone()?;
    Ok(blocking(move || smartctl.abort_self_test(&device)).await??)
}

#[tauri::command]
async fn smart_test_status(
    device: ScanDevice,
    state: State<'_, AppState>,
    cache: State<'_, Cache>,
) -> Result<SelfTestStatus, CommandError> {
    let smartctl = state.smartctl.clone()?;
    let name = device.name.clone();
    let status = blocking(move || smartctl.self_test_status(&device)).await??;
    cache.lock().self_tests.insert(name, status.clone());
    Ok(status)
}

/// État d'une tâche longue (voir `jobs`). Tâche inconnue : état vide.
#[tauri::command]
fn job_state(id: String, jobs: State<'_, Arc<Jobs>>) -> JobState {
    jobs.state(&id)
}

#[tauri::command]
fn cancel_job(id: String, jobs: State<'_, Arc<Jobs>>) {
    jobs.cancel(&id);
}

/// Scan de surface en lecture seule. Identifiant de tâche : `surface:<chemin smartctl>`.
#[tauri::command]
fn start_surface_scan(
    device: ScanDevice,
    total_bytes: u64,
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
    cache: State<'_, Cache>,
) -> Result<String, CommandError> {
    let id = format!("surface:{}", device.name);
    let cache = cache.inner().clone();
    jobs.start(&app, id.clone(), move |ctx| {
        let result =
            surface::scan_device(&device.name, total_bytes, &ctx.cancel, |p| ctx.progress(p))?;
        cache
            .lock()
            .surface
            .insert(device.name.clone(), result.clone());
        Ok::<_, surface::SurfaceError>(result)
    })
    .map_err(CommandError::Internal)?;
    Ok(id)
}

/// Test de capacité réelle sur l'espace libre du volume `path`. Identifiant : `capacity`.
#[tauri::command]
fn start_capacity_test(
    path: String,
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
    cache: State<'_, Cache>,
) -> Result<String, CommandError> {
    let id = "capacity".to_string();
    let cache = cache.inner().clone();
    jobs.start(&app, id.clone(), move |ctx| {
        let result =
            capacity::run_capacity_test(Path::new(&path), None, &ctx.cancel, |p| ctx.progress(p))?;
        cache.lock().capacity = Some(result.clone());
        Ok::<_, capacity::CapacityError>(result)
    })
    .map_err(CommandError::Internal)?;
    Ok(id)
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
    // Le dossier de ressources est souvent celui de l'exécutable, en forme « verbatim » (\\?\D:\…) :
    // même dossier, cherché deux fois sinon.
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let key = d.display().to_string();
        let key = key.strip_prefix(r"\\?\").unwrap_or(&key).to_lowercase();
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    // PhotoRec est livré avec ses DLL dans un sous-dossier.
    let with_testdisk: Vec<PathBuf> = dirs.iter().map(|d| d.join("testdisk")).collect();
    dirs.extend(with_testdisk);
    dirs
}

/// Racine de la clé : parent de `windows/` ou `linux/` (disposition du plan), sinon le dossier de
/// l'exécutable (développement). Les rapports et les récupérations y sont rangés.
fn usb_root() -> PathBuf {
    let base = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    match base.file_name().and_then(|n| n.to_str()) {
        Some("windows" | "linux") => base.parent().map(Path::to_path_buf).unwrap_or(base),
        _ => base,
    }
}

fn reports_dir() -> PathBuf {
    usb_root().join("rapports")
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
            app.manage(Arc::new(Jobs::default()));
            app.manage(Cache::default());
            app.manage(Arc::new(terminal::Terminals::default()));
            app.manage(AppState {
                smartctl,
                self_test,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            scan_disks,
            start_smart_test,
            abort_smart_test,
            smart_test_status,
            job_state,
            cancel_job,
            start_surface_scan,
            start_capacity_test,
            phone::phone_devices,
            phone::phone_collect,
            machine::machine_inventory,
            machine::start_cpu_stress,
            machine::start_ram_test,
            machine::gpu_sensors_now,
            machine::record_gpu_test,
            recover::recovery_status,
            recover::recovery_trim_warning,
            recover::start_recovery,
            recover::recovery_found,
            recover::open_folder,
            recover::tsk_list,
            recover::start_tsk,
            recover::tsk_recover_selected,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::open_console_window,
            reports::save_disk_report,
            reports::save_phone_report,
            reports::save_machine_report,
            reports::preview_machine_report,
            reports::save_recovery_report,
            reports::list_reports,
            reports::open_report_file,
            self_test_report
        ])
        .build(tauri::generate_context!());

    let app = match result {
        Ok(app) => app,
        Err(e) => {
            eprintln!("échec du démarrage de l'application : {e}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        // Fermeture : on annule les tâches et on leur laisse le temps de nettoyer
        // (fichiers du test de capacité, processus PhotoRec).
        if let tauri::RunEvent::Exit = event {
            if let Some(jobs) = handle.try_state::<Arc<Jobs>>() {
                jobs.cancel_all();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while jobs.any_running() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    });
}
