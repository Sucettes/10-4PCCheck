//! Volumes montés proposés comme destination.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::disk::DiskId;
use crate::sys;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Volume {
    /// Racine du volume (`E:\`, `/media/moi/CLE`).
    pub path: PathBuf,
    /// Nom de volume, vide s'il n'en a pas.
    pub label: String,
    /// Système de fichiers (`NTFS`, `FAT32`, `exfat`, `ext4`...). FAT32 limite chaque fichier à
    /// 4 Gio : une longue vidéo récupérée y serait tronquée.
    pub filesystem: String,
    pub total_bytes: u64,
    /// Espace utilisable par l'utilisateur courant.
    pub free_bytes: u64,
    /// Disque physique, `None` si inconnu (volume sur plusieurs disques...).
    pub disk: Option<DiskId>,
    /// Support amovible ou branché en USB.
    pub removable: bool,
}

impl Volume {
    /// `true` si ce volume ne peut pas servir de destination pour une source sur `source_disk`.
    /// Un disque inconnu compte comme interdit, comme dans `check_location`.
    pub fn is_on_disk(&self, source_disk: &DiskId) -> bool {
        match &self.disk {
            Some(d) => d == source_disk,
            None => true,
        }
    }
}

/// Volumes montés et inscriptibles (Windows : lettres de lecteur locales ; Linux : points de
/// montage de `/proc/self/mounts` sur un périphérique `/dev/...`, hors lecture seule).
pub fn list_volumes() -> Vec<Volume> {
    sys::list_volumes()
}

// ---------- Aides pures pour Linux ----------

/// Une ligne de `/proc/self/mounts`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct MountEntry {
    pub device: String,
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub read_only: bool,
}

/// Systèmes de fichiers jamais proposés : lecture seule par nature ou images de paquets.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const SKIPPED_FS: [&str; 5] = ["squashfs", "iso9660", "erofs", "udf", "cramfs"];

/// Analyse `/proc/self/mounts` (format fstab, espaces échappés en octal : `\040`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = unescape_octal(fields.next()?);
            let mount_point = PathBuf::from(unescape_octal(fields.next()?));
            let fs_type = fields.next()?.to_string();
            let read_only = fields.next()?.split(',').any(|o| o == "ro");
            Some(MountEntry {
                device,
                mount_point,
                fs_type,
                read_only,
            })
        })
        .collect()
}

/// Points de montage utilisables comme destination, un seul par périphérique (le premier :
/// les montages « bind » et sous-volumes btrfs répètent le même périphérique).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn destination_mounts(entries: &[MountEntry]) -> Vec<&MountEntry> {
    let mut kept: Vec<&MountEntry> = Vec::new();
    for e in entries {
        let dev_ok = e.device.starts_with("/dev/") && !e.device.starts_with("/dev/loop");
        let mp = e.mount_point.to_string_lossy();
        let system_mp = [
            "/boot",
            "/proc",
            "/sys",
            "/dev",
            "/snap/",
            "/run/credentials",
        ]
        .iter()
        .any(|p| {
            mp == p.trim_end_matches('/')
                || mp.starts_with(&format!("{}/", p.trim_end_matches('/')))
        });
        if dev_ok
            && !e.read_only
            && !system_mp
            && !SKIPPED_FS.contains(&e.fs_type.as_str())
            && !kept.iter().any(|k| k.device == e.device)
        {
            kept.push(e);
        }
    }
    kept
}

/// Montage qui contient `path` (préfixe le plus long). `path` doit être canonique.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn mount_of<'a>(entries: &'a [MountEntry], path: &Path) -> Option<&'a MountEntry> {
    entries
        .iter()
        .filter(|e| path.starts_with(&e.mount_point))
        // À longueur égale, le dernier montage masque les précédents.
        .max_by_key(|e| e.mount_point.as_os_str().len())
}

/// Décode `\040` (et toute séquence `\ooo`) de /proc/mounts.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn unescape_octal(s: &str) -> String {
    decode_escapes(
        s,
        |digits| {
            (digits.len() == 3 && digits.bytes().all(|b| (b'0'..=b'7').contains(&b)))
                .then(|| u8::from_str_radix(digits, 8).ok())
                .flatten()
        },
        3,
        "",
    )
}

