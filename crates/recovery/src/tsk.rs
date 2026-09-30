//! Récupération par le système de fichiers avec The Sleuth Kit (https://sleuthkit.org) : complément de
//! PhotoRec. Au lieu de chercher des signatures dans les données brutes (noms perdus), on lit les
//! entrées de fichiers supprimés que le système de fichiers garde (MFT de NTFS, répertoires FAT,
//! inodes ext) : **noms et dossiers conservés**, tant que ces entrées n'ont pas été réutilisées.
//!
//! - `fls -r -d -p -l <volume>` : liste des fichiers supprimés (chemin, taille, date).
//! - `tsk_recover <volume> <dossier>` : copie des fichiers supprimés (non alloués) dans le dossier.
//!
//! Source : un VOLUME (partition avec système de fichiers), pas un disque entier : `\\.\E:` sous
//! Windows, `/dev/sdb1` sous Linux.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pccheck_core::process::{self, hide_console};
use serde::Serialize;

use crate::disk::validate_destination;
use crate::error::RecoveryError;
use crate::volumes::list_volumes;

/// Variable d'environnement qui force le chemin de `tsk_recover` (tests, développement).
pub const ENV_OVERRIDE: &str = "PCCHECK_TSK";
#[cfg(windows)]
const RECOVER: &str = "tsk_recover.exe";
#[cfg(not(windows))]
const RECOVER: &str = "tsk_recover";
#[cfg(windows)]
const FLS: &str = "fls.exe";
#[cfg(not(windows))]
const FLS: &str = "fls";
/// La liste parcourt toute la table des fichiers : plusieurs minutes sur un gros volume NTFS.
const LIST_TIMEOUT: Duration = Duration::from_secs(600);
/// Au-delà, la liste est tronquée (l'interface n'affiche pas des centaines de milliers de lignes).
pub const LIST_LIMIT: usize = 20_000;

/// Fichier supprimé vu par `fls`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeletedFile {
    /// Numéro d'entrée dans le système de fichiers (inode, entrée MFT).
    pub inode: String,
    /// Chemin dans le volume (`Photos/Vacances/IMG_1234.JPG`). Sous FAT, la première lettre du
    /// nom est remplacée par `_` : FAT l'efface à la suppression.
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// Date de modification telle qu'écrite par `fls`, `None` si inconnue.
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeletedList {
    pub files: Vec<DeletedFile>,
    pub total: usize,
    pub truncated: bool,
}

/// Dossier contenant `tsk_recover` et `fls` (livrés ensemble avec leurs DLL, dans `sleuthkit/`).
pub fn locate_tsk(dirs: &[PathBuf]) -> Result<PathBuf, RecoveryError> {
    let sub: Vec<PathBuf> = dirs.iter().map(|d| d.join("sleuthkit")).collect();
    match process::locate(ENV_OVERRIDE, &sub, RECOVER) {
        Ok(exe) => Ok(exe.parent().map(Path::to_path_buf).unwrap_or_default()),
        Err(process::ProcessError::NotFound { searched, .. }) => {
            Err(RecoveryError::TskNotFound { searched })
        }
        Err(e) => Err(e.into()),
    }
}

/// Chemin de volume à donner à The Sleuth Kit pour la racine d'un volume monté.
/// Windows : `E:\` → `\\.\E:`. Linux : périphérique du point de montage (`/media/cle` → `/dev/sdb1`).
pub fn volume_device(mount: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        let s = mount.to_str()?;
        let letter = s.chars().next().filter(|c| c.is_ascii_alphabetic())?;
        (s.len() <= 3 && s[1..].starts_with(':'))
            .then(|| format!(r"\\.\{}:", letter.to_ascii_uppercase()))
    }
    #[cfg(not(windows))]
    {
        let text = std::fs::read_to_string("/proc/mounts").ok()?;
        crate::volumes::parse_mounts(&text)
            .into_iter()
            .find(|m| m.mount_point == mount)
            .map(|m| m.device)
    }
}

