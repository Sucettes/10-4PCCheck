//! Test de vitesse rapide, environ une minute par disque :
//! - lecture de 256 Mio au début, au milieu et à la fin du disque (un disque dur lit environ deux
//!   fois moins vite à la fin : l'intérieur du plateau défile moins vite sous la tête) ;
//! - temps d'accès : 100 lectures de 4 Kio à des endroits tirés au hasard, ce que l'utilisateur
//!   ressent quand il ouvre beaucoup de petits fichiers ;
//! - écriture d'un fichier NEUF de 1 Gio dans l'espace libre d'un volume du disque, supprimé à la
//!   fin. Aucun fichier existant n'est touché ; un fichier supprimé encore récupérable peut en
//!   revanche être écrasé (il occupe de l'espace libre).
//!
//! Chaque mesure est placée sur une échelle propre au type de disque (`speed_scale`) : 150 Mo/s
//! est excellent pour un disque dur et très faible pour un SSD NVMe.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

use crate::capacity::free_space;
use crate::disk::{DiskInfo, MediaKind, Protocol};
use crate::rawio::{self, AlignedBuf, ALIGN};
use crate::surface::BlockReader;

/// Données lues à chaque position du disque.
const ZONE_BYTES: u64 = 256 << 20;
const HDD_CHUNK: usize = 4 << 20;
const SSD_CHUNK: usize = 8 << 20;
/// Lectures simultanées sur un SSD : il n'atteint son débit qu'avec plusieurs demandes en cours.
/// Sur un disque dur, la tête perdrait son temps à sauter d'une demande à l'autre.
const SSD_PARALLEL: usize = 4;
const ACCESS_SAMPLES: u32 = 100;
const ACCESS_BYTES: usize = ALIGN;
/// Taille du fichier écrit pour mesurer l'écriture.
pub const WRITE_BYTES: u64 = 1 << 30;
/// Espace libre à laisser en plus du fichier : le volume ne doit jamais se retrouver plein.
const WRITE_MARGIN: u64 = 2 << 30;
const WRITE_CHUNK: usize = 8 << 20;
/// Préfixe du fichier de test (reconnaissable s'il restait après un arrêt brutal).
pub const WRITE_FILE_PREFIX: &str = "pccheck-vitesse-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedStep {
    Read,
    Access,
    Write,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeedProgress {
    pub step: SpeedStep,
    /// Avancement de l'étape, de 0 à 100.
    pub pct: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneSpeed {
    /// Position de la zone lue : 0 (début), 50 (milieu), 100 (fin).
    pub position_pct: u8,
    pub mbps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccessTime {
    pub avg_ms: f64,
    pub max_ms: f64,
    pub samples: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadSpeed {
    pub zones: Vec<ZoneSpeed>,
    pub access: Option<AccessTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WriteSpeed {
    /// Volume où le fichier de test a été écrit.
    pub volume: String,
    pub mbps: f64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Error, Serialize, PartialEq, Eq)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum WriteSkip {
    #[error("aucun volume de ce disque n'est accessible en écriture")]
    NoVolume,
    #[error("pas assez d'espace libre ({free_mb} Mo, il faut au moins 3 Go)")]
    NotEnoughSpace { free_mb: u64 },
    #[error("création du fichier de test impossible : {0}")]
    Create(String),
    #[error("écriture interrompue : {0}")]
    Write(String),
    #[error("test arrêté")]
    Cancelled,
}

/// Résultat complet, gardé pour l'écran et le rapport.
#[derive(Debug, Clone, Serialize)]
pub struct SpeedResult {
    pub read: Option<ReadSpeed>,
    /// Lecture impossible (droits, disque qui ne répond pas).
    pub read_error: Option<String>,
    pub write: Option<WriteSpeed>,
    /// Pourquoi l'écriture n'a pas été mesurée.
    pub write_skipped: Option<String>,
    pub cancelled: bool,
    /// Repères du type de disque, `None` si le type est inconnu.
    pub scale: Option<SpeedScale>,
}

/// Lecture aux trois positions puis temps d'accès. `open` ouvre une nouvelle lecture du disque :
/// une par fil sur un SSD (lectures simultanées), une seule sur un disque dur.
pub fn read_test<R: BlockReader>(
    open: impl Fn() -> io::Result<R> + Sync,
    total_bytes: u64,
    ssd: bool,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(&SpeedProgress),
) -> io::Result<ReadSpeed> {
    let chunk = if ssd { SSD_CHUNK } else { HDD_CHUNK } as u64;
    let zone = zone_bytes(total_bytes, chunk).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "disque trop petit pour le test",
        )
    })?;
    let positions = [
        (0u8, 0u64),
        (50, align_down(total_bytes / 2 - zone / 2, chunk)),
        (100, align_down(total_bytes - zone, chunk)),
    ];
    let mut zones = Vec::with_capacity(positions.len());
    for (i, (pct, offset)) in positions.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let elapsed = if ssd {
            read_zone_parallel(&open, offset, zone, chunk as usize, cancel)?
        } else {
            read_zone(&mut open()?, offset, zone, chunk as usize, cancel)?
        };
        zones.push(ZoneSpeed {
            position_pct: pct,
            mbps: mbps(zone, elapsed),
        });
        on_progress(&SpeedProgress {
            step: SpeedStep::Read,
            pct: ((i + 1) * 100 / positions.len()) as u8,
        });
    }
    let access = if cancel.load(Ordering::Relaxed) {
        None
    } else {
        Some(access_test(
            &mut open()?,
            total_bytes,
            cancel,
            &mut on_progress,
        )?)
    };
    Ok(ReadSpeed { zones, access })
}

