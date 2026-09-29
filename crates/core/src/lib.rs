//! Moteur de collecte de 10-4 PCCheck.
//!
//! Aucune dépendance à l'interface : l'application Tauri et les tests appellent
//! les mêmes fonctions, et tout ce qui sort d'ici est sérialisable en JSON.

pub mod disk;
pub mod smartctl;

pub use disk::{
    dedupe_disks, parse_disk, parse_scan, AtaAttribute, DiskEntry, DiskInfo, MediaKind, NvmeHealth,
    Protocol, ScanDevice,
};
pub use smartctl::{Smartctl, SmartctlError};
