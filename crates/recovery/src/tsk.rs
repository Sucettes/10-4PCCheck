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

use crate::disk::{locate_destination, validate_destination, DestinationLocation};
use crate::error::RecoveryError;

/// Variable d'environnement qui force le chemin de `tsk_recover` (tests, développement).
pub const ENV_OVERRIDE: &str = "PCCHECK_TSK";
#[cfg(windows)]
const RECOVER: &str = "tsk_recover.exe";
#[cfg(not(windows))]
const RECOVER: &str = "tsk_recover";
#[cfg(windows)]
const ICAT: &str = "icat.exe";
#[cfg(not(windows))]
const ICAT: &str = "icat";
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
/// Ignorés : les métafichiers (type `v/v`, `$MFT`, `$FAT1`...) et les entrées `(realloc)`, dont
/// le numéro sert déjà à un autre fichier (icat rendrait le contenu de celui-ci). Gardés : la
/// Corbeille vidée (`$RECYCLE.BIN/...`) et les orphelins FAT (`$OrphanFiles/...`).
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
            if !valid_inode(&inode) {
                return None;
            }
            let cols: Vec<&str> = rest.split('\t').collect();
            let path = cols.first()?.to_string();
            if path.is_empty() || is_metafile(&path) {
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

/// Adresse de métadonnée acceptée par icat : `123`, `123-128` ou `123-128-4` (NTFS). Tout le
/// reste est refusé, dont `123(realloc)` et un `-x` qui serait lu comme une option.
fn valid_inode(inode: &str) -> bool {
    let parts: Vec<&str> = inode.split('-').collect();
    parts.len() <= 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Métafichier du système de fichiers : premier élément commençant par `$`, sauf la Corbeille
/// et le dossier virtuel des orphelins.
fn is_metafile(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or(path);
    first.starts_with('$') && !first.eq_ignore_ascii_case("$RECYCLE.BIN") && first != "$OrphanFiles"
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
    /// Fichiers et octets déjà présents dans `dest` au lancement : non comptés.
    baseline: (u64, u64),
    /// Dernier comptage, refait au plus toutes les `RECOUNT_EVERY`.
    counted: (u64, u64),
    counted_at: Option<Instant>,
}

/// Parcourir toute la destination coûte cher sur une clé USB lente avec beaucoup de fichiers.
const RECOUNT_EVERY: Duration = Duration::from_secs(2);

impl TskJob {
    /// Lance la récupération des fichiers supprimés du volume `mount` (racine, ex. `E:\`) vers
    /// `dest`, qui doit être sur un autre disque physique.
    pub fn start(tsk_dir: &Path, mount: &Path, dest: &Path) -> Result<TskJob, RecoveryError> {
        let device = prepare(mount, dest)?;
        Self::spawn(&tsk_dir.join(RECOVER), &[device], dest)
    }

    /// Lancement brut (tests : image disque au lieu d'un volume).
    pub fn spawn(program: &Path, source: &[String], dest: &Path) -> Result<TskJob, RecoveryError> {
        let baseline = count_files(dest);
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
            baseline,
            counted: baseline,
            counted_at: None,
        })
    }

    pub fn progress(&mut self) -> TskProgress {
        if self.exit_code.is_none() {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exit_code = Some(status.code().unwrap_or(-1));
            }
        }
        let finished = self.exit_code.is_some();
        if finished || self.counted_at.is_none_or(|t| t.elapsed() >= RECOUNT_EVERY) {
            self.counted = count_files(&self.dest);
            self.counted_at = Some(Instant::now());
        }
        let files_found = self.counted.0.saturating_sub(self.baseline.0);
        let bytes_found = self.counted.1.saturating_sub(self.baseline.1);
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

/// Vérifie que `dest` est sur un autre disque que le volume `mount`, crée `dest` et rend le chemin
/// système du volume pour The Sleuth Kit.
fn prepare(mount: &Path, dest: &Path) -> Result<String, RecoveryError> {
    let device = volume_device(mount).ok_or_else(|| RecoveryError::InvalidDestination {
        path: mount.to_path_buf(),
        reason: "volume introuvable".into(),
    })?;
    // Disque(s) du volume source résolus directement : la liste des volumes sert aux
    // destinations et écarte les volumes en lecture seule, justement les bons cas de source
    // (carte SD verrouillée, montage `ro`).
    let source_disks = match locate_destination(mount)? {
        DestinationLocation::Disks { disks } if !disks.is_empty() => disks,
        DestinationLocation::Unknown { reason } => {
            return Err(RecoveryError::SourceDiskUnknown {
                path: mount.to_path_buf(),
                reason,
            })
        }
        _ => {
            return Err(RecoveryError::SourceDiskUnknown {
                path: mount.to_path_buf(),
                reason: "volume réseau ou sans disque".into(),
            })
        }
    };
    let check = || {
        source_disks
            .iter()
            .try_for_each(|d| validate_destination(d, dest))
    };
    check()?;
    std::fs::create_dir_all(dest).map_err(|e| RecoveryError::CreateDestination {
        path: dest.to_path_buf(),
        reason: e.to_string(),
    })?;
    // Seconde vérification sur le dossier réel (un point de montage a pu être traversé).
    check()?;
    Ok(device)
}

/// Fichier choisi dans la liste de `fls` pour une récupération ciblée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct SelectedFile {
    pub inode: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectionResult {
    pub recovered: u64,
    pub bytes: u64,
    /// Chemin d'origine et raison, pour chaque fichier non récupéré.
    pub failed: Vec<(String, String)>,
}

/// Chemin relatif sûr à partir d'un chemin lu sur le disque analysé : sans `..`, sans racine ni
/// lettre de lecteur, sans caractère interdit sous Windows. Un nom venu du disque est une donnée
/// non fiable : sans ce nettoyage, `../../Windows/x` écrirait hors du dossier de destination.
pub fn safe_relative_path(original: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in original.split(['/', '\\']) {
        let part = part.trim();
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        let clean: String = part
            .chars()
            .map(|c| match c {
                '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
                c if c.is_control() => '_',
                c => c,
            })
            .collect();
        // Noms finissant par un point ou une espace : refusés par Windows.
        let mut clean = clean.trim_end_matches(['.', ' ']).to_string();
        if is_reserved_windows_name(&clean) {
            clean.insert(0, '_');
        }
        if !clean.is_empty() {
            out.push(clean);
        }
    }
    if out.as_os_str().is_empty() {
        out.push("sans-nom");
    }
    out
}

/// Noms de périphériques de Windows (`NUL.txt` écrirait dans le vide, `COM1` ouvrirait un port
/// série), quelle que soit la casse et l'extension, y compris `COM¹` et `CONIN$`.
fn is_reserved_windows_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_uppercase();
    let port = |p: &str| {
        stem.strip_prefix(p).is_some_and(|n| {
            n.len() <= 2
                && matches!(n.chars().next(), Some('0'..='9' | '¹' | '²' | '³'))
                && n.chars().count() == 1
        })
    };
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || port("COM")
        || port("LPT")
}