/// Zone lue à chaque position : 256 Mio, ou moins sur un petit disque (un quart de sa taille).
fn zone_bytes(total: u64, chunk: u64) -> Option<u64> {
    let zone = align_down(ZONE_BYTES.min(total / 4), chunk);
    (zone >= chunk).then_some(zone)
}

fn align_down(v: u64, to: u64) -> u64 {
    v / to * to
}

fn read_zone<R: BlockReader>(
    reader: &mut R,
    offset: u64,
    len: u64,
    chunk: usize,
    cancel: &AtomicBool,
) -> io::Result<Duration> {
    let mut buf = AlignedBuf::new(chunk);
    let start = Instant::now();
    let mut done = 0;
    while done < len && !cancel.load(Ordering::Relaxed) {
        reader.read_block(offset + done, &mut buf)?;
        done += chunk as u64;
    }
    Ok(start.elapsed())
}

/// Même lecture, répartie entre `SSD_PARALLEL` fils : le fil `t` lit les blocs `t`, `t + n`...
fn read_zone_parallel<R: BlockReader>(
    open: &(impl Fn() -> io::Result<R> + Sync),
    offset: u64,
    len: u64,
    chunk: usize,
    cancel: &AtomicBool,
) -> io::Result<Duration> {
    let blocks = len / chunk as u64;
    let first_error: Mutex<Option<io::Error>> = Mutex::new(None);
    let start = Instant::now();
    std::thread::scope(|s| {
        for t in 0..SSD_PARALLEL as u64 {
            let first_error = &first_error;
            s.spawn(move || {
                let outcome = (|| -> io::Result<()> {
                    let mut reader = open()?;
                    let mut buf = AlignedBuf::new(chunk);
                    let mut b = t;
                    while b < blocks && !cancel.load(Ordering::Relaxed) {
                        reader.read_block(offset + b * chunk as u64, &mut buf)?;
                        b += SSD_PARALLEL as u64;
                    }
                    Ok(())
                })();
                if let Err(e) = outcome {
                    first_error
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get_or_insert(e);
                }
            });
        }
    });
    let elapsed = start.elapsed();
    match first_error.into_inner().unwrap_or_else(|p| p.into_inner()) {
        Some(e) => Err(e),
        None => Ok(elapsed),
    }
}

/// 100 lectures de 4 Kio à des positions pseudo-aléatoires sur tout le disque.
fn access_test<R: BlockReader>(
    reader: &mut R,
    total_bytes: u64,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(&SpeedProgress),
) -> io::Result<AccessTime> {
    let slots = (total_bytes / ACCESS_BYTES as u64).max(1);
    let mut buf = AlignedBuf::new(ACCESS_BYTES);
    let mut x = seed();
    let (mut sum, mut max, mut n) = (Duration::ZERO, Duration::ZERO, 0u32);
    for i in 0..ACCESS_SAMPLES {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // xorshift64 : assez aléatoire pour disperser les positions, sans dépendance.
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let offset = (x % slots) * ACCESS_BYTES as u64;
        let t = Instant::now();
        reader.read_block(offset, &mut buf)?;
        let d = t.elapsed();
        sum += d;
        max = max.max(d);
        n += 1;
        if (i + 1) % 20 == 0 {
            on_progress(&SpeedProgress {
                step: SpeedStep::Access,
                pct: ((i + 1) * 100 / ACCESS_SAMPLES) as u8,
            });
        }
    }
    Ok(AccessTime {
        avg_ms: if n > 0 {
            sum.as_secs_f64() * 1000.0 / f64::from(n)
        } else {
            0.0
        },
        max_ms: max.as_secs_f64() * 1000.0,
        samples: n,
    })
}

