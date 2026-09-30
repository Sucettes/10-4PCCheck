//! Console intégrée : TestDisk (ou PhotoRec manuel) dans un pseudo-terminal, affiché par xterm.js.
//!
//! - `terminal_open` lance l'outil et rend un identifiant de session ;
//! - `terminal-output` { id, data } : texte à écrire dans l'émulateur (séquences d'échappement comprises) ;
//! - `terminal-exit` { id, code } : l'outil s'est terminé ;
//! - `terminal_write` / `terminal_resize` / `terminal_close` : clavier, taille, arrêt.
//!
//! Seuls les outils de `ConsoleTool` sont lancés, jamais un shell (voir `pccheck_recovery::console`).

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use pccheck_recovery::{locate_console_tool, ConsoleTool, PtySession, Utf8Stream};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{tool_dirs, usb_root, CommandError};

#[derive(Default)]
pub struct Terminals {
    sessions: Mutex<HashMap<u32, PtySession>>,
    next: AtomicU32,
}

impl Terminals {
    /// Arrête toutes les consoles (fermeture de l'application).
    pub fn close_all(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for (_, mut s) in sessions.drain() {
                s.kill();
            }
        }
    }
}

/// Intervalle de vérification de la fin de l'outil.
const WATCH_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Clone, Serialize)]
struct Output<'a> {
    id: u32,
    data: &'a str,
}

#[derive(Clone, Serialize)]
struct Exit {
    id: u32,
    code: Option<u32>,
}

fn not_found(tool: ConsoleTool) -> CommandError {
    CommandError::Tool(format!(
        "{} introuvable dans le dossier tools de la clé : lance tools/fetch-tools-windows.ps1 puis tools/assemble-usb.ps1",
        tool.binary_name()
    ))
}

/// Dossier de travail des outils : `recup/` de la clé (TestDisk y écrit `testdisk.log`).
fn workdir() -> std::path::PathBuf {
    let dir = usb_root().join("recup");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[tauri::command]
pub fn terminal_open(
    tool: ConsoleTool,
    cols: u16,
    rows: u16,
    app: AppHandle,
    terms: State<'_, Arc<Terminals>>,
) -> Result<u32, CommandError> {
    let exe = locate_console_tool(tool, &tool_dirs(&app)).ok_or_else(|| not_found(tool))?;
    let (session, mut reader) = PtySession::spawn(&exe, &[], &workdir(), cols, rows, &[])
        .map_err(|e| CommandError::Tool(e.to_string()))?;
    let id = terms.next.fetch_add(1, Ordering::Relaxed) + 1;
    terms
        .sessions
        .lock()
        .map_err(|_| CommandError::Internal("sessions illisibles".into()))?
        .insert(id, session);

    let terms = terms.inner().clone();
    let exit_code = Arc::new(Mutex::new(None::<u32>));

    // Surveillance de la fin de l'outil. Sous Windows, la sortie du pseudo-terminal (ConPTY) ne
    // se ferme qu'à la libération de la session : sans ce fil, la lecture ci-dessous attendrait
    // indéfiniment une fin qui dépend d'elle. La session libérée, la lecture reçoit la fin.
    {
        let terms = terms.clone();
        let exit_code = exit_code.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(WATCH_EVERY);
            let Ok(mut sessions) = terms.sessions.lock() else {
                return;
            };
            let Some(session) = sessions.get_mut(&id) else {
                return; // fermée par `terminal_close` ou par la lecture
            };
            if let Some(code) = session.try_wait() {
                *exit_code.lock().unwrap_or_else(|p| p.into_inner()) = Some(code);
                let session = sessions.remove(&id);
                drop(sessions);
                drop(session);
                return;
            }
        });
    }

    std::thread::spawn(move || {
        let mut utf8 = Utf8Stream::default();
        let mut buf = [0u8; 8192];
        // Lecture bloquante jusqu'à la fin de la sortie (EOF) ou une erreur.
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            let text = utf8.push(&buf[..n]);
            if !text.is_empty() {
                let _ = app.emit("terminal-output", Output { id, data: &text });
            }
        }
        // Linux : la fin de sortie arrive d'abord, la session est encore là.
        let from_session = terms
            .sessions
            .lock()
            .ok()
            .and_then(|mut s| s.remove(&id))
            .and_then(|mut s| s.try_wait());
        let code = from_session.or(*exit_code.lock().unwrap_or_else(|p| p.into_inner()));
        let _ = app.emit("terminal-exit", Exit { id, code });
    });
    Ok(id)
}

fn with_session<T>(
    terms: &Terminals,
    id: u32,
    f: impl FnOnce(&mut PtySession) -> T,
) -> Result<T, CommandError> {
    let mut sessions = terms
        .sessions
        .lock()
        .map_err(|_| CommandError::Internal("sessions illisibles".into()))?;
    let session = sessions
        .get_mut(&id)
        .ok_or_else(|| CommandError::Internal("session terminée".into()))?;
    Ok(f(session))
}

#[tauri::command]
pub fn terminal_write(
    id: u32,
    data: String,
    terms: State<'_, Arc<Terminals>>,
) -> Result<(), CommandError> {
    with_session(&terms, id, |s| s.write(data.as_bytes()))?
        .map_err(|e| CommandError::Internal(format!("écriture impossible : {e}")))
}

#[tauri::command]
pub fn terminal_resize(
    id: u32,
    cols: u16,
    rows: u16,
    terms: State<'_, Arc<Terminals>>,
) -> Result<(), CommandError> {
    with_session(&terms, id, |s| s.resize(cols, rows))
}

/// Arrête l'outil (fermeture de l'onglet). Sans effet si la session est déjà terminée.
#[tauri::command]
pub fn terminal_close(id: u32, terms: State<'_, Arc<Terminals>>) {
    if let Ok(mut sessions) = terms.sessions.lock() {
        if let Some(mut s) = sessions.remove(&id) {
            s.kill();
        }
    }
}

/// Solution de secours : l'outil dans une fenêtre de console séparée, comme lancé à la main.
#[tauri::command]
pub fn open_console_window(tool: ConsoleTool, app: AppHandle) -> Result<(), CommandError> {
    let exe = locate_console_tool(tool, &tool_dirs(&app)).ok_or_else(|| not_found(tool))?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(workdir());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }
    #[cfg(not(windows))]
    {
        // Linux : l'émulateur de terminal par défaut de la distribution (Debian, Ubuntu, Mint).
        cmd = {
            // Politique Debian : `-e` prend le programme et ses arguments séparés, sans shell
            // (xterm, gnome-terminal.wrapper) : un chemin avec espaces passe tel quel.
            let mut t = std::process::Command::new("x-terminal-emulator");
            t.arg("-e").arg(&exe).current_dir(workdir());
            t
        };
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| CommandError::Tool(format!("ouverture de la console impossible : {e}")))
}
