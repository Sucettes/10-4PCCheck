//! Collecte sous Linux : `/sys`, `/proc`, `dmidecode` (root) et `lspci`. Toute l'analyse est
//! dans `linux_parse` ; ce module ne fait que lire les fichiers et lancer les outils.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pccheck_core::process;

use crate::error::{record, InventoryError};
use crate::linux_parse;
use crate::model::{
    BiosInfo, BoardInfo, ComputerInfo, CpuInfo, GpuInfo, MachineInventory, MemoryInfo,
    MemoryModule, NetworkAdapter, NetworkKind, OsInfo, SecureBootState, SecurityInfo, Sensor,
    TpmInfo,
};
use crate::parse::{self, BatteryReading};

const DMI_DIR: &str = "/sys/class/dmi/id";
const SECURE_BOOT_VAR: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
const DMIDECODE: [&str; 3] = [
    "/usr/sbin/dmidecode",
    "/sbin/dmidecode",
    "/usr/bin/dmidecode",
];
const LSPCI: [&str; 4] = [
    "/usr/bin/lspci",
    "/usr/sbin/lspci",
    "/bin/lspci",
    "/sbin/lspci",
];
const TOOL_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn collect(inv: &mut MachineInventory) {
    let errors = &mut inv.errors;
    inv.os = Some(read_os());
    inv.computer = Some(ComputerInfo {
        manufacturer: dmi("sys_vendor"),
        model: dmi("product_name"),
        serial_number: record(errors, read_dmi_serial()).flatten(),
    });
    inv.bios = Some(BiosInfo {
        vendor: dmi("bios_vendor"),
        version: dmi("bios_version"),
        release_date: dmi("bios_date").as_deref().and_then(parse::dmi_date),
    });
    inv.board = Some(BoardInfo {
        manufacturer: dmi("board_vendor"),
        product: dmi("board_name"),
    });
    inv.cpu = record(errors, read_cpu());
    inv.memory = Some(MemoryInfo {
        total_bytes: read_to_string("/proc/meminfo")
            .ok()
            .and_then(|m| linux_parse::meminfo_bytes(&m, "MemTotal")),
        modules: record(errors, read_memory_modules()).unwrap_or_default(),
    });
    inv.gpus = record(errors, read_gpus()).unwrap_or_default();
    inv.network_adapters = read_network();
    inv.battery = read_battery();
    inv.security = Some(SecurityInfo {
        secure_boot: record(errors, read_secure_boot()),
        tpm: Some(read_tpm()),
        ..SecurityInfo::default()
    });
    inv.temperatures = record(errors, read_hwmon()).unwrap_or_default();
}

/// Sonde de température pour le test de charge : maximum des capteurs hwmon.
pub(crate) fn temperature_probe() -> impl FnMut() -> Option<f64> {
    || {
        read_hwmon()
            .ok()?
            .iter()
            .map(|s| s.celsius)
            .reduce(f64::max)
    }
}

pub(crate) fn available_memory_bytes() -> Option<u64> {
    let text = read_to_string("/proc/meminfo").ok()?;
    linux_parse::meminfo_bytes(&text, "MemAvailable")
}

// ---------- Outils ----------

fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    fs::read_to_string(path)
}

fn dmi(name: &str) -> Option<String> {
    read_to_string(Path::new(DMI_DIR).join(name))
        .ok()
        .and_then(|s| parse::clean_smbios(&s))
}

