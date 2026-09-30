//! Moteur de collecte de 10-4 PCCheck.
//!
//! Aucune dépendance à l'interface : l'application Tauri et les tests appellent
//! les mêmes fonctions, et tout ce qui sort d'ici est sérialisable en JSON.

pub mod attributes;
pub mod checks;
pub mod disk;
pub mod process;
pub mod selftest;
pub mod smartctl;

pub use attributes::AttributeStatus;
pub use checks::{Check, CheckLevel};
pub use disk::{
    dedupe_disks, parse_disk, parse_scan, AtaAttribute, DiskEntry, DiskInfo, MediaKind, NvmeHealth,
    Protocol, ScanDevice,
};
pub use process::{ProcessError, ProcessOutput};
pub use selftest::{parse_self_test_status, SelfTestKind, SelfTestResult, SelfTestStatus};
pub use smartctl::{Smartctl, SmartctlError};
