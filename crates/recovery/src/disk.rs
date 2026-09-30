//! Identité des disques physiques : conversion des noms smartctl, chemin à passer à PhotoRec,
//! et règle « la destination doit être sur un autre disque que la source ».
//!
//! La partie système (quel disque porte un dossier ?) est dans `sys` ; ici, tout est pur et
//! testable sur n'importe quelle plateforme.

use std::fmt;
use std::path::{Path, PathBuf};

use pccheck_core::rawio;
use serde::{Deserialize, Serialize};

use crate::error::RecoveryError;
use crate::sys;

/// Un disque physique, tel que le système le nomme.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum DiskId {
    /// Windows : numéro N de `\\.\PhysicalDriveN`.
    PhysicalDrive(u32),
    /// Linux : nom du périphérique bloc du disque entier (`sda`, `nvme0n1`, `mmcblk0`).
    Block(String),
}

impl DiskId {
    /// Chemin du disque entier à passer à PhotoRec après `/cmd`.
    /// Windows : `\\.\PhysicalDriveN`, format que PhotoRec énumère lui-même (hdaccess.c,
    /// https://github.com/cgsecurity/testdisk/blob/master/src/hdaccess.c). Linux : `/dev/<nom>`.
    pub fn photorec_device(&self) -> String {
        match self {
            DiskId::PhysicalDrive(n) => format!(r"\\.\PhysicalDrive{n}"),
            DiskId::Block(name) => format!("/dev/{name}"),
        }
    }
}

impl fmt::Display for DiskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiskId::PhysicalDrive(n) => write!(f, "PhysicalDrive{n}"),
            DiskId::Block(name) => write!(f, "{name}"),
        }
    }
}

/// Plateforme dont on applique les conventions de nommage. Paramètre explicite pour pouvoir
/// tester les règles Windows sous Linux et inversement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// Disque correspondant à un nom de périphérique smartctl (`ScanDevice::name`), selon les
/// conventions de la plateforme courante.
pub fn disk_from_smartctl_name(name: &str) -> Result<DiskId, RecoveryError> {
    disk_from_smartctl_name_for(name, Platform::current())
}

/// Chemin à passer à PhotoRec pour un disque listé par smartctl (ex. `/dev/sdb` sous Windows
/// donne `\\.\PhysicalDrive1`).
pub fn photorec_device_from_smartctl(name: &str) -> Result<String, RecoveryError> {
    disk_from_smartctl_name(name).map(|d| d.photorec_device())
}

/// Conversion pure d'un nom smartctl vers `DiskId`.
///
/// Windows (page de manuel smartctl, section Windows) : `/dev/sda` à `/dev/sdz` sont
/// `\\.\PhysicalDrive0` à 25, puis `/dev/sdaa`... ; `/dev/pdN` est `\\.\PhysicalDriveN`.
/// Les NVMe sont listés en `/dev/sdX` par `--scan` sous Windows ; `/dev/nvmeN` y désigne un
/// contrôleur SCSI, pas un numéro de disque, donc refusé.
///
/// Linux : `/dev/sdX`, `/dev/vdX`, `/dev/hdX`, `/dev/xvdX`, `/dev/mmcblkN` tels quels ;
/// `/dev/nvme0` (contrôleur, forme de smartctl) devient l'espace de noms `nvme0n1`.
///
/// Les cas `/dev/sdX`, `/dev/pdN` (Windows) et `/dev/sdX` (Linux) passent par
/// `pccheck_core::rawio`, déjà utilisé pour la lecture brute : un seul endroit pour la règle.
pub fn disk_from_smartctl_name_for(
    name: &str,
    platform: Platform,
) -> Result<DiskId, RecoveryError> {
    let unsupported = || RecoveryError::UnsupportedDeviceName {
        name: name.to_string(),
    };
    let rest = name.strip_prefix("/dev/").ok_or_else(unsupported)?;
    match platform {
        Platform::Windows => rawio::windows_raw_path(name)
            .and_then(|p| parse_digits(p.strip_prefix(r"\\.\PhysicalDrive")?))
            .map(DiskId::PhysicalDrive)
            .ok_or_else(unsupported),
        Platform::Linux => {
            if rest.starts_with("sd") {
                return rawio::linux_raw_path(name)
                    .map(|_| DiskId::Block(rest.to_string()))
                    .ok_or_else(unsupported);
            }
            for prefix in ["vd", "hd", "xvd"] {
                if let Some(letters) = rest.strip_prefix(prefix) {
                    return if is_disk_letters(letters) {
                        Ok(DiskId::Block(rest.to_string()))
                    } else {
                        Err(unsupported())
                    };
                }
            }
            if let Some(tail) = rest.strip_prefix("nvme") {
                return match tail.split_once('n') {
                    // `/dev/nvme0` : contrôleur, premier espace de noms.
                    None if parse_digits(tail).is_some() => Ok(DiskId::Block(format!("{rest}n1"))),
                    // `/dev/nvme0n1` : déjà un espace de noms (pas une partition `...p1`).
                    Some((ctrl, ns))
                        if parse_digits(ctrl).is_some() && parse_digits(ns).is_some() =>
                    {
                        Ok(DiskId::Block(rest.to_string()))
                    }
                    _ => Err(unsupported()),
                };
            }
            if let Some(digits) = rest.strip_prefix("mmcblk") {
                return parse_digits(digits)
                    .map(|_| DiskId::Block(rest.to_string()))
                    .ok_or_else(unsupported);
            }
            Err(unsupported())
        }
    }
}

