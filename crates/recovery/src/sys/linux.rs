//! Linux : disque physique d'un dossier (stat → /sys/dev/block/M:m → disque parent) et
//! liste des points de montage.

use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::disk::{dev_major_minor, sysfs_disk_name, DestinationLocation, DiskId};
use crate::volumes::{destination_mounts, mount_of, parse_mounts, unescape_udev_label, Volume};

const MOUNTS: &str = "/proc/self/mounts";
/// Profondeur maximale de dm → esclaves → loop → fichier : évite une boucle sur un sysfs
/// inattendu.
const MAX_DEPTH: u32 = 8;
/// Systèmes de fichiers réseau : destination forcément hors du disque source local.
const REMOTE_FS: [&str; 8] = [
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "fuse.sshfs",
    "9p",
    "ceph",
];

pub(crate) fn locate_path(path: &Path) -> DestinationLocation {
    locate_path_depth(path, 0)
}

fn locate_path_depth(path: &Path, depth: u32) -> DestinationLocation {
    let canonical = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => return unknown(format!("{}: {e}", path.display())),
    };
    let meta = match fs::metadata(&canonical) {
        Ok(m) => m,
        Err(e) => return unknown(format!("{}: {e}", canonical.display())),
    };
    let (major, minor) = dev_major_minor(meta.dev());
    // Majeur 0 : périphérique anonyme (btrfs, tmpfs, overlay, NFS...) ; on passe par le
    // montage pour retrouver le vrai périphérique.
    if major != 0 {
        return disks_or_unknown(disks_of_devnum(major, minor, depth), &canonical);
    }
    let mounts = match fs::read_to_string(MOUNTS) {
        Ok(t) => parse_mounts(&t),
        Err(e) => return unknown(format!("{MOUNTS}: {e}")),
    };
    let Some(mount) = mount_of(&mounts, &canonical) else {
        return unknown(format!("aucun montage pour {}", canonical.display()));
    };
    if REMOTE_FS.contains(&mount.fs_type.as_str()) {
        return DestinationLocation::Remote;
    }
    if !mount.device.starts_with("/dev/") {
        return unknown(format!(
            "système de fichiers {} sans disque (en mémoire ou virtuel)",
            mount.fs_type
        ));
    }
    match fs::metadata(&mount.device) {
        Ok(m) => {
            let (ma, mi) = dev_major_minor(m.rdev());
            disks_or_unknown(disks_of_devnum(ma, mi, depth), &canonical)
        }
        Err(e) => unknown(format!("{}: {e}", mount.device)),
    }
}

fn unknown(reason: String) -> DestinationLocation {
    DestinationLocation::Unknown { reason }
}

fn disks_or_unknown(disks: Vec<DiskId>, path: &Path) -> DestinationLocation {
    if disks.is_empty() {
        unknown(format!(
            "disque de {} introuvable dans /sys",
            path.display()
        ))
    } else {
        DestinationLocation::Disks { disks }
    }
}

/// Disques physiques sous un périphérique bloc (majeur:mineur).
fn disks_of_devnum(major: u64, minor: u64, depth: u32) -> Vec<DiskId> {
    match fs::canonicalize(format!("/sys/dev/block/{major}:{minor}")) {
        Ok(p) => disks_of_sysfs(&p, depth),
        Err(_) => Vec::new(),
    }
}

/// Descend jusqu'aux disques physiques : partition → disque ; dm/md (LVM, LUKS, RAID) →
/// leurs `slaves` ; loop → disque du fichier image.
fn disks_of_sysfs(canonical: &Path, depth: u32) -> Vec<DiskId> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let is_partition = canonical.join("partition").exists();
    let disk_dir = if is_partition {
        canonical.parent().unwrap_or(canonical)
    } else {
        canonical
    };
    let Some(name) = sysfs_disk_name(canonical, is_partition) else {
        return Vec::new();
    };

    let slaves: Vec<PathBuf> = fs::read_dir(disk_dir.join("slaves"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|e| fs::canonicalize(e.path()).ok())
                .collect()
        })
        .unwrap_or_default();
    if !slaves.is_empty() {
        let mut out = Vec::new();
        for slave in slaves {
            for d in disks_of_sysfs(&slave, depth + 1) {
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
        return out;
    }

    if name.starts_with("loop") {
        let backing = fs::read_to_string(disk_dir.join("loop/backing_file")).unwrap_or_default();
        let backing = backing.trim();
        if backing.is_empty() {
            return Vec::new();
        }
        return match locate_path_depth(Path::new(backing), depth + 1) {
            DestinationLocation::Disks { disks } => disks,
            _ => Vec::new(),
        };
    }
    vec![DiskId::Block(name)]
}

pub(crate) fn list_volumes() -> Vec<Volume> {
    let Ok(text) = fs::read_to_string(MOUNTS) else {
        return Vec::new();
    };
    let entries = parse_mounts(&text);
    let labels = labels_by_device();
    destination_mounts(&entries)
        .into_iter()
        .filter_map(|m| {
            let (total_bytes, free_bytes) = space(&m.mount_point)?;
            let device = fs::canonicalize(&m.device).unwrap_or_else(|_| PathBuf::from(&m.device));
            let disks = fs::metadata(&device)
                .map(|meta| {
                    let (ma, mi) = dev_major_minor(meta.rdev());
                    disks_of_devnum(ma, mi, 0)
                })
                .unwrap_or_default();
            let disk = match disks.as_slice() {
                [one] => Some(one.clone()),
                _ => None,
            };
            let removable = disk.as_ref().is_some_and(is_removable);
            Some(Volume {
                path: m.mount_point.clone(),
                label: labels
                    .iter()
                    .find(|(dev, _)| *dev == device)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_default(),
                filesystem: m.fs_type.clone(),
                total_bytes,
                free_bytes,
                disk,
                removable,
            })
        })
        .collect()
}

/// (taille totale, espace libre pour un utilisateur ordinaire) en octets.
// Types de `statvfs` variables selon l'architecture (u32 ou u64) : conversions explicites.
#[allow(clippy::unnecessary_cast)]
fn space(mount_point: &Path) -> Option<(u64, u64)> {
    let c = CString::new(mount_point.as_os_str().as_bytes()).ok()?;
    // SAFETY : `statvfs` est une structure C sans invariant ; zéro est une valeur valide.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY : chemin terminé par zéro ; `st` inscriptible.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let unit = st.f_frsize as u64;
    Some((
        (st.f_blocks as u64).saturating_mul(unit),
        (st.f_bavail as u64).saturating_mul(unit),
    ))
}

/// (périphérique canonique, nom de volume) d'après `/dev/disk/by-label`.
fn labels_by_device() -> Vec<(PathBuf, String)> {
    let Ok(entries) = fs::read_dir("/dev/disk/by-label") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let dev = fs::canonicalize(e.path()).ok()?;
            let label = unescape_udev_label(e.file_name().to_str()?);
            Some((dev, label))
        })
        .collect()
}

/// Support amovible (`removable` = 1 : clé USB, carte SD) ou disque branché en USB.
fn is_removable(disk: &DiskId) -> bool {
    let DiskId::Block(name) = disk else {
        return false;
    };
    let base = Path::new("/sys/block").join(name);
    let flag = fs::read_to_string(base.join("removable")).unwrap_or_default();
    let usb = fs::canonicalize(&base)
        .map(|p| p.to_string_lossy().contains("/usb"))
        .unwrap_or(false);
    flag.trim() == "1" || usb
}
