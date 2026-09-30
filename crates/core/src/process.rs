//! Lancement d'outils externes (smartctl, adb, dsregcmd...) avec délai maximal et sortie capturée.
//! Jamais de shell : le programme et ses arguments sont passés séparément.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Error, Serialize, PartialEq, Eq)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ProcessError {
    #[error("{name} introuvable (cherché dans : {searched:?})")]
    NotFound {
        name: String,
        searched: Vec<PathBuf>,
    },
    #[error("impossible de lancer {path:?} : {reason}")]
    Spawn { path: PathBuf, reason: String },
    #[error("{path:?} n'a pas répondu en {seconds} s ({args})")]
    Timeout {
        path: PathBuf,
        seconds: u64,
        args: String,
    },
}

/// Sortie d'un programme terminé. Le code de sortie n'est pas une erreur ici : certains outils
/// (smartctl) s'en servent comme masque d'informations.
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub stdout: String,
    /// `None` si le processus a été tué par un signal.
    pub code: Option<i32>,
}

/// Cherche `file_name` : d'abord le chemin donné par la variable `env_var` s'il existe, puis
/// chaque dossier, dans l'ordre. Pas de repli sur le PATH : on veut la version embarquée sur la clé.
pub fn locate(env_var: &str, dirs: &[PathBuf], file_name: &str) -> Result<PathBuf, ProcessError> {
    locate_with(
        std::env::var_os(env_var).map(PathBuf::from),
        dirs,
        file_name,
    )
}

pub(crate) fn locate_with(
    env: Option<PathBuf>,
    dirs: &[PathBuf],
    file_name: &str,
) -> Result<PathBuf, ProcessError> {
    let mut searched = Vec::new();
    for candidate in env
        .into_iter()
        .chain(dirs.iter().map(|d| d.join(file_name)))
    {
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(candidate);
    }
    Err(ProcessError::NotFound {
        name: file_name.to_string(),
        searched,
    })
}

/// Lance `path args` et attend sa fin, au plus `timeout`. La sortie d'erreur est ignorée.
pub fn run(path: &Path, args: &[&str], timeout: Duration) -> Result<ProcessOutput, ProcessError> {
    let mut cmd = Command::new(path);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console(&mut cmd);
    let spawn_err = |reason: String| ProcessError::Spawn {
        path: path.to_path_buf(),
        reason,
    };
    let mut child = cmd.spawn().map_err(|e| spawn_err(e.to_string()))?;

    // Lecture dans un fil séparé : un tuyau plein bloquerait le programme avant sa fin.
    let mut stdout = child.stdout.take().expect("stdout configuré en pipe");
    let reader = thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).map(|_| buf)
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                // Échec du kill ignoré à dessein : le processus peut s'être terminé entre-temps.
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProcessError::Timeout {
                    path: path.to_path_buf(),
                    seconds: timeout.as_secs(),
                    args: args.join(" "),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(spawn_err(e.to_string())),
        }
    };

    let bytes = match reader.join() {
        Ok(Ok(buf)) => buf,
        Ok(Err(e)) => return Err(spawn_err(e.to_string())),
        Err(_) => return Err(spawn_err("le fil de lecture de la sortie a paniqué".into())),
    };
    Ok(ProcessOutput {
        // Sortie non UTF-8 (page de code OEM de certains outils Windows) : caractères remplacés.
        stdout: String::from_utf8_lossy(&bytes).into_owned(),
        code: status.code(),
    })
}

/// Pas de fenêtre de console qui clignote sous Windows quand l'app lance un outil.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_reports_every_searched_path() {
        let dirs = vec![
            PathBuf::from("/inexistant/a"),
            PathBuf::from("/inexistant/b"),
        ];
        match locate_with(None, &dirs, "outil") {
            Err(ProcessError::NotFound { searched, name }) => {
                assert_eq!(searched.len(), 2);
                assert_eq!(name, "outil");
            }
            other => panic!("attendu NotFound, obtenu {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn run_times_out_and_kills_process() {
        let start = Instant::now();
        let r = run(Path::new("/bin/sleep"), &["5"], Duration::from_millis(300));
        assert!(matches!(r, Err(ProcessError::Timeout { .. })));
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(windows)]
    #[test]
    fn run_captures_stdout_and_exit_code() {
        let out = run(
            Path::new("C:\\Windows\\System32\\cmd.exe"),
            &["/C", "echo bonjour & exit 3"],
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(out.stdout.contains("bonjour"));
        assert_eq!(out.code, Some(3));
    }
}
