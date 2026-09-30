//! Moteur de collecte de PCCheck.
//!
//! Aucune dépendance à l'interface : l'application Tauri et les tests appellent
//! les mêmes fonctions, et tout ce qui sort d'ici est sérialisable en JSON.

pub mod age;
pub mod attributes;
pub mod capacity;
pub mod checks;
pub mod disk;
pub mod process;
pub mod rawio;
pub mod selftest;
pub mod smartctl;
pub mod speed;
pub mod surface;
pub mod usb;

pub use attributes::AttributeStatus;
pub use checks::{Check, CheckLevel};
pub use disk::{
    dedupe_disks, parse_disk, parse_scan, AtaAttribute, DiskEntry, DiskInfo, MediaKind, NvmeHealth,
    Protocol, ScanDevice,
};
pub use process::{ProcessError, ProcessOutput};
pub use selftest::{parse_self_test_status, SelfTestKind, SelfTestResult, SelfTestStatus};
pub use smartctl::{Smartctl, SmartctlError};
