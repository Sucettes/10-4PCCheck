//! Lecture du journal `photorec.log` (option `/log`).
//!
//! Lignes utilisées, relevées dans les sources de PhotoRec
//! (https://github.com/cgsecurity/testdisk/tree/master/src) :
//! - `Command line: PhotoRec ...` (phmain.c) : début d'une exécution (le journal est en ajout) ;
//! - `Pass N (blocksize=B) ...` (phrecn.c) : passe en cours ;
//! - `Elapsed time 0h01m02s` (phrecn.c) ;
//! - `jpg: 12/14 recovered` et `Total: N files found` (photorec.c, `write_stats_log`) ;
//! - `PhotoRec exited normally.` (phmain.c) ;
//! - lignes d'erreur : `Syntax error...` (phcli.c), `Cannot write to file...` (phrecn.c),
//!   `SIGTERM detected!...` (phmain.c).
//!
//! Le comptage principal se fait sur les dossiers `recup_dir.N` ; le journal complète
//! (passe, erreurs, fin normale).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

/// Au plus cette quantité lue à la fin du journal : il contient une ligne par fichier récupéré
/// et peut peser plusieurs Mo.
pub const LOG_TAIL_BYTES: u64 = 256 * 1024;

const RUN_MARKER: &str = "Command line: PhotoRec";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LogSummary {
    /// Dernière passe commencée (PhotoRec en fait plusieurs).
    pub pass: Option<u32>,
    pub elapsed_s: Option<u64>,
    /// Total du dernier bilan `Total: N files found`.
    pub total_found: Option<u64>,
    /// Dernier bilan par format : identifiant PhotoRec → (récupérés, trouvés).
    pub recovered_by_format: BTreeMap<String, (u64, u64)>,
    /// `PhotoRec exited normally.` vu.
    pub finished_normally: bool,
    /// Lignes d'erreur notables, dans l'ordre.
    pub errors: Vec<String>,
}

/// Analyse le texte du journal. Seule la dernière exécution compte (après le dernier
/// `Command line: PhotoRec`), le fichier étant ouvert en ajout.
pub fn parse_log(text: &str) -> LogSummary {
    let last_run = text.rfind(RUN_MARKER).map_or(text, |i| &text[i..]);
    let mut s = LogSummary::default();
    for line in last_run.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("Pass ") {
            if let Some(n) = rest.split_whitespace().next().and_then(|n| n.parse().ok()) {
                // « Pass 1 +3 files » (bilan) répète la passe : même valeur, sans effet.
                s.pass = Some(n);
            }
        } else if let Some(rest) = line.strip_prefix("Elapsed time ") {
            s.elapsed_s = parse_elapsed(rest).or(s.elapsed_s);
        } else if let Some(rest) = line.strip_prefix("Total: ") {
            s.total_found = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .or(s.total_found);
        } else if line == "PhotoRec exited normally." {
            s.finished_normally = true;
        } else if is_error_line(line) {
            s.errors.push(line.to_string());
        } else if let Some((format, counts)) = parse_stat_line(line) {
            s.recovered_by_format.insert(format, counts);
        }
    }
    s
}

/// Lit la fin du journal à partir de `from_offset` (taille du fichier au lancement : les
/// exécutions précédentes dans le même dossier sont ignorées), au plus `LOG_TAIL_BYTES`.
pub fn read_log_tail(path: &Path, from_offset: u64) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = from_offset.max(len.saturating_sub(LOG_TAIL_BYTES)).min(len);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    // Noms de fichiers dans la page de code locale sous Windows : remplacement sans échec.
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// `0h01m02s` → 62.
fn parse_elapsed(s: &str) -> Option<u64> {
    let (h, rest) = s.trim().split_once('h')?;
    let (m, rest) = rest.split_once('m')?;
    let sec = rest.strip_suffix('s')?;
    Some(h.parse::<u64>().ok()? * 3600 + m.parse::<u64>().ok()? * 60 + sec.parse::<u64>().ok()?)
}

/// `jpg: 12/14 recovered` → ("jpg", (12, 14)).
fn parse_stat_line(line: &str) -> Option<(String, (u64, u64))> {
    let rest = line.strip_suffix(" recovered")?;
    let (format, counts) = rest.rsplit_once(": ")?;
    let (ok, total) = counts.split_once('/')?;
    Some((format.to_string(), (ok.parse().ok()?, total.parse().ok()?)))
}

fn is_error_line(line: &str) -> bool {
    const PATTERNS: [&str; 5] = [
        "Syntax error",
        "Cannot write to file",
        "detected! PhotoRec has been killed",
        "No space left",
        "Unable to open",
    ];
    PATTERNS.iter().any(|p| line.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_and_stats_lines() {
        assert_eq!(parse_elapsed("0h01m02s"), Some(62));
        assert_eq!(parse_elapsed("12h00m00s"), Some(43_200));
        assert_eq!(parse_elapsed("n/a"), None);
        assert_eq!(
            parse_stat_line("jpg: 12/14 recovered"),
            Some(("jpg".into(), (12, 14)))
        );
        assert_eq!(
            parse_stat_line("tx?: 1/1 recovered"),
            Some(("tx?".into(), (1, 1)))
        );
        assert_eq!(parse_stat_line("Total: 3 files found"), None);
    }
}
