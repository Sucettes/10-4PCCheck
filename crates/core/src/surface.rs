//! Scan de surface en lecture seule : lit tout le disque, bloc par bloc, sans cache, et relève
//! les zones illisibles et les zones lentes. Rien n'est jamais écrit.
//!
//! Zone lente : un bloc qui prend beaucoup plus de temps que la médiane (le disque a dû relire
//! plusieurs fois un secteur fragile). Zone illisible : erreur de lecture, localisée ensuite par
//! sous-blocs pour ne pas condamner tout le bloc.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

use crate::rawio::{self, AlignedBuf, ALIGN};

/// Taille d'un bloc de lecture : assez gros pour le débit, assez petit pour une progression fluide.
pub const BLOCK_BYTES: usize = 4 << 20;
/// Taille des sous-blocs relus un par un quand un bloc échoue.
const RETRY_BYTES: usize = 64 << 10;
/// Un bloc est lent s'il dépasse `SLOW_FACTOR` fois la médiane ET `SLOW_MIN` (évite les faux
/// positifs sur un SSD où la médiane est de quelques millisecondes).
const SLOW_FACTOR: u32 = 5;
const SLOW_MIN: Duration = Duration::from_millis(250);
/// Nombre de tranches du profil de débit (1 tranche = 1 % du disque).
const PROFILE_SLICES: usize = 100;
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum SurfaceError {
    #[error("ce disque ne peut pas être lu directement ({0})")]
    UnsupportedDevice(String),
    #[error("accès refusé : relance l'outil en administrateur")]
    PermissionDenied,
    #[error("ouverture du disque impossible : {0}")]
    Open(String),
}