/// Liste les fichiers supprimés d'un volume (`device` : résultat de `volume_device`).
pub fn list_deleted(tsk_dir: &Path, device: &str) -> Result<DeletedList, RecoveryError> {
    let out = process::run(
        &tsk_dir.join(FLS),
        &["-r", "-d", "-p", "-l", device],
        LIST_TIMEOUT,
    )?;
    if out.code != Some(0) && out.stdout.trim().is_empty() {
        return Err(RecoveryError::ToolFailed {
            tool: "fls".into(),
            message:
                "lecture du système de fichiers impossible (volume chiffré BitLocker, système de \
                      fichiers non reconnu ou droits administrateur manquants)"
                    .into(),
        });
    }
    let files = parse_fls(&out.stdout);
    let total = files.len();
    Ok(DeletedList {
        truncated: total > LIST_LIMIT,
        files: files.into_iter().take(LIST_LIMIT).collect(),
        total,
    })
}

/// Analyse `fls -r -d -p -l` : `r/r * 4:<TAB>chemin<TAB>mtime<TAB>atime<TAB>ctime<TAB>crtime<TAB>taille<TAB>uid<TAB>gid`.
/// Les métafichiers (`$MBR`, `$FAT1`, `$OrphanFiles`, type `v/v`) sont ignorés.
pub fn parse_fls(text: &str) -> Vec<DeletedFile> {
    text.lines()
        .filter_map(|line| {
            let (head, rest) = line.split_once('\t')?;
            let mut head_parts = head.split_whitespace();
            let kind = head_parts.next()?;
            if kind.starts_with('v') {
                return None;
            }
            let inode = head_parts
                .find(|p| *p != "*")?
                .trim_end_matches(':')
                .to_string();
            let cols: Vec<&str> = rest.split('\t').collect();
            let path = cols.first()?.to_string();
            if path.is_empty() || path.starts_with('$') {
                return None;
            }
            let modified = cols
                .get(1)
                .map(|d| d.split(" (").next().unwrap_or(d).to_string())
                .filter(|d| !d.starts_with("0000"));
            Some(DeletedFile {
                inode,
                is_dir: kind.starts_with('d'),
                size: cols.get(5).and_then(|s| s.trim().parse().ok()),
                modified,
                path,
            })
        })
        .collect()
}

