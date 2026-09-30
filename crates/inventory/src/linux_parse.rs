//! Analyse des formats texte de Linux (`/proc`, `/sys`, `dmidecode`, `lspci`). Fonctions pures,
//! compilées et testées sur toutes les plateformes ; seule la lecture des fichiers est propre
//! à Linux (module `linux`).

use std::collections::BTreeSet;

use crate::model::{CpuInfo, GpuInfo, MemoryModule, NetworkKind, OsInfo, SecureBootState, Sensor};
use crate::parse::{self, BatteryReading};

/// Blocs de lignes séparés par une ligne vide. Accepte les fins de ligne LF et CRLF
/// (fixtures extraites sous Windows).
fn blocks(text: &str) -> Vec<Vec<&str>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Valeur d'un champ « Clé : valeur » d'un bloc (première occurrence, clé exacte).
fn block_field<'a>(block: &[&'a str], key: &str) -> Option<&'a str> {
    block.iter().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

/// `/etc/os-release` : `PRETTY_NAME` (ou `NAME`) et `VERSION_ID`. Le noyau est ajouté par
/// l'appelant dans `build`.
pub fn parse_os_release(text: &str) -> OsInfo {
    let get = |key: &str| {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key)
                .then(|| v.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|v| !v.is_empty())
        })
    };
    OsInfo {
        name: get("PRETTY_NAME").or_else(|| get("NAME")),
        version: get("VERSION_ID"),
        build: None,
        display_version: None,
    }
}

/// `/proc/cpuinfo` (x86) : nom du modèle, fils = entrées `processor`, cœurs = couples
/// (`physical id`, `core id`) distincts. Fréquence de base tirée du nom (« @ 2.60GHz »),
/// que l'appelant remplace par `cpufreq/base_frequency` si le noyau la donne.
pub fn parse_cpuinfo(text: &str) -> CpuInfo {
    let mut name = None;
    let mut threads = 0u32;
    let mut cores = BTreeSet::new();
    for block in blocks(text) {
        let field = |key: &str| block_field(&block, key).map(str::to_string);
        if field("processor").is_none() {
            continue;
        }
        threads += 1;
        if name.is_none() {
            name = field("model name").and_then(|n| parse::clean_smbios(&n));
        }
        if let Some(core) = field("core id") {
            cores.insert((field("physical id").unwrap_or_default(), core));
        }
    }
    let base_mhz = name.as_deref().and_then(mhz_from_cpu_name);
    CpuInfo {
        name,
        cores: (!cores.is_empty()).then(|| u32::try_from(cores.len()).unwrap_or(u32::MAX)),
        threads: (threads > 0).then_some(threads),
        base_mhz,
    }
}

/// « Intel(R) Core(TM) i5-8250U CPU @ 1.60GHz » donne 1600.
pub fn mhz_from_cpu_name(name: &str) -> Option<u32> {
    let after = name
        .rsplit('@')
        .next()
        .filter(|_| name.contains('@'))?
        .trim();
    let ghz: f64 = after
        .strip_suffix("GHz")
        .or_else(|| after.strip_suffix("Ghz"))?
        .trim()
        .parse()
        .ok()?;
    let mhz = (ghz * 1000.0).round();
    (mhz > 0.0 && mhz < 100_000.0).then_some(mhz as u32)
}

/// Valeur de `/proc/meminfo` en octets (`MemTotal:  16318480 kB`).
pub fn meminfo_bytes(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        if k.trim() != key {
            return None;
        }
        let mut parts = v.split_whitespace();
        let n: u64 = parts.next()?.parse().ok()?;
        match parts.next() {
            Some("kB") => n.checked_mul(1024),
            None => Some(n),
            Some(_) => None,
        }
    })
}

/// Sortie de `dmidecode -t memory` : un bloc « Memory Device » par emplacement. Les
/// emplacements vides (« No Module Installed ») sont ignorés.
pub fn parse_dmidecode_memory(text: &str) -> Vec<MemoryModule> {
    let mut modules = Vec::new();
    for block in blocks(text) {
        if !block.iter().any(|l| l.trim() == "Memory Device") {
            continue;
        }
        let field = |key: &str| block_field(&block, key);
        let Some(capacity) = field("Size").and_then(parse_dmi_size) else {
            continue;
        };
        let speed = field("Configured Memory Speed")
            .or_else(|| field("Configured Clock Speed"))
            .and_then(parse_dmi_speed)
            .or_else(|| field("Speed").and_then(parse_dmi_speed));
        modules.push(MemoryModule {
            capacity_bytes: Some(capacity),
            speed_mhz: speed,
            manufacturer: field("Manufacturer").and_then(parse::clean_smbios),
            part_number: field("Part Number").and_then(parse::clean_smbios),
            slot: field("Locator").and_then(parse::clean_smbios),
            memory_type: field("Type").and_then(parse::clean_smbios),
        });
    }
    modules
}

