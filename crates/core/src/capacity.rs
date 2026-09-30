//! Test de capacité réelle (méthode de H2testw et f3) sur l'ESPACE LIBRE d'un volume : les fichiers
//! existants ne sont pas touchés. On remplit l'espace libre de fichiers dont chaque secteur porte sa
//! position et une signature, puis on relit tout sans cache.
//!
//! Une fausse clé USB annonce plus que sa mémoire réelle : les écritures au-delà reviennent au début
//! de la puce et écrasent ce qui y était. À la relecture, on trouve alors des secteurs dont la
//! position inscrite ne correspond pas à l'endroit lu (« écrasés »), à distinguer des secteurs
//! simplement abîmés (« corrompus »).
//!
//! Le mode « disque entier » (destructif) du plan n'est pas fait : sur le volume formaté d'une clé
//! vide, l'espace libre couvre presque toute la puce, et c'est le cas d'usage d'un achat.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

use crate::rawio::{self, AlignedBuf, ALIGN};

const SECTOR: usize = 512;
const CHUNK: usize = 4 << 20;
const FILE_BYTES: u64 = 1 << 30;
/// Espace laissé libre pour ne pas bloquer le système de fichiers.
const MARGIN_BYTES: u64 = 32 << 20;
const TEST_DIR: &str = "10-4-capacite-test";
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum CapacityError {
    #[error("espace libre illisible sur {path} : {reason}")]
    FreeSpace { path: String, reason: String },
    #[error("pas assez d'espace libre pour un test (au moins 64 Mo)")]
    NotEnoughSpace,
    #[error("création du dossier de test impossible : {0}")]
    CreateDir(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityPhase {
    Write,
    Verify,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapacityProgress {
    pub phase: CapacityPhase,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub rate_bps: u64,
    pub bad_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CapacityVerdict {
    /// Tout ce qui a été écrit a été relu intact.
    Genuine,
    /// Tout est bon jusqu'à une limite, puis presque plus rien : capacité réelle estimée.
    Fake { real_bytes: u64 },
    /// Erreurs éparses : mémoire abîmée.
    Damaged,
    /// Test annulé avant la fin de la relecture.
    Incomplete,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapacityResult {
    pub target: String,
    pub free_bytes_before: u64,
    pub written_bytes: u64,
    pub verified_bytes: u64,
    pub ok_bytes: u64,
    pub corrupted_bytes: u64,
    /// Secteurs relus avec une autre position que la leur : signe d'une fausse capacité.
    pub overwritten_bytes: u64,
    pub first_bad_offset: Option<u64>,
    pub write_error: Option<String>,
    pub write_mbps: f64,
    pub read_mbps: f64,
    pub cancelled: bool,
    pub verdict: CapacityVerdict,
}

/// Espace libre du volume qui contient `path`, en octets.
pub fn free_space(path: &Path) -> io::Result<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut free = 0u64;
        // SAFETY : chaîne terminée par un zéro ; pointeurs vers des variables locales, les autres
        // sorties sont facultatives (NULL).
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(free)
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY : chaîne C valide, `stat` écrit dans une variable locale.
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
}

/// Lance le test sur l'espace libre du volume qui contient `target` (dossier existant).
/// `max_bytes` limite la quantité écrite (tests, essais rapides). Les fichiers de test sont
/// toujours supprimés à la fin, même après une annulation.
pub fn run_capacity_test(
    target: &Path,
    max_bytes: Option<u64>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(&CapacityProgress),
) -> Result<CapacityResult, CapacityError> {
    let free = free_space(target).map_err(|e| CapacityError::FreeSpace {
        path: target.display().to_string(),
        reason: e.to_string(),
    })?;
    let mut budget = free.saturating_sub(MARGIN_BYTES);
    if let Some(max) = max_bytes {
        budget = budget.min(max);
    }
    budget = budget / CHUNK as u64 * CHUNK as u64;
    if budget < (16 << 20) {
        return Err(CapacityError::NotEnoughSpace);
    }
    let dir = target.join(TEST_DIR);
    fs::create_dir_all(&dir).map_err(|e| CapacityError::CreateDir(e.to_string()))?;
    let seed = seed();

    let written = write_phase(&dir, budget, seed, cancel, &mut on_progress);
    let verify = if cancel.load(Ordering::Relaxed) {
        None
    } else {
        Some(verify_phase(&written.files, seed, cancel, &mut on_progress))
    };
    let _ = fs::remove_dir_all(&dir);

    let v = verify.unwrap_or_default();
    let cancelled = cancel.load(Ordering::Relaxed);
    let verdict = if cancelled || v.verified == 0 {
        CapacityVerdict::Incomplete
    } else {
        classify(&v, written.bytes)
    };
    Ok(CapacityResult {
        target: target.display().to_string(),
        free_bytes_before: free,
        written_bytes: written.bytes,
        verified_bytes: v.verified,
        ok_bytes: v.ok,
        corrupted_bytes: v.corrupted,
        overwritten_bytes: v.overwritten,
        first_bad_offset: v.first_bad,
        write_error: written.error,
        write_mbps: mbps(written.bytes, written.duration),
        read_mbps: mbps(v.verified, v.duration),
        cancelled,
        verdict,
    })
}

struct Written {
    files: Vec<(PathBuf, u64, u64)>, // (chemin, position globale de départ, taille)
    bytes: u64,
    duration: Duration,
    error: Option<String>,
}

fn write_phase(
    dir: &Path,
    budget: u64,
    seed: u64,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(&CapacityProgress),
) -> Written {
    let mut buf = AlignedBuf::new(CHUNK);
    let start = Instant::now();
    let mut last = start;
    let mut out = Written {
        files: Vec::new(),
        bytes: 0,
        duration: Duration::ZERO,
        error: None,
    };
    let mut index = 0;
    'files: while out.bytes < budget && !cancel.load(Ordering::Relaxed) {
        index += 1;
        let path = dir.join(format!("bloc_{index:04}.bin"));
        let size = (budget - out.bytes).min(FILE_BYTES);
        let base = out.bytes;
        let mut file = match rawio::create_uncached(&path) {
            Ok(f) => f,
            Err(e) => {
                out.error = Some(e.to_string());
                break;
            }
        };
        let mut done = 0u64;
        while done < size {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            fill(&mut buf, base + done, seed);
            if let Err(e) = file.write_all(&buf) {
                // Disque plein ou puce qui ne répond plus : on vérifie ce qui a été écrit.
                out.error = Some(e.to_string());
                out.files.push((path, base, done));
                out.bytes += done;
                break 'files;
            }
            done += CHUNK as u64;
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                on_progress(&CapacityProgress {
                    phase: CapacityPhase::Write,
                    done_bytes: out.bytes + done,
                    total_bytes: budget,
                    rate_bps: rate(out.bytes + done, start.elapsed()),
                    bad_bytes: 0,
                });
            }
        }
        if let Err(e) = file.sync_all() {
            out.error.get_or_insert(e.to_string());
        }
        out.files.push((path, base, done));
        out.bytes += done;
        if done < size {
            break;
        }
    }
    out.duration = start.elapsed();
    out
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Verified {
    verified: u64,
    ok: u64,
    corrupted: u64,
    overwritten: u64,
    first_bad: Option<u64>,
    /// Octets mauvais au-delà de `first_bad` (pour distinguer fausse capacité et erreurs éparses).
    bad_after_first: u64,
    duration: Duration,
}

fn verify_phase(
    files: &[(PathBuf, u64, u64)],
    seed: u64,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(&CapacityProgress),
) -> Verified {
    let total: u64 = files.iter().map(|f| f.2).sum();
    let mut buf = AlignedBuf::new(CHUNK);
    let mut v = Verified::default();
    let start = Instant::now();
    let mut last = start;
    for (path, base, size) in files {
        let file = rawio::open_uncached(path);
        let mut done = 0u64;
        while done < *size {
            if cancel.load(Ordering::Relaxed) {
                v.duration = start.elapsed();
                return v;
            }
            let pos = base + done;
            match &file {
                Ok(f) if rawio::read_exact_at(f, &mut buf, done).is_ok() => {
                    check_chunk(&buf, pos, seed, &mut v)
                }
                // Fichier ou zone illisible : tout le morceau compte comme corrompu.
                _ => record_bad(&mut v, pos, CHUNK as u64, false),
            }
            v.verified += CHUNK as u64;
            done += CHUNK as u64;
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                on_progress(&CapacityProgress {
                    phase: CapacityPhase::Verify,
                    done_bytes: v.verified,
                    total_bytes: total,
                    rate_bps: rate(v.verified, start.elapsed()),
                    bad_bytes: v.corrupted + v.overwritten,
                });
            }
        }
    }
    v.duration = start.elapsed();
    v
}

/// Remplit `buf` avec le motif de la position globale `pos` : pour chaque secteur de 512 octets,
/// 8 octets de position, 8 octets de signature du test, puis une suite pseudo-aléatoire
/// déterminée par les deux (xorshift64).
pub(crate) fn fill(buf: &mut [u8], pos: u64, seed: u64) {
    for (i, sector) in buf.as_chunks_mut::<SECTOR>().0.iter_mut().enumerate() {
        fill_sector(sector, pos + (i * SECTOR) as u64, seed);
    }
}

fn fill_sector(sector: &mut [u8], pos: u64, seed: u64) {
    sector[..8].copy_from_slice(&pos.to_le_bytes());
    sector[8..16].copy_from_slice(&seed.to_le_bytes());
    let mut x = (pos ^ seed) | 1;
    for word in sector[16..].as_chunks_mut::<8>().0 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *word = x.to_le_bytes();
    }
}

pub(crate) fn check_chunk(buf: &[u8], pos: u64, seed: u64, v: &mut Verified) {
    let mut expected = [0u8; SECTOR];
    for (i, &sector) in buf.as_chunks::<SECTOR>().0.iter().enumerate() {
        let at = pos + (i * SECTOR) as u64;
        fill_sector(&mut expected, at, seed);
        if sector == expected {
            v.ok += SECTOR as u64;
            continue;
        }
        // Secteur intact mais écrit pour une autre position : la puce a « bouclé ».
        let written_pos = u64::from_le_bytes(sector[..8].try_into().unwrap_or_default());
        let written_seed = u64::from_le_bytes(sector[8..16].try_into().unwrap_or_default());
        let aliased = written_seed == seed && written_pos != at && {
            fill_sector(&mut expected, written_pos, seed);
            sector == expected
        };
        record_bad(v, at, SECTOR as u64, aliased);
    }
}

fn record_bad(v: &mut Verified, at: u64, len: u64, overwritten: bool) {
    if overwritten {
        v.overwritten += len;
    } else {
        v.corrupted += len;
    }
    v.first_bad.get_or_insert(at);
    v.bad_after_first += len;
}

/// Tout bon jusqu'à une limite puis presque tout mauvais (≥ 90 %) : fausse capacité.
fn classify(v: &Verified, written: u64) -> CapacityVerdict {
    match v.first_bad {
        None => CapacityVerdict::Genuine,
        Some(first) => {
            let after = written.saturating_sub(first);
            if first > 0 && after > 0 && v.bad_after_first * 10 >= after * 9 {
                CapacityVerdict::Fake { real_bytes: first }
            } else {
                CapacityVerdict::Damaged
            }
        }
    }
}

fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    nanos ^ u64::from(std::process::id()).rotate_left(32)
}

