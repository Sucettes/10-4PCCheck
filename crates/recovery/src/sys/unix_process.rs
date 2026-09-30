//! Unix : PhotoRec lancé avec `std::process::Command`.
//!
//! PhotoRec démarre toujours ncurses, même en mode `/cmd` (phmain.c, `start_ncurses`).
//! Sous Linux, ncurses (`newterm(NULL, stdout, stdin)`, intrfn.c) n'exige pas un vrai
//! terminal, seulement une description terminfo : on redirige les flux vers /dev/null et on
//! fixe `TERM=linux` si l'environnement n'en donne pas (appli lancée depuis le bureau).
//! PhotoRec essaie d'ailleurs lui-même `linux` en repli. À valider avec photorec_static ;
//! repli si besoin : un pseudo-terminal (`openpty`) dont on vide la sortie.

use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use pccheck_core::process::hide_console;

use crate::error::RecoveryError;

pub(crate) struct ChildProcess {
    child: Child,
}

impl ChildProcess {
    pub(crate) fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<Self, RecoveryError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Groupe de processus à part : au second signal, PhotoRec fait `kill(0, sig)`
            // (phmain.c, `sighup_hdlr`), qui tuerait sinon l'application avec lui.
            .process_group(0);
        let term_missing = match std::env::var("TERM") {
            Ok(t) => t.is_empty() || t == "dumb",
            Err(_) => true,
        };
        if term_missing {
            cmd.env("TERM", "linux");
        }
        hide_console(&mut cmd);
        let child = cmd.spawn().map_err(|e| RecoveryError::Spawn {
            path: program.to_path_buf(),
            reason: e.to_string(),
        })?;
        Ok(ChildProcess { child })
    }

    /// Code de sortie si le processus est terminé ; tué par un signal : `-numéro_du_signal`.
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(self
            .child
            .try_wait()?
            .map(|s| s.code().unwrap_or_else(|| -s.signal().unwrap_or(0))))
    }

    /// Arrêt propre : SIGTERM. PhotoRec le capte, note l'arrêt dans son journal, ferme le
    /// fichier en cours et s'arrête (phmain.c, `sighup_hdlr` met `need_to_stop`).
    pub(crate) fn request_stop(&mut self) {
        if let Ok(pid) = libc::pid_t::try_from(self.child.id()) {
            // SAFETY : simple envoi de signal à notre enfant ; échec ignoré (déjà terminé).
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }

    pub(crate) fn kill(&mut self) {
        // SIGKILL ; échec ignoré : le processus peut être déjà terminé.
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(child: &mut ChildProcess, max: Duration) -> Option<i32> {
        let start = Instant::now();
        while start.elapsed() < max {
            if let Some(code) = child.try_wait().unwrap() {
                return Some(code);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn exit_code_and_sigterm() {
        let dir = std::env::temp_dir();
        let args = vec!["-c".to_string(), "exit 3".to_string()];
        let mut c = ChildProcess::spawn(Path::new("/bin/sh"), &args, &dir).unwrap();
        assert_eq!(wait(&mut c, Duration::from_secs(5)), Some(3));

        let args = vec!["-c".to_string(), "sleep 30".to_string()];
        let mut c = ChildProcess::spawn(Path::new("/bin/sh"), &args, &dir).unwrap();
        assert_eq!(c.try_wait().unwrap(), None);
        c.request_stop();
        assert_eq!(wait(&mut c, Duration::from_secs(5)), Some(-libc::SIGTERM));
    }
}