/// « 8 GB », « 8192 MB », « 16 GiB » vers octets. `None` pour « No Module Installed ».
fn parse_dmi_size(v: &str) -> Option<u64> {
    let mut parts = v.split_whitespace();
    let n: u64 = parts.next()?.parse().ok()?;
    let mult: u64 = match parts.next()? {
        "kB" | "KB" | "KiB" => 1 << 10,
        "MB" | "MiB" => 1 << 20,
        "GB" | "GiB" => 1 << 30,
        "TB" | "TiB" => 1 << 40,
        _ => return None,
    };
    n.checked_mul(mult).filter(|b| *b > 0)
}

/// « 3200 MT/s » ou « 2667 MHz » vers 3200 / 2667. `None` pour « Unknown ».
fn parse_dmi_speed(v: &str) -> Option<u32> {
    let mut parts = v.split_whitespace();
    let n: u32 = parts.next()?.parse().ok()?;
    matches!(parts.next(), Some("MT/s" | "MHz"))
        .then_some(n)
        .filter(|n| *n > 0)
}

/// Classes PCI des cartes graphiques, en texte comme les affiche `lspci -mm`.
const GPU_CLASSES: [&str; 3] = [
    "VGA compatible controller",
    "3D controller",
    "Display controller",
];

/// Sortie de `lspci -mm` : une ligne par périphérique, champs entre guillemets
/// (`00:02.0 "VGA compatible controller" "Intel Corporation" "UHD Graphics 620" -r07 ...`).
pub fn parse_lspci_mm(text: &str) -> Vec<GpuInfo> {
    text.lines()
        .filter_map(|line| {
            let q = quoted_fields(line);
            let (class, vendor, device) = (q.first()?, q.get(1)?, q.get(2)?);
            GPU_CLASSES.contains(&class.as_str()).then(|| GpuInfo {
                name: format!("{vendor} {device}").trim().to_string(),
                driver_version: None,
                memory_bytes: None,
            })
        })
        .collect()
}

/// Chaînes entre guillemets d'une ligne, dans l'ordre (les options `-r07` sont ignorées).
fn quoted_fields(line: &str) -> Vec<String> {
    line.split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_string())
        .collect()
}

/// Fichier `uevent` d'une batterie (`/sys/class/power_supply/BAT0/uevent`). Unités du noyau :
/// µWh pour `ENERGY_*`, µAh pour `CHARGE_*`, µV pour `VOLTAGE_*`. `None` si ce n'est pas une
/// batterie du système (pile de souris sans fil : `SCOPE=Device`) ou si elle est absente.
pub fn parse_power_supply_uevent(text: &str) -> Option<BatteryReading> {
    let get = |key: &str| {
        let key = format!("POWER_SUPPLY_{key}");
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    };
    let num = |key: &str| get(key).and_then(|v| v.parse::<u64>().ok());
    if get("TYPE").is_some_and(|t| t != "Battery")
        || get("SCOPE").is_some_and(|s| s == "Device")
        || get("PRESENT").is_some_and(|p| p == "0")
    {
        return None;
    }
    let cycle_count = num("CYCLE_COUNT").and_then(|c| u32::try_from(c).ok());
    let reading = match (num("ENERGY_FULL_DESIGN"), num("ENERGY_FULL")) {
        (Some(design), Some(full)) => BatteryReading {
            design_mwh: Some(design / 1000),
            full_mwh: Some(full / 1000),
            health_pct: parse::health_pct(full, design),
            cycle_count,
        },
        _ => match (num("CHARGE_FULL_DESIGN"), num("CHARGE_FULL")) {
            (Some(design), Some(full)) => {
                // µAh × µV = 10⁻⁹ mWh. Tension minimale de conception : la référence des
                // fabricants pour annoncer la capacité en Wh.
                let to_mwh = |uah: u64| {
                    num("VOLTAGE_MIN_DESIGN")
                        .map(|uv| u128::from(uah) * u128::from(uv) / 1_000_000_000)
                        .and_then(|mwh| u64::try_from(mwh).ok())
                };
                BatteryReading {
                    design_mwh: to_mwh(design),
                    full_mwh: to_mwh(full),
                    health_pct: parse::health_pct(full, design),
                    cycle_count,
                }
            }
            _ if cycle_count.is_some() => BatteryReading {
                cycle_count,
                ..BatteryReading::default()
            },
            _ => return None,
        },
    };
    Some(reading)
}