/// Suffixe de disque entier (`a`, `ab`) : une ou deux minuscules, pas de numéro de partition.
fn is_disk_letters(letters: &str) -> bool {
    (1..=2).contains(&letters.len()) && letters.bytes().all(|b| b.is_ascii_lowercase())
}

fn parse_digits(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Où se trouve physiquement un dossier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DestinationLocation {
    /// Disques physiques qui portent le volume (plusieurs pour un LVM ou un RAID logiciel).
    Disks { disks: Vec<DiskId> },
    /// Partage réseau : forcément un autre disque que la source locale.
    Remote,
    /// Impossible à déterminer (tmpfs, volume dynamique, erreur système...).
    Unknown { reason: String },
}

/// La destination doit être sur un autre disque physique que la source (règle stricte du plan).
/// Résout le disque de `dest` (ou de son plus proche parent existant, le dossier pouvant ne pas
/// encore exister), puis applique `check_location`.
pub fn validate_destination(source_disk: &DiskId, dest: &Path) -> Result<(), RecoveryError> {
    let location = locate_destination(dest)?;
    check_location(source_disk, dest, &location)
}

/// Disque(s) physique(s) portant `dest` ou son plus proche parent existant.
pub fn locate_destination(dest: &Path) -> Result<DestinationLocation, RecoveryError> {
    let existing =
        nearest_existing_ancestor(dest).ok_or_else(|| RecoveryError::InvalidDestination {
            path: dest.to_path_buf(),
            reason: "aucun dossier parent n'existe".into(),
        })?;
    Ok(sys::locate_path(&existing))
}

/// Règle pure : refus si la source est parmi les disques de la destination, ou si on ne sait
/// pas où est la destination (on ne peut pas prouver qu'elle est ailleurs).
pub fn check_location(
    source_disk: &DiskId,
    dest: &Path,
    location: &DestinationLocation,
) -> Result<(), RecoveryError> {
    match location {
        DestinationLocation::Remote => Ok(()),
        DestinationLocation::Disks { disks } if disks.is_empty() => {
            Err(RecoveryError::DestinationDiskUnknown {
                path: dest.to_path_buf(),
                reason: "aucun disque trouvé".into(),
            })
        }
        DestinationLocation::Disks { disks } if disks.contains(source_disk) => {
            Err(RecoveryError::SameDisk {
                path: dest.to_path_buf(),
                disk: source_disk.clone(),
            })
        }
        DestinationLocation::Disks { .. } => Ok(()),
        DestinationLocation::Unknown { reason } => Err(RecoveryError::DestinationDiskUnknown {
            path: dest.to_path_buf(),
            reason: reason.clone(),
        }),
    }
}

/// Premier ancêtre (ou `path` lui-même) qui existe. Chemin relatif : rendu absolu d'abord,
/// sinon on remonterait vers `""` sans jamais trouver le vrai volume.
pub(crate) fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    absolute
        .ancestors()
        .find(|p| p.exists())
        .map(Path::to_path_buf)
}