/// Écrit un fichier neuf de `bytes` octets dans `dir` sans cache, puis le supprime (même en cas
/// d'erreur ou d'arrêt). Refus si l'espace libre ne laisse pas au moins 2 Gio après le fichier.
pub fn write_test(
    dir: &Path,
    bytes: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(&SpeedProgress),
) -> Result<WriteSpeed, WriteSkip> {
    let free = free_space(dir).map_err(|e| WriteSkip::Create(e.to_string()))?;
    if free < bytes + WRITE_MARGIN {
        return Err(WriteSkip::NotEnoughSpace {
            free_mb: free / 1_000_000,
        });
    }
    let path = dir.join(format!(
        "{WRITE_FILE_PREFIX}{}-{}.tmp",
        std::process::id(),
        seed() % 1_000_000
    ));
    // `create_new` : si ce nom existait, on s'arrête au lieu d'écrire dedans.
    let mut file =
        rawio::create_new_uncached(&path).map_err(|e| WriteSkip::Create(e.to_string()))?;
    let _cleanup = RemoveOnDrop(path);

    // Contenu pseudo-aléatoire : certains contrôleurs de SSD compressent les zéros, ce qui
    // gonflerait la mesure.
    let mut buf = AlignedBuf::new(WRITE_CHUNK);
    let mut x = seed();
    for word in buf.as_chunks_mut::<8>().0 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *word = x.to_le_bytes();
    }
    let total = align_down(bytes, WRITE_CHUNK as u64);
    let start = Instant::now();
    let mut done = 0u64;
    let mut last_pct = 0;
    while done < total {
        if cancel.load(Ordering::Relaxed) {
            return Err(WriteSkip::Cancelled);
        }
        file.write_all(&buf)
            .map_err(|e| WriteSkip::Write(e.to_string()))?;
        done += WRITE_CHUNK as u64;
        let pct = (done * 100 / total) as u8;
        if pct >= last_pct + 10 {
            last_pct = pct;
            on_progress(&SpeedProgress {
                step: SpeedStep::Write,
                pct,
            });
        }
    }
    // Les données doivent être sur le disque, pas dans un cache, avant d'arrêter le chrono.
    file.sync_all()
        .map_err(|e| WriteSkip::Write(e.to_string()))?;
    let elapsed = start.elapsed();
    Ok(WriteSpeed {
        volume: dir.display().to_string(),
        mbps: mbps(done, elapsed),
        bytes: done,
    })
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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

fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    (nanos ^ u64::from(std::process::id()).rotate_left(32)) | 1
}

// ---------- Échelles ----------

/// Repères d'une mesure : `good` et `acceptable` sont des seuils (Mo/s, ou ms pour le temps
/// d'accès où plus bas vaut mieux).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Band {
    pub good: f64,
    pub acceptable: f64,
    pub higher_is_better: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Good,
    Acceptable,
    Weak,
    /// Faible, mais bridé par un port SATA ancien : le disque lui-même n'est pas en cause.
    LimitedByLink,
}

impl Band {
    const fn up(good: f64, acceptable: f64) -> Band {
        Band {
            good,
            acceptable,
            higher_is_better: true,
        }
    }

    const fn down(good: f64, acceptable: f64) -> Band {
        Band {
            good,
            acceptable,
            higher_is_better: false,
        }
    }

    pub fn rate(&self, v: f64) -> Rating {
        let (good, ok) = if self.higher_is_better {
            (v >= self.good, v >= self.acceptable)
        } else {
            (v <= self.good, v <= self.acceptable)
        };
        match (good, ok) {
            (true, _) => Rating::Good,
            (false, true) => Rating::Acceptable,
            _ => Rating::Weak,
        }
    }
}

/// Repères pour un type de disque.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeedScale {
    /// « Disque dur 7 200 tr/min », « SSD SATA », « SSD NVMe ».
    pub class: String,
    pub read: Band,
    pub write: Band,
    /// Disques durs seulement : sur un SSD, l'accès est toujours quasi instantané.
    pub access: Option<Band>,
    /// Débit maximal du lien SATA quand il est ancien (SATA II : environ 280 Mo/s).
    pub link_cap_mbps: Option<f64>,
}

impl SpeedScale {
    /// Note d'un débit, en tenant compte d'un lien SATA qui bride le disque.
    pub fn rate_throughput(&self, band: &Band, mbps: f64) -> Rating {
        let r = band.rate(mbps);
        match self.link_cap_mbps {
            Some(cap) if r != Rating::Good && band.good > cap && mbps >= cap * 0.85 => {
                Rating::LimitedByLink
            }
            _ => r,
        }
    }
}

