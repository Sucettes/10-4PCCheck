//! Rapport d'un téléphone : assemblage pur des sorties brutes des commandes adb.
//!
//! `Adb::collect` lance les commandes et remplit `RawCollection` ; `assemble` en fait un
//! `PhoneReport`. Séparer les deux permet de tester tout l'assemblage sans téléphone.

use serde::Serialize;

use crate::accounts::{parse_dumpsys_account, AccountCount};
use crate::battery::{parse_dumpsys_battery, BatteryInfo};
use crate::owners::{parse_dpm_list_owners, parse_dumpsys_device_policy, DeviceOwners};
use crate::props::{
    identity_from_props, parse_getprop, parse_which_su, security_from_props, DeviceIdentity,
    SecurityState,
};
use crate::storage::{parse_df, StorageInfo};

/// L'IMEI n'est pas lisible par ADB sans privilèges (Android 10 et plus).
pub const IMEI_NOTE: &str = "L'IMEI n'est pas lisible par ADB sans privilèges. Compose *#06# sur \
     le téléphone, compare avec la boîte et le tiroir SIM, puis vérifie-le sur un service de liste \
     noire (téléphone déclaré perdu ou volé).";

/// L'usure de la mémoire flash (UFS/eMMC) exige le root.
pub const FLASH_WEAR_NOTE: &str =
    "Usure de la mémoire interne (UFS/eMMC) non mesurable sans accès root.";

/// Étape de la collecte, pour situer un problème.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectStep {
    Root,
    Battery,
    Accounts,
    Owners,
    Storage,
}

/// Une information qui n'a pas pu être lue. Le reste du rapport reste valable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CollectIssue {
    pub step: CollectStep,
    pub message: String,
}

/// Sorties brutes des commandes. `None` : commande non lancée ou échouée (délai dépassé...).
#[derive(Debug, Clone, Default)]
pub struct RawCollection {
    /// `getprop` : seule sortie obligatoire.
    pub getprop: String,
    /// `which su`
    pub which_su: Option<String>,
    /// `dumpsys battery`
    pub battery: Option<String>,
    /// `cat` de `cycle_count`, `charge_full`, `charge_full_design` dans `SYSFS_BATTERY_DIR`.
    pub cycle_count: Option<String>,
    pub charge_full: Option<String>,
    pub charge_full_design: Option<String>,
    /// `dumpsys account`
    pub accounts: Option<String>,
    /// `dpm list-owners`
    pub owners: Option<String>,
    /// `dumpsys device_policy` : repli si `dpm list-owners` n'est pas reconnu.
    pub device_policy: Option<String>,
    /// `df /data`
    pub storage: Option<String>,
}

/// Rapport d'un téléphone, destiné à l'affichage et au rapport enregistré.
/// Contient le numéro de série (outil personnel), mais ni adresse de compte ni IMEI : ils ne
/// servent pas au diagnostic.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhoneReport {
    pub serial: String,
    pub identity: DeviceIdentity,
    pub security: SecurityState,
    pub battery: Option<BatteryInfo>,
    /// Comptes par type. `None` : liste illisible (l'absence de compte n'est pas prouvée).
    pub accounts: Option<Vec<AccountCount>>,
    /// `None` : état de gestion d'entreprise inconnu.
    pub owners: Option<DeviceOwners>,
    pub storage: Option<StorageInfo>,
    pub imei_note: String,
    pub flash_wear_note: String,
    pub issues: Vec<CollectIssue>,
}

/// Construit le rapport à partir des sorties brutes. Une sortie présente mais illisible ajoute
/// un `CollectIssue` ; une sortie absente est laissée à l'appelant (qui connaît l'erreur).
pub fn assemble(serial: &str, raw: &RawCollection) -> PhoneReport {
    let props = parse_getprop(&raw.getprop);
    let mut issues = Vec::new();
    let mut unreadable = |step, what: &str| {
        issues.push(CollectIssue {
            step,
            message: format!("Sortie de « {what} » illisible."),
        });
    };

    let mut security = security_from_props(&props);
    security.root.su_found = raw.which_su.as_deref().map(parse_which_su);

    let battery = raw.battery.as_deref().and_then(|text| {
        let parsed = parse_dumpsys_battery(text);
        if parsed.is_none() {
            unreadable(CollectStep::Battery, "dumpsys battery");
        }
        parsed.map(|b| {
            b.with_sysfs(
                raw.cycle_count.as_deref(),
                raw.charge_full.as_deref(),
                raw.charge_full_design.as_deref(),
            )
        })
    });

    let accounts = raw.accounts.as_deref().and_then(|text| {
        let parsed = parse_dumpsys_account(text);
        if parsed.is_none() {
            unreadable(CollectStep::Accounts, "dumpsys account");
        }
        parsed
    });

    let owners = raw
        .owners
        .as_deref()
        .and_then(parse_dpm_list_owners)
        .or_else(|| {
            raw.device_policy
                .as_deref()
                .and_then(parse_dumpsys_device_policy)
        });
    if owners.is_none() && (raw.owners.is_some() || raw.device_policy.is_some()) {
        unreadable(CollectStep::Owners, "dpm list-owners");
    }

    let storage = raw.storage.as_deref().and_then(|text| {
        let parsed = parse_df(text);
        if parsed.is_none() {
            unreadable(CollectStep::Storage, "df /data");
        }
        parsed
    });

    PhoneReport {
        serial: serial.to_string(),
        identity: identity_from_props(&props),
        security,
        battery,
        accounts,
        owners,
        storage,
        imei_note: IMEI_NOTE.to_string(),
        flash_wear_note: FLASH_WEAR_NOTE.to_string(),
        issues,
    }
}
