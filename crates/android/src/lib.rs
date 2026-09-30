//! Analyse d'un téléphone Android usagé par ADB, avant un achat.
//!
//! Organisation :
//! - `adb` : localisation d'adb, version, serveur, liste des appareils, collecte.
//! - modules d'analyse purs (`devices`, `props`, `battery`, `accounts`, `owners`, `storage`) :
//!   une fonction `parse_*` par sortie de commande, testable sans téléphone.
//! - `collect` : assemblage des sorties brutes en `PhoneReport`.
//! - `evaluate` : constats vert/jaune/rouge selon les règles du plan.
//! - `checklist` : vérifications manuelles.
//!
//! Contenu du rapport : numéro de série en clair (outil personnel) ; ni l'adresse d'un compte
//! (seulement le nombre de comptes par type) ni l'IMEI, inutiles au diagnostic.

pub mod accounts;
pub mod adb;
pub mod battery;
pub mod checklist;
pub mod collect;
pub mod date;
pub mod devices;
pub mod evaluate;
pub mod owners;
pub mod props;
pub mod storage;

pub use accounts::{parse_dumpsys_account, AccountCount};
pub use adb::{parse_version, validate_serial, Adb, AdbError, AdbVersion};
pub use battery::{
    parse_dumpsys_battery, parse_sysfs_number, BatteryHealth, BatteryInfo, ChargeStatus,
};
pub use checklist::{manual_checklist, ChecklistItem};
pub use collect::{assemble, CollectIssue, CollectStep, PhoneReport, RawCollection};
pub use date::{today, Date};
pub use devices::{
    no_device_guidance, no_device_guidance_for, parse_devices, AdbDevice, DeviceState,
};
pub use evaluate::{evaluate, verdict, Finding, FindingLevel};
pub use owners::{parse_dpm_list_owners, parse_dumpsys_device_policy, DeviceOwners, ProfileOwner};
pub use props::{
    identity_from_props, parse_getprop, parse_which_su, security_from_props, DeviceIdentity,
    RootHints, SecurityState, VerifiedBootState,
};
pub use storage::{parse_df, StorageInfo};
