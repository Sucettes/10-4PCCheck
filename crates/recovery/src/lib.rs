//! Récupération de fichiers supprimés : interface de programmation devant PhotoRec
//! (CGSecurity), lancé en sous-processus en mode script (`/cmd`).
//!
//! Déroulé type côté interface :
//! 1. `list_volumes()` pour proposer une destination, en écartant `Volume::is_on_disk(source)` ;
//! 2. `trim_warning()` et `SUPPORT_HELP` à côté de la source choisie ;
//! 3. `RecoveryJob::start(config, &locate_photorec(&dirs)?)` ;
//! 4. `job.progress()` toutes les secondes ; `job.stop()` sur demande ;
//! 5. `list_found(dest, n)` pour parcourir le résultat.
//!
//! Règle stricte du plan : la destination est sur un autre disque physique que la source
//! (`validate_destination`, appelée par `RecoveryJob::start` avant toute écriture).

pub mod advice;
pub mod config;
pub mod disk;
pub mod error;
pub mod job;
pub mod log;
pub mod scan;
mod sys;
pub mod tsk;
pub mod volumes;

pub use advice::{trim_warning, SUPPORT_HELP};
pub use config::{build_args, FileFamily, RecoveryConfig, Source};
pub use disk::{
    check_location, disk_from_smartctl_name, disk_from_smartctl_name_for, locate_destination,
    photorec_device_from_smartctl, validate_destination, DestinationLocation, DiskId, Platform,
};
pub use error::RecoveryError;
pub use job::{locate_photorec, RecoveryJob, RecoveryProgress};
pub use log::{parse_log, LogSummary};
pub use scan::{count_found, list_found, FoundFile, FoundSummary};
pub use tsk::{
    list_deleted, locate_tsk, parse_fls, volume_device, DeletedFile, DeletedList, TskJob,
    TskProgress,
};
pub use volumes::{list_volumes, Volume};