// ---------- Aides pures pour Linux (sysfs, numéros de périphérique) ----------
// Compilées partout pour être testées sous Windows aussi.

/// Décompose un `dev_t` Linux en (majeur, mineur). Encodage glibc/musl (sys/sysmacros.h) :
/// 12 bits de majeur et 20 bits de mineur répartis sur 64 bits.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn dev_major_minor(dev: u64) -> (u64, u64) {
    let major = ((dev >> 32) & 0xffff_f000) | ((dev >> 8) & 0x0fff);
    let minor = ((dev >> 12) & 0xffff_ff00) | (dev & 0x00ff);
    (major, minor)
}

/// Nom du disque entier à partir du chemin canonique sysfs d'un périphérique bloc
/// (`/sys/devices/.../block/sda/sda1`). Une partition (fichier `partition` présent) a son
/// disque dans le dossier parent.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn sysfs_disk_name(canonical: &Path, is_partition: bool) -> Option<String> {
    let disk_dir = if is_partition {
        canonical.parent()?
    } else {
        canonical
    };
    disk_dir.file_name()?.to_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_sd_names_map_to_physical_drive_numbers() {
        let w = |n| disk_from_smartctl_name_for(n, Platform::Windows);
        assert_eq!(w("/dev/sda"), Ok(DiskId::PhysicalDrive(0)));
        assert_eq!(w("/dev/sdb"), Ok(DiskId::PhysicalDrive(1)));
        assert_eq!(w("/dev/sdz"), Ok(DiskId::PhysicalDrive(25)));
        assert_eq!(w("/dev/sdaa"), Ok(DiskId::PhysicalDrive(26)));
        assert_eq!(w("/dev/sdab"), Ok(DiskId::PhysicalDrive(27)));
        assert_eq!(w("/dev/pd3"), Ok(DiskId::PhysicalDrive(3)));
        assert_eq!(
            DiskId::PhysicalDrive(1).photorec_device(),
            r"\\.\PhysicalDrive1"
        );
    }

    #[test]
    fn windows_rejects_names_without_drive_number() {
        for name in [
            "/dev/nvme0",
            "/dev/csmi0,1",
            "/dev/sd",
            "/dev/sdA",
            "/dev/sda1",
            "sda",
            "/dev/sdabc",
        ] {
            assert!(
                matches!(
                    disk_from_smartctl_name_for(name, Platform::Windows),
                    Err(RecoveryError::UnsupportedDeviceName { .. })
                ),
                "{name} aurait dû être refusé"
            );
        }
    }

    #[test]
    fn linux_names_map_to_block_devices() {
        let l = |n| disk_from_smartctl_name_for(n, Platform::Linux).map(|d| d.photorec_device());
        assert_eq!(l("/dev/sda"), Ok("/dev/sda".into()));
        assert_eq!(l("/dev/sdab"), Ok("/dev/sdab".into()));
        assert_eq!(l("/dev/vdb"), Ok("/dev/vdb".into()));
        assert_eq!(l("/dev/nvme0"), Ok("/dev/nvme0n1".into()));
        assert_eq!(l("/dev/nvme1n2"), Ok("/dev/nvme1n2".into()));
        assert_eq!(l("/dev/mmcblk0"), Ok("/dev/mmcblk0".into()));
        assert_eq!(
            disk_from_smartctl_name_for("/dev/nvme0", Platform::Linux),
            Ok(DiskId::Block("nvme0n1".into()))
        );
    }

    #[test]
    fn linux_rejects_partitions_and_exotic_names() {
        for name in [
            "/dev/sda1",
            "/dev/nvme0n1p2",
            "/dev/nvme",
            "/dev/bus/0",
            "/dev/mmcblk0p1",
            "/dev/",
        ] {
            assert!(
                disk_from_smartctl_name_for(name, Platform::Linux).is_err(),
                "{name} aurait dû être refusé"
            );
        }
    }

    #[test]
    fn same_disk_is_refused_and_other_disk_accepted() {
        let src = DiskId::PhysicalDrive(1);
        let dest = Path::new(r"E:\recup");
        let same = DestinationLocation::Disks {
            disks: vec![DiskId::PhysicalDrive(1)],
        };
        let other = DestinationLocation::Disks {
            disks: vec![DiskId::PhysicalDrive(2)],
        };
        assert!(matches!(
            check_location(&src, dest, &same),
            Err(RecoveryError::SameDisk { .. })
        ));
        assert_eq!(check_location(&src, dest, &other), Ok(()));
    }

    #[test]
    fn volume_spanning_source_disk_is_refused() {
        // LVM sur sda et sdb : la source sdb fait partie du volume.
        let loc = DestinationLocation::Disks {
            disks: vec![DiskId::Block("sda".into()), DiskId::Block("sdb".into())],
        };
        assert!(check_location(&DiskId::Block("sdb".into()), Path::new("/home"), &loc).is_err());
        assert!(check_location(&DiskId::Block("sdc".into()), Path::new("/home"), &loc).is_ok());
    }

    #[test]
    fn unknown_location_is_refused_remote_accepted() {
        let src = DiskId::Block("sdb".into());
        let unknown = DestinationLocation::Unknown {
            reason: "tmpfs".into(),
        };
        assert!(matches!(
            check_location(&src, Path::new("/tmp/x"), &unknown),
            Err(RecoveryError::DestinationDiskUnknown { .. })
        ));
        let empty = DestinationLocation::Disks { disks: vec![] };
        assert!(check_location(&src, Path::new("/tmp/x"), &empty).is_err());
        assert_eq!(
            check_location(&src, Path::new("/mnt/nas"), &DestinationLocation::Remote),
            Ok(())
        );
    }

    #[test]
    fn nearest_ancestor_skips_missing_folders() {
        let base = std::env::temp_dir();
        let missing = base.join("pccheck-inexistant-a").join("b").join("c");
        assert_eq!(nearest_existing_ancestor(&missing), Some(base));
    }

    #[test]
    fn dev_t_decomposition_matches_glibc() {
        // makedev(8, 17) = /dev/sdb1 ; makedev(259, 3) = partition NVMe ; grand mineur.
        let makedev = |ma: u64, mi: u64| {
            ((ma & 0xffff_f000) << 32)
                | ((ma & 0x0fff) << 8)
                | ((mi & 0xffff_ff00) << 12)
                | (mi & 0x00ff)
        };
        assert_eq!(dev_major_minor(makedev(8, 17)), (8, 17));
        assert_eq!(dev_major_minor(makedev(259, 3)), (259, 3));
        assert_eq!(dev_major_minor(makedev(4097, 1_048_575)), (4097, 1_048_575));
        assert_eq!(dev_major_minor(0x0811), (8, 17));
    }

    #[test]
    fn sysfs_path_gives_parent_disk() {
        let part = Path::new(
            "/sys/devices/pci0000:00/0000:00:17.0/ata1/host0/target0:0:0/0:0:0:0/block/sda/sda1",
        );
        assert_eq!(sysfs_disk_name(part, true), Some("sda".into()));
        let nvme = Path::new(
            "/sys/devices/pci0000:00/0000:00:1d.0/0000:3d:00.0/nvme/nvme0/nvme0n1/nvme0n1p3",
        );
        assert_eq!(sysfs_disk_name(nvme, true), Some("nvme0n1".into()));
        let whole = Path::new("/sys/devices/virtual/block/dm-0");
        assert_eq!(sysfs_disk_name(whole, false), Some("dm-0".into()));
    }

    #[test]
    fn disk_id_serializes_with_kind() {
        let json = serde_json::to_string(&DiskId::PhysicalDrive(2)).unwrap();
        assert_eq!(json, r#"{"kind":"physical_drive","id":2}"#);
        let back: DiskId = serde_json::from_str(r#"{"kind":"block","id":"sdb"}"#).unwrap();
        assert_eq!(back, DiskId::Block("sdb".into()));
    }
}