/// Lecture positionnée d'un support : un disque brut en vrai, un tampon en mémoire dans les tests.
pub trait BlockReader {
    fn read_block(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
}

impl BlockReader for std::fs::File {
    fn read_block(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        rawio::read_exact_at(self, buf, offset)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceProgress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
    /// Débit moyen depuis le début, en octets par seconde.
    pub rate_bps: u64,
    pub bad_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ByteRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceResult {
    pub total_bytes: u64,
    pub read_bytes: u64,
    pub duration_ms: u64,
    pub cancelled: bool,
    /// Zones illisibles, fusionnées quand elles se touchent.
    pub bad_ranges: Vec<ByteRange>,
    pub bad_bytes: u64,
    pub slow_blocks: u64,
    /// Temps de lecture médian et maximal d'un bloc.
    pub median_block_ms: f64,
    pub max_block_ms: f64,
    /// Débit moyen par tranche de 1 % du disque, en Mo/s (courbe descendante normale sur un
    /// disque dur : l'extérieur du plateau est plus rapide). Tranches non lues : `None`.
    pub profile_mbps: Vec<Option<f64>>,
}

/// Ouvre le disque nommé par smartctl et le scanne sur `total_bytes`.
pub fn scan_device(
    smartctl_name: &str,
    total_bytes: u64,
    cancel: &AtomicBool,
    on_progress: impl FnMut(&SurfaceProgress),
) -> Result<SurfaceResult, SurfaceError> {
    let path = rawio::raw_device_path(smartctl_name)
        .ok_or_else(|| SurfaceError::UnsupportedDevice(smartctl_name.to_string()))?;
    let mut file = rawio::open_device_read(&path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => SurfaceError::PermissionDenied,
        _ => SurfaceError::Open(format!("{path} : {e}")),
    })?;
    Ok(scan(&mut file, total_bytes, cancel, on_progress))
}

/// Scan générique. La fin du support non alignée sur 4 Kio (quelques octets au plus) est ignorée.
pub fn scan<R: BlockReader>(
    reader: &mut R,
    total_bytes: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(&SurfaceProgress),
) -> SurfaceResult {
    let total = total_bytes / ALIGN as u64 * ALIGN as u64;
    let mut buf = AlignedBuf::new(BLOCK_BYTES);
    let mut bad: Vec<ByteRange> = Vec::new();
    let mut timings: Vec<(u64, Duration, u64)> = Vec::new(); // (position, durée, octets)
    let start = Instant::now();
    let mut last_progress = start;
    let mut offset = 0u64;
    let mut cancelled = false;

    while offset < total {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let len = (total - offset).min(BLOCK_BYTES as u64) as usize;
        let t0 = Instant::now();
        let ok = reader.read_block(offset, &mut buf[..len]).is_ok();
        let elapsed = t0.elapsed();
        if ok {
            timings.push((offset, elapsed, len as u64));
        } else {
            locate_bad(reader, &mut buf, offset, len, &mut bad, cancel);
        }
        offset += len as u64;

        if last_progress.elapsed() >= PROGRESS_EVERY || offset >= total {
            last_progress = Instant::now();
            on_progress(&SurfaceProgress {
                done_bytes: offset,
                total_bytes: total,
                elapsed_ms: start.elapsed().as_millis() as u64,
                rate_bps: rate(offset, start.elapsed()),
                bad_bytes: bad.iter().map(|r| r.len).sum(),
            });
        }
    }

    summarize(total, offset, start.elapsed(), cancelled, bad, &timings)
}

/// Relit un bloc en échec par sous-blocs pour ne marquer que la partie vraiment illisible.
fn locate_bad<R: BlockReader>(
    reader: &mut R,
    buf: &mut AlignedBuf,
    offset: u64,
    len: usize,
    bad: &mut Vec<ByteRange>,
    cancel: &AtomicBool,
) {
    let mut sub = 0usize;
    // Sur un disque mourant, chaque relecture peut prendre plusieurs secondes : l'annulation est
    // vérifiée entre deux relectures, pas seulement entre deux blocs de 4 Mio.
    while sub < len && !cancel.load(Ordering::Relaxed) {
        let n = RETRY_BYTES.min(len - sub);
        let at = offset + sub as u64;
        if reader.read_block(at, &mut buf[..n]).is_err() {
            push_merged(bad, at, n as u64);
        }
        sub += n;
    }
}

fn push_merged(ranges: &mut Vec<ByteRange>, offset: u64, len: u64) {
    match ranges.last_mut() {
        Some(last) if last.offset + last.len == offset => last.len += len,
        _ => ranges.push(ByteRange { offset, len }),
    }
}

fn rate(bytes: u64, elapsed: Duration) -> u64 {
    let secs = elapsed.as_secs_f64();
    if secs > 0.0 {
        (bytes as f64 / secs) as u64
    } else {
        0
    }
}

fn summarize(
    total: u64,
    read: u64,
    duration: Duration,
    cancelled: bool,
    bad_ranges: Vec<ByteRange>,
    timings: &[(u64, Duration, u64)],
) -> SurfaceResult {
    let mut durations: Vec<Duration> = timings.iter().map(|t| t.1).collect();
    durations.sort_unstable();
    let median = durations
        .get(durations.len() / 2)
        .copied()
        .unwrap_or_default();
    let max = durations.last().copied().unwrap_or_default();
    let slow_limit = (median * SLOW_FACTOR).max(SLOW_MIN);
    let slow_blocks = timings.iter().filter(|t| t.1 > slow_limit).count() as u64;

    // Profil : octets et temps cumulés par tranche de 1 %.
    let mut slices = vec![(0u64, Duration::ZERO); PROFILE_SLICES];
    if total > 0 {
        for &(pos, d, bytes) in timings {
            let i = ((pos as u128 * PROFILE_SLICES as u128) / total as u128) as usize;
            let slot = &mut slices[i.min(PROFILE_SLICES - 1)];
            slot.0 += bytes;
            slot.1 += d;
        }
    }
    let profile_mbps = slices
        .iter()
        .map(|&(bytes, d)| {
            (bytes > 0 && !d.is_zero()).then(|| bytes as f64 / 1e6 / d.as_secs_f64())
        })
        .collect();

    SurfaceResult {
        total_bytes: total,
        read_bytes: read,
        duration_ms: duration.as_millis() as u64,
        cancelled,
        bad_bytes: bad_ranges.iter().map(|r| r.len).sum(),
        bad_ranges,
        slow_blocks,
        median_block_ms: median.as_secs_f64() * 1000.0,
        max_block_ms: max.as_secs_f64() * 1000.0,
        profile_mbps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Support simulé : `size` octets, lecture en échec sur les zones `bad`.
    struct Fake {
        size: u64,
        bad: Vec<ByteRange>,
        reads: u64,
    }

    impl BlockReader for Fake {
        fn read_block(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
            self.reads += 1;
            let end = offset + buf.len() as u64;
            assert!(end <= self.size, "lecture hors du support");
            assert_eq!(offset % ALIGN as u64, 0, "position non alignée");
            if self
                .bad
                .iter()
                .any(|b| offset < b.offset + b.len && b.offset < end)
            {
                return Err(io::Error::other("secteur illisible"));
            }
            Ok(())
        }
    }

    #[test]
    fn healthy_media_is_fully_read() {
        let size = 10 * BLOCK_BYTES as u64 + 1000; // fin non alignée ignorée
        let mut fake = Fake {
            size,
            bad: vec![],
            reads: 0,
        };
        let mut last = None;
        let r = scan(&mut fake, size, &AtomicBool::new(false), |p| {
            last = Some(p.clone())
        });
        assert_eq!(r.total_bytes, 10 * BLOCK_BYTES as u64);
        assert_eq!(r.read_bytes, r.total_bytes);
        assert!(r.bad_ranges.is_empty() && !r.cancelled);
        assert_eq!(fake.reads, 10);
        assert_eq!(last.unwrap().done_bytes, r.total_bytes);
        assert_eq!(r.profile_mbps.len(), 100);
    }

    #[test]
    fn bad_sectors_are_located_to_64k() {
        let size = 4 * BLOCK_BYTES as u64;
        // 4 Kio illisibles au milieu du 2e bloc, et deux sous-blocs contigus dans le 4e.
        let bad = vec![
            ByteRange {
                offset: BLOCK_BYTES as u64 + 100 * 4096,
                len: 4096,
            },
            ByteRange {
                offset: 3 * BLOCK_BYTES as u64,
                len: 2 * RETRY_BYTES as u64,
            },
        ];
        let mut fake = Fake {
            size,
            bad,
            reads: 0,
        };
        let r = scan(&mut fake, size, &AtomicBool::new(false), |_| {});
        assert_eq!(
            r.bad_ranges,
            vec![
                ByteRange {
                    offset: BLOCK_BYTES as u64 + 6 * RETRY_BYTES as u64,
                    len: RETRY_BYTES as u64
                },
                ByteRange {
                    offset: 3 * BLOCK_BYTES as u64,
                    len: 2 * RETRY_BYTES as u64
                },
            ]
        );
        assert_eq!(r.bad_bytes, 3 * RETRY_BYTES as u64);
        assert_eq!(
            r.read_bytes, size,
            "le scan continue après une zone illisible"
        );
    }

    #[test]
    fn cancel_before_start_reads_nothing() {
        let size = 100 * BLOCK_BYTES as u64;
        let mut fake = Fake {
            size,
            bad: vec![],
            reads: 0,
        };
        let r = scan(&mut fake, size, &AtomicBool::new(true), |_| {});
        assert!(r.cancelled);
        assert_eq!(r.read_bytes, 0);
        assert_eq!(fake.reads, 0);
    }

    #[test]
    fn real_file_is_scanned_through_raw_io() {
        let dir = std::env::temp_dir().join(format!("pccheck-surface-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("support.bin");
        std::fs::write(&path, vec![0xA5u8; 3 * BLOCK_BYTES]).unwrap();
        let mut file = rawio::open_uncached(&path).unwrap();
        let r = scan(
            &mut file,
            3 * BLOCK_BYTES as u64,
            &AtomicBool::new(false),
            |_| {},
        );
        assert_eq!(r.read_bytes, 3 * BLOCK_BYTES as u64);
        assert!(r.bad_ranges.is_empty());
        drop(file);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