/// Décode `\x20` des noms de `/dev/disk/by-label` (échappement de udev).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn unescape_udev_label(s: &str) -> String {
    decode_escapes(s, |hex| u8::from_str_radix(hex, 16).ok(), 2, "x")
}

/// Remplace `\<prefix><n chiffres>` par l'octet décodé ; travaille sur des octets pour
/// reconstituer l'UTF-8 multioctet (`\303\251` = « é »).
fn decode_escapes(
    s: &str,
    decode: impl Fn(&str) -> Option<u8>,
    width: usize,
    prefix: &str,
) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let start = i + 1 + prefix.len();
        let decoded = if bytes[i] == b'\\' && s.get(i + 1..start) == Some(prefix) {
            s.get(start..start + width).and_then(&decode)
        } else {
            None
        };
        match decoded {
            Some(b) => {
                out.push(b);
                i = start + width;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p3 / btrfs rw,relatime,ssd,subvol=/root 0 0
/dev/nvme0n1p3 /home btrfs rw,relatime,ssd,subvol=/home 0 0
/dev/nvme0n1p2 /boot ext4 rw,relatime 0 0
/dev/nvme0n1p1 /boot/efi vfat rw,relatime 0 0
/dev/loop3 /snap/core/1 squashfs ro,nodev,relatime 0 0
tmpfs /tmp tmpfs rw,nosuid,nodev 0 0
/dev/sdc1 /run/media/moi/MA\\040CLE exfat rw,nosuid,nodev,relatime 0 0
/dev/sr0 /run/media/moi/CDROM iso9660 ro,nosuid,nodev 0 0
/dev/sdd1 /mnt/lecture ext4 ro,relatime 0 0
";

    #[test]
    fn mounts_are_parsed_and_filtered() {
        let entries = parse_mounts(MOUNTS);
        assert_eq!(entries.len(), 11);
        let kept: Vec<String> = destination_mounts(&entries)
            .iter()
            .map(|e| e.mount_point.to_string_lossy().into_owned())
            .collect();
        assert_eq!(kept, vec!["/", "/run/media/moi/MA CLE"]);
    }

    #[test]
    fn longest_mount_prefix_wins() {
        let entries = parse_mounts(MOUNTS);
        let m = mount_of(&entries, Path::new("/home/moi/recup")).unwrap();
        assert_eq!(m.mount_point, PathBuf::from("/home"));
        let m = mount_of(&entries, Path::new("/tmp/x")).unwrap();
        assert_eq!(m.fs_type, "tmpfs");
        // `/boot` ne doit pas capturer `/bootleg`.
        let m = mount_of(&entries, Path::new("/bootleg")).unwrap();
        assert_eq!(m.mount_point, PathBuf::from("/"));
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(unescape_octal(r"MA\040CLE"), "MA CLE");
        assert_eq!(unescape_octal(r"caf\303\251"), "café");
        assert_eq!(unescape_octal(r"fin\04"), r"fin\04");
        assert_eq!(unescape_udev_label(r"MA\x20CLE"), "MA CLE");
        assert_eq!(unescape_udev_label(r"sans"), "sans");
        assert_eq!(unescape_udev_label(r"\xZZ"), r"\xZZ");
    }

    #[test]
    fn unknown_disk_counts_as_forbidden() {
        let v = Volume {
            path: PathBuf::from("E:\\"),
            label: String::new(),
            filesystem: "NTFS".into(),
            total_bytes: 0,
            free_bytes: 0,
            disk: None,
            removable: true,
        };
        assert!(v.is_on_disk(&DiskId::PhysicalDrive(0)));
        let v = Volume {
            disk: Some(DiskId::PhysicalDrive(2)),
            ..v
        };
        assert!(!v.is_on_disk(&DiskId::PhysicalDrive(0)));
        assert!(v.is_on_disk(&DiskId::PhysicalDrive(2)));
    }
}