fn rate(bytes: u64, elapsed: Duration) -> u64 {
    let s = elapsed.as_secs_f64();
    if s > 0.0 {
        (bytes as f64 / s) as u64
    } else {
        0
    }
}

fn mbps(bytes: u64, d: Duration) -> f64 {
    let s = d.as_secs_f64();
    if s > 0.0 {
        bytes as f64 / 1e6 / s
    } else {
        0.0
    }
}

// Tampon aligné requis par `ALIGN` : vérifié à la compilation.
const _: () = assert!(CHUNK.is_multiple_of(ALIGN) && ALIGN.is_multiple_of(SECTOR));

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x1234_5678_9ABC_DEF0;

    #[test]
    fn intact_pattern_verifies() {
        let mut buf = vec![0u8; 64 * SECTOR];
        fill(&mut buf, 1 << 20, SEED);
        let mut v = Verified::default();
        check_chunk(&buf, 1 << 20, SEED, &mut v);
        assert_eq!(v.ok, buf.len() as u64);
        assert_eq!(v.first_bad, None);
    }

    #[test]
    fn corruption_and_aliasing_are_told_apart() {
        let mut buf = vec![0u8; 8 * SECTOR];
        fill(&mut buf, 0, SEED);
        // Secteur 2 abîmé (un bit changé), secteur 5 remplacé par celui d'une autre position.
        buf[2 * SECTOR + 100] ^= 0x10;
        let mut other = vec![0u8; SECTOR];
        fill(&mut other, 64 << 30, SEED);
        buf[5 * SECTOR..6 * SECTOR].copy_from_slice(&other);
        let mut v = Verified::default();
        check_chunk(&buf, 0, SEED, &mut v);
        assert_eq!(v.corrupted, SECTOR as u64);
        assert_eq!(v.overwritten, SECTOR as u64);
        assert_eq!(v.first_bad, Some(2 * SECTOR as u64));
    }

    #[test]
    fn fake_capacity_is_estimated() {
        // 8 Go annoncés, 2 Go réels : tout mauvais au-delà de 2 Go.
        let written = 8u64 << 30;
        let v = Verified {
            first_bad: Some(2 << 30),
            bad_after_first: 6 << 30,
            ..Verified::default()
        };
        assert_eq!(
            classify(&v, written),
            CapacityVerdict::Fake {
                real_bytes: 2 << 30
            }
        );
        let scattered = Verified {
            first_bad: Some(1 << 30),
            bad_after_first: 10 << 20,
            ..Verified::default()
        };
        assert_eq!(classify(&scattered, written), CapacityVerdict::Damaged);
    }

    #[test]
    fn small_real_run_on_temp_dir_is_genuine_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("pccheck-capacity-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut phases = Vec::new();
        let r = run_capacity_test(&dir, Some(32 << 20), &AtomicBool::new(false), |p| {
            phases.push(p.phase)
        })
        .unwrap();
        assert_eq!(r.written_bytes, 32 << 20);
        assert_eq!(r.verified_bytes, 32 << 20);
        assert_eq!(r.ok_bytes, 32 << 20);
        assert_eq!(r.verdict, CapacityVerdict::Genuine);
        assert!(!dir.join(TEST_DIR).exists(), "fichiers de test supprimés");
        let _ = fs::remove_dir_all(&dir);
    }
}
