//! Console interactive pour TestDisk (et PhotoRec en mode manuel) dans un pseudo-terminal.
//!
//! TestDisk est un programme plein écran (ncurses) piloté au clavier : impossible à scripter
//! proprement, mais parfait à la main. On le lance dans un pseudo-terminal (ConPTY sous Windows,
//! openpty sous Linux, via `portable-pty`) : le programme croit parler à une vraie console, et
//! l'application relaie l'écran et le clavier vers un émulateur de terminal dans l'interface.
//!
//! Sécurité : seuls les outils de `ConsoleTool` peuvent être lancés, jamais un shell ; sinon la
//! vue web, qui tourne en administrateur, pourrait exécuter n'importe quelle commande.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::error::RecoveryError;

/// Outils autorisés dans la console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleTool {
    Testdisk,
    Photorec,
}

impl ConsoleTool {
    pub fn binary_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (ConsoleTool::Testdisk, true) => "testdisk_win.exe",
            (ConsoleTool::Photorec, true) => "photorec_win.exe",
            (ConsoleTool::Testdisk, false) => "testdisk_static",
            (ConsoleTool::Photorec, false) => "photorec_static",
        }
    }
}

/// Programme lancé dans un pseudo-terminal. La lecture de l'écran se fait sur le lecteur rendu
/// par `spawn`, dans un fil séparé (lecture bloquante).
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

fn pty_err(program: &Path, e: impl std::fmt::Display) -> RecoveryError {
    RecoveryError::Spawn {
        path: program.to_path_buf(),
        reason: e.to_string(),
    }
}

impl PtySession {
    /// Lance `program args` dans un pseudo-terminal de `cols` x `rows`, dossier courant `cwd`
    /// (TestDisk y écrit `testdisk.log`). `env` : variables ajoutées (tests).
    pub fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
        env: &[(&str, &str)],
    ) -> Result<(PtySession, Box<dyn Read + Send>), RecoveryError> {
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|e| pty_err(program, e))?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);
        // ncurses choisit ses séquences d'échappement selon TERM : xterm.js les comprend toutes.
        cmd.env("TERM", "xterm-256color");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| pty_err(program, e))?;
        // Le côté « esclave » appartient maintenant au programme : le garder ouvert empêcherait de
        // détecter sa fin (la lecture ne rendrait jamais EOF).
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| pty_err(program, e))?;
        let writer = pair.master.take_writer().map_err(|e| pty_err(program, e))?;
        Ok((
            PtySession {
                master: pair.master,
                writer,
                child,
            },
            reader,
        ))
    }

    /// Envoie des frappes clavier (séquences d'échappement comprises, telles que produites par
    /// l'émulateur de terminal).
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        // Échec ignoré : la fenêtre garde simplement l'ancienne taille.
        let _ = self.master.resize(size(cols, rows));
    }

    /// Code de sortie si le programme est terminé.
    pub fn try_wait(&mut self) -> Option<u32> {
        self.child.try_wait().ok().flatten().map(|s| s.exit_code())
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if self.try_wait().is_none() {
            self.kill();
        }
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.clamp(10, 200),
        cols: cols.clamp(40, 400),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Découpe un flux d'octets en texte UTF-8 sans casser les caractères coupés entre deux lectures :
/// les octets d'un caractère incomplet attendent la lecture suivante.
#[derive(Default)]
pub struct Utf8Stream {
    carry: Vec<u8>,
}

impl Utf8Stream {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.carry.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.carry) {
                Ok(s) => {
                    out.push_str(s);
                    self.carry.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // SAFETY non requis : from_utf8 vient de valider cette partie.
                    out.push_str(std::str::from_utf8(&self.carry[..valid]).unwrap_or_default());
                    match e.error_len() {
                        // Caractère incomplet en fin de tampon : on le garde pour la suite.
                        None => {
                            self.carry.drain(..valid);
                            return out;
                        }
                        // Octets invalides : remplacés, puis on continue.
                        Some(n) => {
                            out.push('\u{FFFD}');
                            self.carry.drain(..valid + n);
                        }
                    }
                }
            }
        }
    }
}

/// Binaire de l'outil dans les dossiers d'outils (`testdisk/` compris).
pub fn locate_console_tool(tool: ConsoleTool, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|d| [d.join("testdisk"), d.clone()])
        .map(|d| d.join(tool.binary_name()))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_across_reads_is_rebuilt() {
        let text = "Récupération ✓";
        let bytes = text.as_bytes();
        let mut s = Utf8Stream::default();
        // Coupe au milieu du « é » (2 octets) puis au milieu du « ✓ » (3 octets).
        let a = s.push(&bytes[..2]);
        let b = s.push(&bytes[2..bytes.len() - 1]);
        let c = s.push(&bytes[bytes.len() - 1..]);
        assert_eq!(format!("{a}{b}{c}"), text);
        assert_eq!(a, "R");
    }

    #[test]
    fn invalid_bytes_are_replaced_not_stuck() {
        let mut s = Utf8Stream::default();
        assert_eq!(s.push(b"a\xFFb"), "a\u{FFFD}b");
        assert_eq!(s.push(b"c"), "c");
    }

    #[test]
    fn sizes_are_clamped() {
        let p = size(5, 1000);
        assert_eq!((p.cols, p.rows), (40, 200));
    }
}