/// Variable EFI `SecureBoot-8be4df61-...` lue dans `efivarfs` : 4 octets d'attributs puis
/// 1 octet de valeur (1 = actif).
pub fn parse_efivar_secure_boot(bytes: &[u8]) -> Option<SecureBootState> {
    match bytes.get(4)? {
        1 => Some(SecureBootState::Enabled),
        0 => Some(SecureBootState::Disabled),
        _ => None,
    }
}

/// Capteur hwmon : `temp*_input` en millidegrés. Libellé : nom de la puce puis
/// `temp*_label` (ou le nom du fichier). Lecture absurde écartée.
pub fn hwmon_sensor(
    chip: &str,
    label: Option<&str>,
    input_name: &str,
    raw: &str,
) -> Option<Sensor> {
    let milli: i64 = raw.trim().parse().ok()?;
    let celsius = milli as f64 / 1000.0;
    if !parse::plausible_celsius(celsius) {
        return None;
    }
    let what = label
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| input_name.trim_end_matches("_input"));
    Some(Sensor {
        label: format!("{} {what}", chip.trim()),
        celsius,
    })
}

/// Type d'une interface de `/sys/class/net` : sans-fil si elle a un dossier `wireless` ou
/// `phy80211`, Ethernet si son `type` vaut 1 (ARPHRD_ETHER).
pub fn linux_net_kind(is_wireless: bool, arphrd_type: Option<u32>) -> NetworkKind {
    match (is_wireless, arphrd_type) {
        (true, _) => NetworkKind::Wifi,
        (false, Some(1)) => NetworkKind::Ethernet,
        _ => NetworkKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_name_frequency() {
        assert_eq!(
            mhz_from_cpu_name("Intel(R) Core(TM) i5-8250U CPU @ 1.60GHz"),
            Some(1600)
        );
        assert_eq!(
            mhz_from_cpu_name("AMD Ryzen 7 5800U with Radeon Graphics"),
            None
        );
    }

    #[test]
    fn dmi_sizes_and_speeds() {
        assert_eq!(parse_dmi_size("8 GB"), Some(8 << 30));
        assert_eq!(parse_dmi_size("8192 MB"), Some(8 << 30));
        assert_eq!(parse_dmi_size("No Module Installed"), None);
        assert_eq!(parse_dmi_speed("3200 MT/s"), Some(3200));
        assert_eq!(parse_dmi_speed("Unknown"), None);
    }

    #[test]
    fn efivar_and_net() {
        assert_eq!(
            parse_efivar_secure_boot(&[6, 0, 0, 0, 1]),
            Some(SecureBootState::Enabled)
        );
        assert_eq!(
            parse_efivar_secure_boot(&[6, 0, 0, 0, 0]),
            Some(SecureBootState::Disabled)
        );
        assert_eq!(parse_efivar_secure_boot(&[6, 0]), None);
        assert_eq!(linux_net_kind(true, Some(1)), NetworkKind::Wifi);
        assert_eq!(linux_net_kind(false, Some(1)), NetworkKind::Ethernet);
        assert_eq!(linux_net_kind(false, Some(772)), NetworkKind::Other);
    }

    #[test]
    fn hwmon_values() {
        let s = hwmon_sensor("coretemp", Some("Package id 0"), "temp1_input", "52000\n").unwrap();
        assert_eq!(s.label, "coretemp Package id 0");
        assert!((s.celsius - 52.0).abs() < 1e-9);
        let s = hwmon_sensor("acpitz", None, "temp1_input", "27800").unwrap();
        assert_eq!(s.label, "acpitz temp1");
        assert!(hwmon_sensor("x", None, "temp1_input", "-273000").is_none());
        assert!(hwmon_sensor("x", None, "temp1_input", "abc").is_none());
    }
}
