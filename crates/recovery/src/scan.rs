//! Comptage des fichiers récupérés dans les dossiers `recup_dir.N` de la destination.
//!
//! PhotoRec nomme ses fichiers `f<secteur>.<ext>` (parfois `f<secteur>_<nom>.<ext>`, ou `t`/`b`
//! pour des cas particuliers) et passe au dossier suivant tous les 500 fichiers. La date de
//! modification n'est pas celle de la récupération : PhotoRec y remet la date interne du
//! fichier (EXIF, etc.). L'ordre « plus récent » est donc approché par (numéro de dossier,
//! numéro de secteur).

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::RECUP_DIR_PREFIX;
use crate::error::RecoveryError;

/// Nombre de fichiers gardés dans `RecoveryProgress::last_files`.
pub const LAST_FILES: usize = 10;

/// Rapport DFXML que PhotoRec écrit dans son premier dossier : pas un fichier récupéré.
const REPORT_FILE: &str = "report.xml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FoundFile {
    pub path: PathBuf,
    pub name: String,
    /// Extension en minuscules, vide s'il n'y en a pas.
    pub extension: String,
    pub size: u64,
}

/// Bilan des fichiers trouvés.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FoundSummary {
    pub files_found: u64,
    pub bytes_found: u64,
    pub by_extension: BTreeMap<String, u64>,
    /// Les `LAST_FILES` derniers, du plus récent au plus ancien.
    pub last_files: Vec<FoundFile>,
}

/// Mémoire des dossiers terminés. Quand `recup_dir.N+1` existe, PhotoRec n'écrit plus dans
/// `recup_dir.N` : inutile de le relire à chaque rafraîchissement (des centaines de milliers
/// de fichiers sur un gros disque).
#[derive(Debug, Default)]
pub(crate) struct ScanCache {
    frozen: BTreeMap<u32, DirSummary>,
}

#[derive(Debug, Clone, Default)]
struct DirSummary {
    files: u64,
    bytes: u64,
    by_extension: BTreeMap<String, u64>,
    /// Les plus récents du dossier, déjà triés.
    newest: Vec<FoundFile>,
}

/// Numéro d'un dossier `recup_dir.N`.
pub(crate) fn parse_recup_index(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(RECUP_DIR_PREFIX)?.strip_prefix('.')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Dossiers `recup_dir.N` de `dest`, triés par numéro. Destination absente : liste vide.
pub(crate) fn recup_dirs(dest: &Path) -> Result<Vec<(u32, PathBuf)>, RecoveryError> {
    let entries = match fs::read_dir(dest) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(RecoveryError::io(dest, &e)),
    };
    let mut dirs: Vec<(u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let index = parse_recup_index(e.file_name().to_str()?)?;
            Some((index, e.path()))
        })
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// Plus grand numéro de `recup_dir.N` déjà présent (0 si aucun). Une nouvelle exécution de
/// PhotoRec écrira à partir de ce numéro + 1.
pub(crate) fn max_recup_index(dest: &Path) -> Result<u32, RecoveryError> {
    Ok(recup_dirs(dest)?.last().map_or(0, |(i, _)| *i))
}

/// Compte les fichiers des dossiers `recup_dir.N` avec `N >= first_index`.
pub fn count_found(dest: &Path, first_index: u32) -> Result<FoundSummary, RecoveryError> {
    count_found_cached(dest, first_index, &mut ScanCache::default())
}

pub(crate) fn count_found_cached(
    dest: &Path,
    first_index: u32,
    cache: &mut ScanCache,
) -> Result<FoundSummary, RecoveryError> {
    let dirs: Vec<(u32, PathBuf)> = recup_dirs(dest)?
        .into_iter()
        .filter(|(i, _)| *i >= first_index)
        .collect();
    let last = dirs.last().map(|(i, _)| *i);

    let mut total = FoundSummary::default();
    let mut newest: Vec<(u32, FoundFile)> = Vec::new();
    for (index, path) in &dirs {
        let summary = match cache.frozen.get(index) {
            Some(s) => s.clone(),
            None => {
                let s = summarize_dir(path)?;
                if Some(*index) != last {
                    cache.frozen.insert(*index, s.clone());
                }
                s
            }
        };
        total.files_found += summary.files;
        total.bytes_found += summary.bytes;
        for (ext, n) in summary.by_extension {
            *total.by_extension.entry(ext).or_insert(0) += n;
        }
        newest.extend(summary.newest.into_iter().map(|f| (*index, f)));
    }
    sort_newest_first(&mut newest);
    total.last_files = newest
        .into_iter()
        .take(LAST_FILES)
        .map(|(_, f)| f)
        .collect();
    Ok(total)
}

