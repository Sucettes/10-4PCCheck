//! Batterie : `adb shell dumpsys battery`, complété au mieux par les fichiers de
//! `/sys/class/power_supply/battery/` (souvent lisibles sans root, selon le fabricant).
//!
//! Format de dumpsys : lignes `  clé: valeur` sous « Current Battery Service state: ».
//! Les anciennes versions (Android 4) écrivent parfois `voltage:3872` sans espace et ont moins de
//! champs ; les récentes ajoutent `Charge counter`, `Charging state`, etc. Samsung ajoute ses
//! propres lignes (`mSecPlugTypeSummary`...), ignorées. Une ligne « UPDATES STOPPED » signale que
//! les valeurs ont été figées par `dumpsys battery set` : niveau et température peuvent être faux.

use std::collections::BTreeMap;

use serde::Serialize;

/// Santé selon Android (`BatteryManager.BATTERY_HEALTH_*`). Grossière : « bonne » veut dire
/// « aucune panne détectée », pas « peu usée ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryHealth {
    Unknown,
    Good,
    Overheat,
    Dead,
    OverVoltage,
    UnspecifiedFailure,
    Cold,
    Other(i64),
}

impl BatteryHealth {
    pub fn from_code(code: i64) -> Self {
        match code {
            1 => BatteryHealth::Unknown,
            2 => BatteryHealth::Good,
            3 => BatteryHealth::Overheat,
            4 => BatteryHealth::Dead,
            5 => BatteryHealth::OverVoltage,
            6 => BatteryHealth::UnspecifiedFailure,
            7 => BatteryHealth::Cold,
            other => BatteryHealth::Other(other),
        }
    }

    pub fn label(self) -> String {
        match self {
            BatteryHealth::Unknown => "Inconnue".into(),
            BatteryHealth::Good => "Bonne".into(),
            BatteryHealth::Overheat => "Surchauffe".into(),
            BatteryHealth::Dead => "Hors d'usage".into(),
            BatteryHealth::OverVoltage => "Surtension".into(),
            BatteryHealth::UnspecifiedFailure => "Défaillance non précisée".into(),
            BatteryHealth::Cold => "Trop froide".into(),
            BatteryHealth::Other(c) => format!("Code inconnu ({c})"),
        }
    }
}

/// État de charge (`BatteryManager.BATTERY_STATUS_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeStatus {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
    Other(i64),
}

impl ChargeStatus {
    pub fn from_code(code: i64) -> Self {
        match code {
            1 => ChargeStatus::Unknown,
            2 => ChargeStatus::Charging,
            3 => ChargeStatus::Discharging,
            4 => ChargeStatus::NotCharging,
            5 => ChargeStatus::Full,
            other => ChargeStatus::Other(other),
        }
    }