fn first_existing(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

fn io_error(part: &str, e: &io::Error) -> InventoryError {
    if e.kind() == io::ErrorKind::PermissionDenied {
        InventoryError::admin(part)
    } else {
        InventoryError::unreadable(part, e)
    }
}

// ---------- Système et matériel ----------

fn read_os() -> OsInfo {
    let mut os = read_to_string("/etc/os-release")
        .or_else(|_| read_to_string("/usr/lib/os-release"))
        .map(|t| linux_parse::parse_os_release(&t))
        .unwrap_or_default();
    os.build = read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if os.name.is_none() {
        os.name = Some("Linux".into());
    }
    os
}

/// Le numéro de série n'est lisible qu'en root (`product_serial` en mode 0400).
fn read_dmi_serial() -> Result<Option<String>, InventoryError> {
    match read_to_string(Path::new(DMI_DIR).join("product_serial")) {
        Ok(s) => Ok(parse::clean_smbios(&s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error("Numéro de série", &e)),
    }
}

fn read_cpu() -> Result<CpuInfo, InventoryError> {
    let text = read_to_string("/proc/cpuinfo").map_err(|e| io_error("Processeur", &e))?;
    let mut cpu = linux_parse::parse_cpuinfo(&text);
    // Fréquence de base exposée par intel_pstate, en kHz.
    let base_khz = read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/base_frequency")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    if let Some(khz) = base_khz.filter(|k| *k > 0) {
        cpu.base_mhz = Some(khz / 1000);
    }
    Ok(cpu)
}

fn read_memory_modules() -> Result<Vec<MemoryModule>, InventoryError> {
    let part = "Barrettes mémoire (dmidecode)";
    let path = first_existing(&DMIDECODE)
        .ok_or_else(|| InventoryError::unreadable(part, "dmidecode introuvable"))?;
    let out = process::run(&path, &["-t", "memory"], TOOL_TIMEOUT)
        .map_err(|e| InventoryError::process(part, e))?;
    let modules = linux_parse::parse_dmidecode_memory(&out.stdout);
    if modules.is_empty() && out.code != Some(0) {
        // Sans root, dmidecode ne peut pas lire la table SMBIOS et sort en erreur.
        return Err(InventoryError::admin(part));
    }
    Ok(modules)
}

fn read_gpus() -> Result<Vec<GpuInfo>, InventoryError> {
    let part = "Cartes graphiques (lspci)";
    let path = first_existing(&LSPCI)
        .ok_or_else(|| InventoryError::unreadable(part, "lspci introuvable"))?;
    let out = process::run(&path, &["-mm"], TOOL_TIMEOUT)
        .map_err(|e| InventoryError::process(part, e))?;
    Ok(linux_parse::parse_lspci_mm(&out.stdout))
}

/// Interfaces rattachées à un périphérique réel (lien `device`), sans l'adresse MAC.
/// Les adaptateurs Bluetooth ne sont pas des interfaces réseau : lus dans `/sys/class/bluetooth`.
fn read_network() -> Vec<NetworkAdapter> {
    let mut adapters = Vec::new();
    for (name, path) in dir_entries("/sys/class/net") {
        if !path.join("device").exists() {
            continue;
        }
        let wireless = path.join("wireless").exists() || path.join("phy80211").exists();
        let arphrd = read_to_string(path.join("type"))
            .ok()
            .and_then(|t| t.trim().parse().ok());
        adapters.push(NetworkAdapter {
            kind: linux_parse::linux_net_kind(wireless, arphrd),
            name,
        });
    }
    for (name, _) in dir_entries("/sys/class/bluetooth") {
        // hci0 : l'adaptateur ; hci0:1 : une connexion en cours.
        if !name.contains(':') {
            adapters.push(NetworkAdapter {
                name,
                kind: NetworkKind::Bluetooth,
            });
        }
    }
    adapters
}

/// Entrées d'un dossier (nom, chemin), triées par nom. Dossier absent : liste vide.
fn dir_entries(dir: &str) -> Vec<(String, PathBuf)> {
    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    entries
}

fn read_battery() -> Option<crate::model::BatteryInfo> {
    let readings: Vec<BatteryReading> = dir_entries("/sys/class/power_supply")
        .into_iter()
        .filter_map(|(_, path)| read_to_string(path.join("uevent")).ok())
        .filter_map(|uevent| linux_parse::parse_power_supply_uevent(&uevent))
        .collect();
    parse::merge_batteries(&readings)
}

fn read_hwmon() -> Result<Vec<Sensor>, InventoryError> {
    let mut sensors = Vec::new();
    for (hwmon, path) in dir_entries("/sys/class/hwmon") {
        let chip = read_to_string(path.join("name"))
            .map(|n| n.trim().to_string())
            .unwrap_or(hwmon);
        for (file, input) in dir_entries(&path.to_string_lossy()) {
            if !(file.starts_with("temp") && file.ends_with("_input")) {
                continue;
            }
            let Ok(raw) = read_to_string(&input) else {
                continue;
            };
            let label_file = format!("{}_label", file.trim_end_matches("_input"));
            let label = read_to_string(path.join(label_file)).ok();
            if let Some(s) = linux_parse::hwmon_sensor(&chip, label.as_deref(), &file, &raw) {
                sensors.push(s);
            }
        }
    }
    if sensors.is_empty() {
        return Err(InventoryError::unreadable(
            "Températures",
            "aucun capteur dans /sys/class/hwmon",
        ));
    }
    Ok(sensors)
}

// ---------- Sécurité ----------

fn read_secure_boot() -> Result<SecureBootState, InventoryError> {
    let part = "Secure Boot";
    if !Path::new("/sys/firmware/efi").exists() {
        return Ok(SecureBootState::Unsupported);
    }
    let bytes = fs::read(SECURE_BOOT_VAR).map_err(|e| io_error(part, &e))?;
    linux_parse::parse_efivar_secure_boot(&bytes)
        .ok_or_else(|| InventoryError::unreadable(part, "variable EFI illisible"))
}

/// Présence et version du TPM. `tpm_version_major` n'existe que sur les noyaux récents.
fn read_tpm() -> TpmInfo {
    let Some((_, path)) = dir_entries("/sys/class/tpm")
        .into_iter()
        .find(|(name, _)| name.starts_with("tpm"))
    else {
        return TpmInfo::default();
    };
    let version = read_to_string(path.join("tpm_version_major"))
        .ok()
        .and_then(|v| match v.trim() {
            "2" => Some("2.0".to_string()),
            "1" => Some("1.2".to_string()),
            _ => None,
        });
    TpmInfo {
        present: true,
        version,
        manufacturer: None,
    }
}