/// Chemin libre : `nom.ext`, sinon `nom (2).ext`, `nom (3).ext`... Plusieurs fichiers supprimés
/// portent souvent le même nom (versions successives d'un document, première lettre effacée
/// sous FAT) : aucun ne doit en écraser un autre.
fn create_unique(target: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = target
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut candidate = target.to_path_buf();
    for n in 2..10_000 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => return Ok((candidate, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate = target.with_file_name(format!("{stem} ({n}){ext}"));
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::ErrorKind::AlreadyExists.into())
}

/// Récupère seulement les fichiers choisis, avec `icat -r`, à leur chemin d'origine sous `dest`.
/// Un fichier en échec n'arrête pas les autres.
pub fn recover_selected(
    tsk_dir: &Path,
    mount: &Path,
    files: &[SelectedFile],
    dest: &Path,
) -> Result<SelectionResult, RecoveryError> {
    let device = prepare(mount, dest)?;
    Ok(extract_files(&tsk_dir.join(ICAT), &device, files, dest))
}

/// Extraction brute (tests : image disque au lieu d'un volume).
pub fn extract_files(
    icat: &Path,
    source: &str,
    files: &[SelectedFile],
    dest: &Path,
) -> SelectionResult {
    let mut result = SelectionResult {
        recovered: 0,
        bytes: 0,
        failed: Vec::new(),
    };
    for f in files {
        let wanted = dest.join(safe_relative_path(&f.path));
        let outcome = (|| -> Result<u64, String> {
            // L'adresse vient de l'interface : jamais passée à icat si elle n'en est pas une.
            if !valid_inode(&f.inode) {
                return Err("adresse de fichier invalide".into());
            }
            if let Some(parent) = wanted.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let (target, file) = create_unique(&wanted).map_err(|e| e.to_string())?;
            let mut cmd = Command::new(icat);
            cmd.args(["-r", source, &f.inode])
                .stdin(Stdio::null())
                .stdout(file)
                .stderr(Stdio::null());
            hide_console(&mut cmd);
            let status = cmd.status().map_err(|e| e.to_string())?;
            let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            if !status.success() && size == 0 {
                let _ = std::fs::remove_file(&target);
                return Err("contenu illisible (données déjà écrasées)".into());
            }
            Ok(size)
        })();
        match outcome {
            Ok(size) => {
                result.recovered += 1;
                result.bytes += size;
            }
            Err(reason) => result.failed.push((f.path.clone(), reason)),
        }
    }
    result
}

/// Nombre de fichiers par extension (en minuscules) sous `dir`, récursivement.
pub fn count_by_extension(dir: &Path) -> std::collections::BTreeMap<String, u64> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let ext = path
                    .extension()
                    .map(|x| x.to_string_lossy().to_lowercase())
                    .unwrap_or_else(|| "(sans)".into());
                *out.entry(ext).or_insert(0) += 1;
            }
        }
    }
    out
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

    #[test]
    fn unsafe_paths_stay_inside_destination() {
        assert_eq!(
            safe_relative_path("Photos/IMG_1.JPG"),
            PathBuf::from("Photos").join("IMG_1.JPG")
        );
        assert_eq!(
            safe_relative_path("../../Windows/x.dll"),
            PathBuf::from("Windows").join("x.dll")
        );
        assert_eq!(
            safe_relative_path("C:\\a:b?.txt"),
            PathBuf::from("C_").join("a_b_.txt")
        );
        assert_eq!(safe_relative_path("/.."), PathBuf::from("sans-nom"));
        assert_eq!(
            safe_relative_path("dossier. /nom "),
            PathBuf::from("dossier").join("nom")
        );
    }

    #[cfg(windows)]
    #[test]
    fn drive_letters_map_to_volume_devices() {
        assert_eq!(volume_device(Path::new(r"E:\")).as_deref(), Some(r"\\.\E:"));
        assert_eq!(volume_device(Path::new("d:")).as_deref(), Some(r"\\.\D:"));
        assert_eq!(volume_device(Path::new(r"E:\dossier")), None);
    }

    #[test]
    fn recycle_bin_and_orphans_are_kept_realloc_and_metafiles_are_not() {
        let text = "r/r * 70-128-2:\t$RECYCLE.BIN/S-1-5-21-1/$R3AB12.jpg\t2025-09-01 00:00:00 (UTC)\t\t\t\t2048\t0\t0\n\
r/r * 71-128-1:\t$MFT\t2025-09-01 00:00:00 (UTC)\t\t\t\t4096\t0\t0\n\
r/r * 123(realloc):\tAncien.docx\t2025-09-01 00:00:00 (UTC)\t\t\t\t10\t0\t0\n\
r/r * 900:\t$OrphanFiles/_ICHIER.TXT\t2025-09-01 00:00:00 (UTC)\t\t\t\t5\t0\t0\n";
        let paths: Vec<String> = parse_fls(text).into_iter().map(|f| f.path).collect();
        assert_eq!(
            paths,
            [
                "$RECYCLE.BIN/S-1-5-21-1/$R3AB12.jpg",
                "$OrphanFiles/_ICHIER.TXT"
            ]
        );
        assert!(valid_inode("70-128-2") && valid_inode("5"));
        assert!(!valid_inode("-x") && !valid_inode("1-2-3-4") && !valid_inode("12a"));
    }

    #[test]
    fn windows_device_names_are_renamed() {
        assert_eq!(
            safe_relative_path("docs/NUL.txt"),
            PathBuf::from("docs").join("_NUL.txt")
        );
        assert_eq!(safe_relative_path("com1"), PathBuf::from("_com1"));
        assert_eq!(safe_relative_path("COM¹.log"), PathBuf::from("_COM¹.log"));
        assert_eq!(
            safe_relative_path("CONSOLE.txt"),
            PathBuf::from("CONSOLE.txt")
        );
        assert_eq!(safe_relative_path("COM10"), PathBuf::from("COM10"));
    }

    #[test]
    fn same_name_files_do_not_overwrite_each_other() {
        let dir = std::env::temp_dir().join(format!("pccheck-unique-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("Rapport.docx");
        let (a, _) = create_unique(&target).unwrap();
        let (b, _) = create_unique(&target).unwrap();
        assert_eq!(a, target);
        assert_eq!(b, dir.join("Rapport (2).docx"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