/// `Files Recovered: 12` à la fin de la sortie de `tsk_recover`.
pub fn parse_recovered_count(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Files Recovered:"))
        .and_then(|n| n.trim().parse().ok())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TskProgress {
    pub running: bool,
    pub elapsed_s: u64,
    pub files_found: u64,
    pub bytes_found: u64,
    /// Nombre annoncé par `tsk_recover` à la fin.
    pub reported: Option<u64>,
    pub exit_code: Option<i32>,
    pub stopped_by_user: bool,
}

/// `tsk_recover` en cours sur un volume.
pub struct TskJob {
    child: Child,
    dest: PathBuf,
    started: Instant,
    output: std::sync::Arc<std::sync::Mutex<String>>,
    exit_code: Option<i32>,
    stopped_by_user: bool,
}

impl TskJob {
    /// Lance la récupération des fichiers supprimés du volume `mount` (racine, ex. `E:\`) vers
    /// `dest`, qui doit être sur un autre disque physique.
    pub fn start(tsk_dir: &Path, mount: &Path, dest: &Path) -> Result<TskJob, RecoveryError> {
        let device = volume_device(mount).ok_or_else(|| RecoveryError::InvalidDestination {
            path: mount.to_path_buf(),
            reason: "volume introuvable".into(),
        })?;
        let source_disk = list_volumes()
            .into_iter()
            .find(|v| v.path == mount)
            .and_then(|v| v.disk)
            .ok_or_else(|| RecoveryError::DestinationDiskUnknown {
                path: mount.to_path_buf(),
                reason: "disque physique du volume source inconnu".into(),
            })?;
        validate_destination(&source_disk, dest)?;
        std::fs::create_dir_all(dest).map_err(|e| RecoveryError::CreateDestination {
            path: dest.to_path_buf(),
            reason: e.to_string(),
        })?;
        Self::spawn(&tsk_dir.join(RECOVER), &[device], dest)
    }

    /// Lancement brut (tests : image disque au lieu d'un volume).
    pub fn spawn(program: &Path, source: &[String], dest: &Path) -> Result<TskJob, RecoveryError> {
        let mut cmd = Command::new(program);
        cmd.args(source)
            .arg(dest)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_console(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| RecoveryError::Spawn {
            path: program.to_path_buf(),
            reason: e.to_string(),
        })?;
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        if let Some(mut stdout) = child.stdout.take() {
            let sink = output.clone();
            std::thread::spawn(move || {
                let mut buf = String::new();
                let _ = std::io::Read::read_to_string(&mut stdout, &mut buf);
                if let Ok(mut s) = sink.lock() {
                    *s = buf;
                }
            });
        }
        Ok(TskJob {
            child,
            dest: dest.to_path_buf(),
            started: Instant::now(),
            output,
            exit_code: None,
            stopped_by_user: false,
        })
    }

    pub fn progress(&mut self) -> TskProgress {
        if self.exit_code.is_none() {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exit_code = Some(status.code().unwrap_or(-1));
            }
        }
        let (files_found, bytes_found) = count_files(&self.dest);
        let reported = self
            .output
            .lock()
            .ok()
            .and_then(|s| parse_recovered_count(&s));
        TskProgress {
            running: self.exit_code.is_none(),
            elapsed_s: self.started.elapsed().as_secs(),
            files_found,
            bytes_found,
            reported,
            exit_code: self.exit_code,
            stopped_by_user: self.stopped_by_user,
        }
    }

    pub fn stop(&mut self) {
        if self.exit_code.is_none() {
            self.stopped_by_user = true;
            let _ = self.child.kill();
            self.exit_code = self.child.wait().ok().map(|s| s.code().unwrap_or(-1));
        }
    }
}

impl Drop for TskJob {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Fichiers et octets sous `dir`, récursivement.
pub fn count_files(dir: &Path) -> (u64, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(_) => {
                    files += 1;
                    bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
                Err(_) => {}
            }
        }
    }
    (files, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sortie réelle de `fls -r -d -p -l` sur l'image de tools/dev/make-fat-image.py.
    const FLS: &str = "r/r * 4:\t_HOTO.PNG\t2025-09-01 00:00:00 (Eastern Daylight Time)\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t8455\t0\t0\n\
r/r * 5:\t_APPORT.PDF\t2025-09-01 00:00:00 (Eastern Daylight Time)\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t142\t0\t0\n\
d/d * 12:\tPhotos/Vacances\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t0000-00-00 00:00:00 (UTC)\t0\t0\t0\n\
v/v 523251:\t$MBR\t0000-00-00 00:00:00 (UTC)\t\t\t\t512\t0\t0\n";

    #[test]
    fn fls_output_is_parsed() {
        let files = parse_fls(FLS);
        assert_eq!(files.len(), 3, "métafichier $MBR ignoré");
        assert_eq!(files[0].path, "_HOTO.PNG");
        assert_eq!(files[0].inode, "4");
        assert_eq!(files[0].size, Some(8455));
        assert_eq!(files[0].modified.as_deref(), Some("2025-09-01 00:00:00"));
        assert!(files[2].is_dir);
        assert_eq!(files[2].modified, None, "date nulle écartée");
    }

    #[test]
    fn recovered_count_is_read() {
        assert_eq!(parse_recovered_count("Files Recovered: 2\n"), Some(2));
        assert_eq!(parse_recovered_count("rien"), None);
    }

    #[cfg(windows)]
    #[test]
    fn drive_letters_map_to_volume_devices() {
        assert_eq!(volume_device(Path::new(r"E:\")).as_deref(), Some(r"\\.\E:"));
        assert_eq!(volume_device(Path::new("d:")).as_deref(), Some(r"\\.\D:"));
        assert_eq!(volume_device(Path::new(r"E:\dossier")), None);
    }
}