/// Fichiers récupérés dans tous les `recup_dir.N` de `dest`, du plus récent au plus ancien,
/// au plus `limit`.
pub fn list_found(dest: &Path, limit: usize) -> Result<Vec<FoundFile>, RecoveryError> {
    let mut all: Vec<(u32, FoundFile)> = Vec::new();
    // Du dernier dossier au premier : on s'arrête dès que `limit` est atteint.
    for (index, path) in recup_dirs(dest)?.into_iter().rev() {
        if all.len() >= limit {
            break;
        }
        all.extend(files_of(&path)?.into_iter().map(|f| (index, f)));
    }
    sort_newest_first(&mut all);
    all.truncate(limit);
    Ok(all.into_iter().map(|(_, f)| f).collect())
}

fn summarize_dir(dir: &Path) -> Result<DirSummary, RecoveryError> {
    let files = files_of(dir)?;
    let mut s = DirSummary::default();
    for f in &files {
        s.files += 1;
        s.bytes += f.size;
        *s.by_extension.entry(f.extension.clone()).or_insert(0) += 1;
    }
    let mut sorted: Vec<(u32, FoundFile)> = files.into_iter().map(|f| (0, f)).collect();
    sort_newest_first(&mut sorted);
    s.newest = sorted
        .into_iter()
        .take(LAST_FILES)
        .map(|(_, f)| f)
        .collect();
    Ok(s)
}

/// Fichiers d'un dossier `recup_dir.N`. Un fichier disparu entre la liste et la lecture de sa
/// taille (rare) est ignoré plutôt que de faire échouer tout le comptage.
fn files_of(dir: &Path) -> Result<Vec<FoundFile>, RecoveryError> {
    let entries = fs::read_dir(dir).map_err(|e| RecoveryError::io(dir, &e))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name == REPORT_FILE {
                return None;
            }
            Some(FoundFile {
                extension: extension_of(&name),
                path: e.path(),
                name,
                size: meta.len(),
            })
        })
        .collect())
}

fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Numéro de secteur d'un nom PhotoRec : `f0012345.jpg`, `f0012345_titre.doc` → 12345.
pub(crate) fn sector_of(name: &str) -> Option<u64> {
    let rest = name.get(1..)?;
    let digits: &str = &rest[..rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len())];
    digits.parse().ok()
}

fn sort_newest_first(files: &mut [(u32, FoundFile)]) {
    files.sort_by_key(|(dir, f)| Reverse((*dir, sector_of(&f.name), f.name.clone())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recup_dir_names() {
        assert_eq!(parse_recup_index("recup_dir.1"), Some(1));
        assert_eq!(parse_recup_index("recup_dir.42"), Some(42));
        assert_eq!(parse_recup_index("recup_dir"), None);
        assert_eq!(parse_recup_index("recup_dir."), None);
        assert_eq!(parse_recup_index("recup_dir.1a"), None);
        assert_eq!(parse_recup_index("autre.1"), None);
    }

    #[test]
    fn sector_numbers_from_photorec_names() {
        assert_eq!(sector_of("f0012345.jpg"), Some(12345));
        assert_eq!(sector_of("f0012345_Rapport.doc"), Some(12345));
        assert_eq!(sector_of("t123.txt"), Some(123));
        assert_eq!(sector_of("report.xml"), None);
        assert_eq!(sector_of(""), None);
        assert_eq!(sector_of("é1.jpg"), None);
    }

    #[test]
    fn finished_dirs_are_cached_not_reread() {
        let dest =
            std::env::temp_dir().join(format!("pccheck-recovery-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);
        let d1 = dest.join("recup_dir.1");
        fs::create_dir_all(&d1).unwrap();
        fs::write(d1.join("f0000001.jpg"), b"a").unwrap();
        let mut cache = ScanCache::default();

        // Seul dossier : c'est le dossier courant, jamais mis en cache.
        assert_eq!(
            count_found_cached(&dest, 1, &mut cache)
                .unwrap()
                .files_found,
            1
        );
        fs::write(d1.join("f0000002.jpg"), b"b").unwrap();
        assert_eq!(
            count_found_cached(&dest, 1, &mut cache)
                .unwrap()
                .files_found,
            2
        );

        // recup_dir.2 apparaît : recup_dir.1 est terminé et mémorisé.
        let d2 = dest.join("recup_dir.2");
        fs::create_dir_all(&d2).unwrap();
        fs::write(d2.join("f0000003.png"), b"c").unwrap();
        let s = count_found_cached(&dest, 1, &mut cache).unwrap();
        assert_eq!(s.files_found, 3);
        assert_eq!(s.by_extension.get("jpg"), Some(&2));
        // Preuve du cache : un ajout tardif dans recup_dir.1 n'est plus relu.
        fs::write(d1.join("f0000009.jpg"), b"d").unwrap();
        assert_eq!(
            count_found_cached(&dest, 1, &mut cache)
                .unwrap()
                .files_found,
            3
        );
        assert_eq!(count_found(&dest, 1).unwrap().files_found, 4);
    }
}