/// Repères selon le type de disque (valeurs validées avec le propriétaire le 2026-09-30).
/// Type inconnu : `None`, les mesures sont alors affichées sans jugement.
pub fn speed_scale(info: &DiskInfo) -> Option<SpeedScale> {
    let (class, read, access) = match (&info.media, &info.protocol) {
        (_, Protocol::Nvme) => ("SSD NVMe".to_string(), Band::up(1500.0, 800.0), None),
        (MediaKind::Ssd, _) => ("SSD SATA".to_string(), Band::up(450.0, 300.0), None),
        (MediaKind::Hdd { rpm }, _) if *rpm >= 7000 => (
            format!("Disque dur {} tr/min", fmt_rpm(*rpm)),
            Band::up(120.0, 80.0),
            Some(Band::down(15.0, 25.0)),
        ),
        // 5 400 tr/min : la tête attend plus longtemps que le secteur passe sous elle.
        (MediaKind::Hdd { rpm }, _) => (
            format!("Disque dur {} tr/min", fmt_rpm(*rpm)),
            Band::up(90.0, 60.0),
            Some(Band::down(20.0, 30.0)),
        ),
        (MediaKind::Unknown, _) => return None,
    };
    let link_cap_mbps = match (&info.protocol, info.link_speed.as_deref()) {
        (Protocol::Ata, Some(s)) if s.starts_with("1.5") => Some(140.0),
        (Protocol::Ata, Some(s)) if s.starts_with("3.0") => Some(280.0),
        _ => None,
    };
    Some(SpeedScale {
        class,
        read,
        write: read,
        access,
        link_cap_mbps,
    })
}

fn fmt_rpm(rpm: u32) -> String {
    if rpm >= 1000 {
        format!("{} {:03}", rpm / 1000, rpm % 1000)
    } else {
        rpm.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Disque simulé en mémoire : lit des zéros, sans erreur.
    struct Fake {
        size: u64,
    }

    impl BlockReader for Fake {
        fn read_block(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
            if offset + buf.len() as u64 > self.size {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            buf.fill(0);
            Ok(())
        }
    }

    #[test]
    fn three_zones_and_access_are_measured_inside_the_disk() {
        let size = 3u64 << 30;
        for ssd in [false, true] {
            let r = read_test(
                || Ok(Fake { size }),
                size,
                ssd,
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            let positions: Vec<u8> = r.zones.iter().map(|z| z.position_pct).collect();
            assert_eq!(positions, [0, 50, 100]);
            assert!(r.zones.iter().all(|z| z.mbps > 0.0));
            assert_eq!(r.access.unwrap().samples, ACCESS_SAMPLES);
        }
    }

    #[test]
    fn small_disk_uses_smaller_zones_and_tiny_disk_is_refused() {
        assert_eq!(zone_bytes(64 << 20, 4 << 20), Some(16 << 20));
        assert_eq!(zone_bytes(8 << 20, 4 << 20), None);
    }

    #[test]
    fn read_error_is_reported() {
        let r = read_test(
            || Err::<Fake, _>(io::Error::other("illisible")),
            1 << 30,
            false,
            &AtomicBool::new(false),
            |_| {},
        );
        assert!(r.is_err());
    }

    #[test]
    fn write_test_leaves_nothing_and_never_reuses_a_name() {
        let dir = std::env::temp_dir().join(format!("pccheck-speed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let w = write_test(&dir, 16 << 20, &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(w.bytes, 16 << 20);
        assert!(w.mbps > 0.0);
        let left = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(left, 0, "fichier de test supprimé");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bands_rate_in_the_right_direction() {
        let read = Band::up(120.0, 80.0);
        assert_eq!(read.rate(150.0), Rating::Good);
        assert_eq!(read.rate(100.0), Rating::Acceptable);
        assert_eq!(read.rate(40.0), Rating::Weak);
        let access = Band::down(15.0, 25.0);
        assert_eq!(access.rate(12.0), Rating::Good);
        assert_eq!(access.rate(20.0), Rating::Acceptable);
        assert_eq!(access.rate(40.0), Rating::Weak);
    }

    #[test]
    fn old_sata_port_is_blamed_instead_of_the_ssd() {
        let scale = SpeedScale {
            class: "SSD SATA".into(),
            read: Band::up(450.0, 300.0),
            write: Band::up(450.0, 300.0),
            access: None,
            link_cap_mbps: Some(280.0),
        };
        assert_eq!(
            scale.rate_throughput(&scale.read, 265.0),
            Rating::LimitedByLink
        );
        assert_eq!(scale.rate_throughput(&scale.read, 120.0), Rating::Weak);
        assert_eq!(scale.rate_throughput(&scale.read, 520.0), Rating::Good);
    }
}