    pub fn label(self) -> String {
        match self {
            ChargeStatus::Unknown => "Inconnu".into(),
            ChargeStatus::Charging => "En charge".into(),
            ChargeStatus::Discharging => "Sur batterie".into(),
            ChargeStatus::NotCharging => "Branché, ne charge pas".into(),
            ChargeStatus::Full => "Pleine".into(),
            ChargeStatus::Other(c) => format!("Code inconnu ({c})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatteryInfo {
    /// Niveau de charge en %, ramené à l'échelle 100 (`level * 100 / scale`).
    pub level_pct: Option<u8>,
    pub status: Option<ChargeStatus>,
    pub status_label: Option<String>,
    pub health: Option<BatteryHealth>,
    pub health_label: Option<String>,
    /// Température brute en dixièmes de °C (290 = 29,0 °C).
    pub temperature_decicelsius: Option<i32>,
    /// Même valeur en °C, pour l'affichage.
    pub temperature_c: Option<f64>,
    /// Tension en millivolts (ramenée en mV si l'appareil la donne en V ou en µV).
    pub voltage_mv: Option<u32>,
    pub technology: Option<String>,
    pub present: Option<bool>,
    /// Charge actuelle en µAh (Android 5 et plus, pas toujours renseignée).
    pub charge_counter_uah: Option<u64>,
    /// Cycles de charge : dumpsys (certaines versions) ou `cycle_count` de sysfs.
    pub cycle_count: Option<u32>,
    /// Capacité pleine actuelle et d'origine en µAh (sysfs `charge_full`, `charge_full_design`).
    pub charge_full_uah: Option<u64>,
    pub charge_full_design_uah: Option<u64>,
    /// Capacité actuelle en % de la capacité d'origine. `None` si non calculable ou incohérente.
    pub capacity_pct: Option<u16>,
    /// Valeurs figées par `dumpsys battery set` (ligne « UPDATES STOPPED »).
    pub updates_stopped: bool,
}

/// Bornes de plausibilité du rapport capacité actuelle / d'origine. En dehors, les deux fichiers
/// sysfs n'ont probablement pas la même unité (mAh contre µAh) : on ne calcule rien.
const CAPACITY_PLAUSIBLE_MIN: u64 = 10;
const CAPACITY_PLAUSIBLE_MAX: u64 = 150;

/// Dossier sysfs de la batterie sur la plupart des téléphones.
pub const SYSFS_BATTERY_DIR: &str = "/sys/class/power_supply/battery";

/// Analyse `dumpsys battery`. `None` si la sortie ne contient aucun champ connu
/// (service absent, refus d'accès).
pub fn parse_dumpsys_battery(text: &str) -> Option<BatteryInfo> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut updates_stopped = false;
    for line in text.lines() {
        let line = line.trim();
        if line.contains("UPDATES STOPPED") {
            updates_stopped = true;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        // Première occurrence gardée : l'état principal est imprimé en premier.
        fields
            .entry(key.trim().to_ascii_lowercase())
            .or_insert_with(|| value.trim().to_string());
    }
    let int = |key: &str| fields.get(key).and_then(|v| v.parse::<i64>().ok());
    let text_field = |key: &str| fields.get(key).filter(|v| !v.is_empty()).cloned();

    let level = int("level");
    let status = int("status").map(ChargeStatus::from_code);
    let health = int("health").map(BatteryHealth::from_code);
    if level.is_none() && status.is_none() && health.is_none() {
        return None;
    }
    let scale = int("scale").filter(|s| *s > 0).unwrap_or(100);
    let temperature = int("temperature").and_then(|t| i32::try_from(t).ok());
    let cycle_count = ["cycle count", "cycle_count", "battery cycle count"]
        .iter()
        .find_map(|k| int(k))
        .and_then(|c| u32::try_from(c).ok());

    Some(BatteryInfo {
        level_pct: level.map(|l| (l * 100 / scale).clamp(0, 100) as u8),
        status,
        status_label: status.map(ChargeStatus::label),
        health,
        health_label: health.map(BatteryHealth::label),
        temperature_decicelsius: temperature,
        temperature_c: temperature.map(|t| f64::from(t) / 10.0),
        voltage_mv: int("voltage").and_then(normalize_voltage_mv),
        technology: text_field("technology"),
        present: text_field("present").map(|v| v == "true"),
        charge_counter_uah: int("charge counter")
            .filter(|c| *c > 0)
            .and_then(|c| u64::try_from(c).ok()),
        cycle_count,
        charge_full_uah: None,
        charge_full_design_uah: None,
        capacity_pct: None,
        updates_stopped,
    })
}

/// Selon les appareils, la tension est en V (4), en mV (4012) ou en µV (4012000).
fn normalize_voltage_mv(raw: i64) -> Option<u32> {
    let mv = match raw {
        r if r <= 0 => return None,
        r if r < 100 => r * 1000,
        r if r > 100_000 => r / 1000,
        r => r,
    };
    u32::try_from(mv).ok()
}

/// Lit un entier positif dans le contenu d'un fichier sysfs. `None` pour un message d'erreur
/// (« Permission denied », « No such file ») ou une valeur nulle.
pub fn parse_sysfs_number(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok().filter(|n| *n > 0)
}

impl BatteryInfo {
    /// Complète avec les fichiers sysfs (contenu brut de `cat`, `None` si non lu).
    /// Le nombre de cycles de dumpsys, s'il existe, reste prioritaire.
    pub fn with_sysfs(
        mut self,
        cycle_count: Option<&str>,
        charge_full: Option<&str>,
        charge_full_design: Option<&str>,
    ) -> Self {
        if self.cycle_count.is_none() {
            self.cycle_count = cycle_count
                .and_then(parse_sysfs_number)
                .and_then(|c| u32::try_from(c).ok());
        }
        self.charge_full_uah = charge_full.and_then(parse_sysfs_number);
        self.charge_full_design_uah = charge_full_design.and_then(parse_sysfs_number);
        self.capacity_pct = match (self.charge_full_uah, self.charge_full_design_uah) {
            (Some(full), Some(design)) => {
                // Arrondi vers le bas : 79,5 % ne doit pas passer au vert (seuil « ≥ 80 % »).
                let pct = full * 100 / design;
                (CAPACITY_PLAUSIBLE_MIN..=CAPACITY_PLAUSIBLE_MAX)
                    .contains(&pct)
                    .then_some(pct as u16)
            }
            _ => None,
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voltage_units_are_normalized() {
        assert_eq!(normalize_voltage_mv(4), Some(4000));
        assert_eq!(normalize_voltage_mv(3872), Some(3872));
        assert_eq!(normalize_voltage_mv(4_012_000), Some(4012));
        assert_eq!(normalize_voltage_mv(0), None);
    }

    #[test]
    fn unreadable_output_gives_none() {
        assert!(parse_dumpsys_battery("Can't find service: battery\n").is_none());
        assert!(parse_dumpsys_battery("").is_none());
    }

    #[test]
    fn sysfs_values_and_capacity() {
        let b = parse_dumpsys_battery("level: 50\nhealth: 2\n")
            .unwrap()
            .with_sysfs(Some("412\n"), Some("3120000\n"), Some("4000000\n"));
        assert_eq!(b.cycle_count, Some(412));
        assert_eq!(b.capacity_pct, Some(78));

        let denied = "cat: /sys/class/power_supply/battery/charge_full: Permission denied\n";
        let b = parse_dumpsys_battery("level: 50\n").unwrap().with_sysfs(
            None,
            Some(denied),
            Some("4000000"),
        );
        assert_eq!(b.charge_full_uah, None);
        assert_eq!(b.capacity_pct, None);

        // Unités différentes (mAh contre µAh) : rapport absurde, rien n'est calculé.
        let b = parse_dumpsys_battery("level: 50\n").unwrap().with_sysfs(
            None,
            Some("3120"),
            Some("4000000"),
        );
        assert_eq!(b.capacity_pct, None);
    }
}
