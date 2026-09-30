//! Collecte sous Windows : WMI (crate `wmi`), registre (crate `winreg`) et `dsregcmd`.
//!
//! `WMIConnection` n'est pas `Send` : chaque fil ouvre ses propres connexions. La crate `wmi`
//! initialise COM (multithread) si le fil ne l'a pas encore fait.

use std::io;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use pccheck_core::process;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use winreg::enums::{KEY_READ, KEY_WOW64_64KEY};
use winreg::{RegKey, HKLM};
use wmi::{AuthLevel, WMIConnection, WMIError};

use crate::error::{record, InventoryError};
use crate::model::{
    AutopilotInfo, BiosInfo, BitLockerProtection, BitLockerVolume, BoardInfo, ComputerInfo,
    CpuInfo, DeviceJoin, GpuInfo, LicenseInfo, MachineInventory, MemoryInfo, MemoryModule,
    NetworkAdapter, OsInfo, SecureBootState, SecurityInfo, Sensor, TpmInfo,
};
use crate::parse::{self, clean_opt, BatteryReading};

// Codes HRESULT de WMI (https://learn.microsoft.com/windows/win32/wmisdk/wmi-error-constants).
const WBEM_E_NOT_FOUND: i32 = 0x8004_1002_u32 as i32;
const WBEM_E_ACCESS_DENIED: i32 = 0x8004_1003_u32 as i32;
const WBEM_E_NOT_SUPPORTED: i32 = 0x8004_100C_u32 as i32;
const WBEM_E_INVALID_NAMESPACE: i32 = 0x8004_100E_u32 as i32;
const WBEM_E_INVALID_CLASS: i32 = 0x8004_1010_u32 as i32;
const E_ACCESSDENIED: i32 = 0x8007_0005_u32 as i32;

/// Application « Windows » dans le service de licences.
const WINDOWS_APP_ID: &str = "55c92734-d682-4d71-983e-d6ec3f16059f";
/// Classe de périphériques « Carte graphique » dans le registre.
const DISPLAY_CLASS_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
/// `BatteryStaticData.Capabilities` : capacités en valeurs relatives, pas en mWh.
const BATTERY_CAPACITY_RELATIVE: u32 = 0x4000_0000;

pub(crate) fn collect(inv: &mut MachineInventory) {
    // La requête de licence prend plusieurs secondes et dsregcmd environ une : en parallèle.
    let (hardware, license, join) = thread::scope(|s| {
        let license = s.spawn(read_license);
        let join = s.spawn(read_device_join);
        let hardware = s.spawn(|| {
            let mut part = MachineInventory::default();
            collect_hardware(&mut part);
            part
        });
        (hardware.join(), license.join(), join.join())
    });

    match hardware {
        Ok(part) => *inv = part,
        Err(_) => inv
            .errors
            .push("Inventaire : le fil de collecte a planté".into()),
    }
    let security = inv.security.get_or_insert_with(SecurityInfo::default);
    let panicked = |part: &str| format!("{part} : le fil de collecte a planté");
    match license {
        Ok(r) => security.windows_license = record(&mut inv.errors, r),
        Err(_) => inv.errors.push(panicked("Licence Windows")),
    }
    match join {
        Ok(r) => security.device_join = record(&mut inv.errors, r),
        Err(_) => inv.errors.push(panicked("Azure AD (dsregcmd)")),
    }
}

fn collect_hardware(inv: &mut MachineInventory) {
    let errors = &mut inv.errors;
    match WMIConnection::new() {
        Ok(cim) => {
            inv.os = record(errors, read_os(&cim));
            let bios = record(errors, read_bios(&cim));
            let system = record(errors, read_computer_system(&cim));
            if let Some((bios, serial)) = bios {
                inv.bios = Some(bios);
                inv.computer
                    .get_or_insert_with(ComputerInfo::default)
                    .serial_number = serial;
            }
            let mut total_bytes = None;
            if let Some((computer, total)) = system {
                let c = inv.computer.get_or_insert_with(ComputerInfo::default);
                c.manufacturer = computer.manufacturer;
                c.model = computer.model;
                total_bytes = total;
            }
            inv.board = record(errors, read_board(&cim));
            inv.cpu = record(errors, read_cpu(&cim));
            let modules = record(errors, read_memory_modules(&cim)).unwrap_or_default();
            if total_bytes.is_some() || !modules.is_empty() {
                inv.memory = Some(MemoryInfo {
                    total_bytes,
                    modules,
                });
            }
            inv.gpus = record(errors, read_gpus(&cim)).unwrap_or_default();
        }
        Err(e) => errors.push(wmi_error("WMI (matériel)", e).to_string()),
    }
    inv.network_adapters = record(errors, read_network()).unwrap_or_default();

    match WMIConnection::with_namespace_path("ROOT\\WMI") {
        Ok(root_wmi) => {
            inv.battery = record(errors, read_battery(&root_wmi)).flatten();
            match read_thermal_zones(&root_wmi) {
                Ok(sensors) if !sensors.is_empty() => inv.temperatures = sensors,
                Ok(_) => errors.push(no_thermal_zone().to_string()),
                Err(e) => errors.push(e.to_string()),
            }
        }
        Err(e) => errors.push(wmi_error("WMI (batterie, températures)", e).to_string()),
    }

    let security = SecurityInfo {
        secure_boot: record(errors, read_secure_boot()),
        tpm: record(errors, read_tpm()),
        bitlocker: record(errors, read_bitlocker()),
        intune_enrolled: record(errors, read_intune()),
        autopilot: record(errors, read_autopilot()),
        ..SecurityInfo::default()
    };
    inv.security = Some(security);
}

/// Sonde de température pour le test de charge : température maximale des zones ACPI.
/// À créer dans le fil qui l'appelle (connexion WMI non `Send`).
pub(crate) fn temperature_probe() -> impl FnMut() -> Option<f64> {
    let con = WMIConnection::with_namespace_path("ROOT\\WMI").ok();
    move || {
        let sensors = read_thermal_zones(con.as_ref()?).ok()?;
        sensors.iter().map(|s| s.celsius).reduce(f64::max)
    }
}

pub(crate) fn available_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY : `status` est une structure valide, initialisée, avec `dwLength` renseigné
    // comme l'exige l'API ; Windows n'écrit que dans cette structure.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status.ullAvailPhys)
}

// ---------- WMI : outils ----------

fn query<T: DeserializeOwned>(
    con: &WMIConnection,
    part: &str,
    q: &str,
) -> Result<Vec<T>, InventoryError> {
    con.raw_query(q).map_err(|e| wmi_error(part, e))
}

fn hres(e: &WMIError) -> Option<i32> {
    match e {
        WMIError::HResultError { hres } => Some(*hres),
        _ => None,
    }
}

fn wmi_error(part: &str, e: WMIError) -> InventoryError {
    match hres(&e) {
        Some(WBEM_E_ACCESS_DENIED | E_ACCESSDENIED) => InventoryError::admin(part),
        _ => InventoryError::unreadable(part, format!("erreur WMI ({e})")),
    }
}

/// Classe ou espace de noms absent : le matériel ou le composant n'existe pas sur cette machine.
fn is_missing(e: &WMIError) -> bool {
    matches!(
        hres(e),
        Some(
            WBEM_E_NOT_FOUND
                | WBEM_E_INVALID_CLASS
                | WBEM_E_NOT_SUPPORTED
                | WBEM_E_INVALID_NAMESPACE
        )
    )
}

// ---------- Système et matériel ----------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawOs {
    caption: Option<String>,
    version: Option<String>,
    build_number: Option<String>,
}

fn read_os(cim: &WMIConnection) -> Result<OsInfo, InventoryError> {
    let part = "Système";
    let os: Vec<RawOs> = query(
        cim,
        part,
        "SELECT Caption, Version, BuildNumber FROM Win32_OperatingSystem",
    )?;
    let os = os
        .into_iter()
        .next()
        .ok_or_else(|| InventoryError::unreadable(part, "aucun résultat WMI"))?;
    let current = hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok();
    let display_version = current
        .as_ref()
        .and_then(|k| reg_string(k, "DisplayVersion"));
    let ubr: Option<u32> = current.as_ref().and_then(|k| k.get_value("UBR").ok());
    let build = os
        .build_number
        .as_deref()
        .and_then(parse::clean_smbios)
        .map(|b| match ubr {
            Some(r) => format!("{b}.{r}"),
            None => b,
        });
    Ok(OsInfo {
        name: clean_opt(os.caption.as_deref()),
        version: clean_opt(os.version.as_deref()),
        build,
        display_version,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawBios {
    manufacturer: Option<String>,
    #[serde(rename = "SMBIOSBIOSVersion")]
    smbios_bios_version: Option<String>,
    release_date: Option<String>,
    serial_number: Option<String>,
}

/// BIOS et numéro de série de la machine (porté par la table SMBIOS du BIOS).
fn read_bios(cim: &WMIConnection) -> Result<(BiosInfo, Option<String>), InventoryError> {
    let rows: Vec<RawBios> = query(
        cim,
        "BIOS",
        "SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate, SerialNumber FROM Win32_BIOS",
    )?;
    let b = rows
        .into_iter()
        .next()
        .ok_or_else(|| InventoryError::unreadable("BIOS", "aucun résultat WMI"))?;
    Ok((
        BiosInfo {
            vendor: clean_opt(b.manufacturer.as_deref()),
            version: clean_opt(b.smbios_bios_version.as_deref()),
            release_date: b.release_date.as_deref().and_then(parse::cim_date),
        },
        clean_opt(b.serial_number.as_deref()),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawComputerSystem {
    manufacturer: Option<String>,
    model: Option<String>,
    total_physical_memory: Option<u64>,
}

fn read_computer_system(
    cim: &WMIConnection,
) -> Result<(ComputerInfo, Option<u64>), InventoryError> {
    let rows: Vec<RawComputerSystem> = query(
        cim,
        "Ordinateur",
        "SELECT Manufacturer, Model, TotalPhysicalMemory FROM Win32_ComputerSystem",
    )?;
    let c = rows
        .into_iter()
        .next()
        .ok_or_else(|| InventoryError::unreadable("Ordinateur", "aucun résultat WMI"))?;
    Ok((
        ComputerInfo {
            manufacturer: clean_opt(c.manufacturer.as_deref()),
            model: clean_opt(c.model.as_deref()),
            serial_number: None,
        },
        c.total_physical_memory,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawBaseBoard {
    manufacturer: Option<String>,
    product: Option<String>,
}

fn read_board(cim: &WMIConnection) -> Result<BoardInfo, InventoryError> {
    let rows: Vec<RawBaseBoard> = query(
        cim,
        "Carte mère",
        "SELECT Manufacturer, Product FROM Win32_BaseBoard",
    )?;
    let b = rows
        .into_iter()
        .next()
        .ok_or_else(|| InventoryError::unreadable("Carte mère", "aucun résultat WMI"))?;
    Ok(BoardInfo {
        manufacturer: clean_opt(b.manufacturer.as_deref()),
        product: clean_opt(b.product.as_deref()),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawProcessor {
    name: Option<String>,
    number_of_cores: Option<u32>,
    number_of_logical_processors: Option<u32>,
    max_clock_speed: Option<u32>,
}

/// Une ligne par socket : cœurs et fils additionnés, nom et fréquence du premier.
/// `MaxClockSpeed` est en pratique la fréquence de base, pas le turbo.
fn read_cpu(cim: &WMIConnection) -> Result<CpuInfo, InventoryError> {
    let rows: Vec<RawProcessor> = query(
        cim,
        "Processeur",
        "SELECT Name, NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed FROM Win32_Processor",
    )?;
    let first = rows
        .first()
        .ok_or_else(|| InventoryError::unreadable("Processeur", "aucun résultat WMI"))?;
    let sum = |f: fn(&RawProcessor) -> Option<u32>| {
        rows.iter()
            .map(f)
            .try_fold(0u32, |acc, v| acc.checked_add(v?))
    };
    Ok(CpuInfo {
        name: clean_opt(first.name.as_deref()),
        cores: sum(|r| r.number_of_cores),
        threads: sum(|r| r.number_of_logical_processors),
        base_mhz: first.max_clock_speed.filter(|m| *m > 0),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawPhysicalMemory {
    capacity: Option<u64>,
    speed: Option<u32>,
    configured_clock_speed: Option<u32>,
    manufacturer: Option<String>,
    part_number: Option<String>,
    device_locator: Option<String>,
    bank_label: Option<String>,
    #[serde(rename = "SMBIOSMemoryType")]
    smbios_memory_type: Option<u32>,
}

fn read_memory_modules(cim: &WMIConnection) -> Result<Vec<MemoryModule>, InventoryError> {
    let rows: Vec<RawPhysicalMemory> = query(
        cim,
        "Barrettes mémoire",
        "SELECT Capacity, Speed, ConfiguredClockSpeed, Manufacturer, PartNumber, DeviceLocator, \
         BankLabel, SMBIOSMemoryType FROM Win32_PhysicalMemory",
    )?;
    Ok(rows
        .into_iter()
        .map(|m| MemoryModule {
            capacity_bytes: m.capacity.filter(|c| *c > 0),
            speed_mhz: m.configured_clock_speed.or(m.speed).filter(|s| *s > 0),
            manufacturer: clean_opt(m.manufacturer.as_deref()),
            part_number: clean_opt(m.part_number.as_deref()),
            slot: clean_opt(m.device_locator.as_deref())
                .or_else(|| clean_opt(m.bank_label.as_deref())),
            memory_type: m
                .smbios_memory_type
                .and_then(parse::smbios_memory_type)
                .map(String::from),
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawVideoController {
    name: Option<String>,
    driver_version: Option<String>,
    #[serde(rename = "AdapterRAM")]
    adapter_ram: Option<u64>,
}

/// `AdapterRAM` est un entier 32 bits : plafonné à 4 Gio. La vraie taille est lue dans le
/// registre du pilote (`HardwareInformation.qwMemorySize`) quand elle y est.
fn read_gpus(cim: &WMIConnection) -> Result<Vec<GpuInfo>, InventoryError> {
    let rows: Vec<RawVideoController> = query(
        cim,
        "Cartes graphiques",
        "SELECT Name, DriverVersion, AdapterRAM FROM Win32_VideoController",
    )?;
    let registry = gpu_memory_from_registry();
    Ok(rows
        .into_iter()
        .filter_map(|g| {
            let name = clean_opt(g.name.as_deref())?;
            let from_registry = registry
                .iter()
                .find(|(desc, _)| *desc == name)
                .map(|(_, m)| *m);
            Some(GpuInfo {
                memory_bytes: from_registry.or(g.adapter_ram).filter(|m| *m > 0),
                driver_version: clean_opt(g.driver_version.as_deref()),
                name,
            })
        })
        .collect())
}

/// Couples (nom du pilote, mémoire dédiée) des cartes graphiques. Silencieux en cas d'échec :
/// la valeur WMI sert alors de repli.
fn gpu_memory_from_registry() -> Vec<(String, u64)> {
    let Ok(class) = hklm(DISPLAY_CLASS_KEY) else {
        return Vec::new();
    };
    class
        .enum_keys()
        .filter_map(Result::ok)
        .filter_map(|sub| {
            let key = class.open_subkey(&sub).ok()?;
            let desc = reg_string(&key, "DriverDesc")?;
            let raw = key
                .get_raw_value("HardwareInformation.qwMemorySize")
                .or_else(|_| key.get_raw_value("HardwareInformation.MemorySize"))
                .ok()?;
            Some((desc, parse::le_bytes_to_u64(&raw.bytes)?))
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawNetAdapter {
    interface_description: Option<String>,
    ndis_physical_medium: Option<u32>,
}

/// Cartes physiques seulement (`HardwareInterface`). L'adresse MAC n'est pas demandée.
fn read_network() -> Result<Vec<NetworkAdapter>, InventoryError> {
    let part = "Cartes réseau";
    let con = WMIConnection::with_namespace_path("ROOT\\StandardCimv2")
        .map_err(|e| wmi_error(part, e))?;
    let rows: Vec<RawNetAdapter> = query(
        &con,
        part,
        "SELECT InterfaceDescription, NdisPhysicalMedium FROM MSFT_NetAdapter WHERE HardwareInterface = TRUE",
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|a| {
            let name = clean_opt(a.interface_description.as_deref())?;
            Some(NetworkAdapter {
                kind: parse::network_kind_from_ndis(a.ndis_physical_medium, &name),
                name,
            })
        })
        .collect())
}

// ---------- Batterie et températures (ROOT\WMI) ----------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawBatteryStatic {
    instance_name: Option<String>,
    designed_capacity: Option<u32>,
    capabilities: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawBatteryFull {
    instance_name: Option<String>,
    full_charged_capacity: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawBatteryCycles {
    instance_name: Option<String>,
    cycle_count: Option<u32>,
}

/// `Ok(None)` : pas de batterie (les classes n'existent pas sur un poste fixe).
fn read_battery(
    root_wmi: &WMIConnection,
) -> Result<Option<crate::model::BatteryInfo>, InventoryError> {
    let part = "Batterie";
    let statics: Vec<RawBatteryStatic> = match root_wmi
        .raw_query("SELECT InstanceName, DesignedCapacity, Capabilities FROM BatteryStaticData")
    {
        Ok(rows) => rows,
        Err(e) if is_missing(&e) => return Ok(None),
        Err(e) => return Err(wmi_error(part, e)),
    };
    let fulls: Vec<RawBatteryFull> = root_wmi
        .raw_query("SELECT InstanceName, FullChargedCapacity FROM BatteryFullChargedCapacity")
        .unwrap_or_default();
    // Compteur de cycles absent sur bien des batteries : pas une erreur.
    let cycles: Vec<RawBatteryCycles> = root_wmi
        .raw_query("SELECT InstanceName, CycleCount FROM BatteryCycleCount")
        .unwrap_or_default();

    let readings: Vec<BatteryReading> = statics
        .iter()
        .map(|s| {
            let full = fulls
                .iter()
                .find(|f| f.instance_name == s.instance_name)
                .and_then(|f| f.full_charged_capacity);
            let cycle_count = cycles
                .iter()
                .find(|c| c.instance_name == s.instance_name)
                .and_then(|c| c.cycle_count)
                .filter(|c| *c > 0);
            let relative = s
                .capabilities
                .is_some_and(|c| c & BATTERY_CAPACITY_RELATIVE != 0);
            let mwh = |v: Option<u32>| v.filter(|_| !relative).filter(|v| *v > 0).map(u64::from);
            BatteryReading {
                design_mwh: mwh(s.designed_capacity),
                full_mwh: mwh(full),
                health_pct: match (full, s.designed_capacity) {
                    (Some(f), Some(d)) => parse::health_pct(u64::from(f), u64::from(d)),
                    _ => None,
                },
                cycle_count,
            }
        })
        .collect();
    Ok(parse::merge_batteries(&readings))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawThermalZone {
    instance_name: Option<String>,
    current_temperature: Option<u32>,
}

/// Zones thermiques ACPI (souvent absentes, ou figées à une valeur fictive par le BIOS).
/// La température réelle du processeur exige un pilote noyau, hors de portée ici.
fn read_thermal_zones(root_wmi: &WMIConnection) -> Result<Vec<Sensor>, InventoryError> {
    let part = "Températures";
    let rows: Vec<RawThermalZone> = match root_wmi
        .raw_query("SELECT InstanceName, CurrentTemperature FROM MSAcpi_ThermalZoneTemperature")
    {
        Ok(rows) => rows,
        Err(e) if is_missing(&e) => return Ok(Vec::new()),
        Err(e) => return Err(wmi_error(part, e)),
    };
    Ok(rows
        .into_iter()
        .filter_map(|z| {
            let celsius = parse::decikelvin_to_celsius(z.current_temperature?);
            parse::plausible_celsius(celsius).then(|| Sensor {
                label: parse::thermal_zone_label(z.instance_name.as_deref().unwrap_or_default()),
                celsius,
            })
        })
        .collect())
}

fn no_thermal_zone() -> InventoryError {
    InventoryError::unreadable(
        "Températures",
        "aucune zone thermique ACPI lisible ; la température du processeur exige un pilote noyau (non inclus)",
    )
}

// ---------- Sécurité ----------

fn read_secure_boot() -> Result<SecureBootState, InventoryError> {
    let part = "Secure Boot";
    let key = match hklm(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State") {
        Ok(k) => k,
        // Clé absente : micrologiciel BIOS classique.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(SecureBootState::Unsupported),
        Err(e) => return Err(io_error(part, e)),
    };
    match key.get_value::<u32, _>("UEFISecureBootEnabled") {
        Ok(1) => Ok(SecureBootState::Enabled),
        Ok(_) => Ok(SecureBootState::Disabled),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(SecureBootState::Unsupported),
        Err(e) => Err(io_error(part, e)),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawTpm {
    spec_version: Option<String>,
    manufacturer_id_txt: Option<String>,
}

/// Exige les droits administrateur. Aucune instance : pas de TPM (ou désactivé dans le BIOS).
fn read_tpm() -> Result<TpmInfo, InventoryError> {
    let part = "TPM";
    let rows: Vec<RawTpm> =
        match WMIConnection::with_namespace_path("ROOT\\CIMV2\\Security\\MicrosoftTpm")
            .and_then(|con| con.raw_query("SELECT SpecVersion, ManufacturerIdTxt FROM Win32_Tpm"))
        {
            Ok(rows) => rows,
            Err(e) if is_missing(&e) => Vec::new(),
            Err(e) => return Err(wmi_error(part, e)),
        };
    Ok(match rows.into_iter().next() {
        Some(t) => TpmInfo {
            present: true,
            version: t.spec_version.as_deref().and_then(parse::tpm_spec_version),
            manufacturer: clean_opt(t.manufacturer_id_txt.as_deref()),
        },
        None => TpmInfo::default(),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawEncryptableVolume {
    drive_letter: Option<String>,
    protection_status: Option<u32>,
}

/// Exige les droits administrateur et une connexion chiffrée (`PktPrivacy`).
fn read_bitlocker() -> Result<Vec<BitLockerVolume>, InventoryError> {
    let part = "BitLocker";
    let con =
        WMIConnection::with_namespace_path("ROOT\\CIMV2\\Security\\MicrosoftVolumeEncryption")
            .map_err(|e| match hres(&e) {
                Some(WBEM_E_INVALID_NAMESPACE) => {
                    InventoryError::unreadable(part, "non disponible sur cette édition de Windows")
                }
                _ => wmi_error(part, e),
            })?;
    con.set_proxy_blanket(AuthLevel::PktPrivacy)
        .map_err(|e| wmi_error(part, e))?;
    let rows: Vec<RawEncryptableVolume> = query(
        &con,
        part,
        "SELECT DriveLetter, ProtectionStatus FROM Win32_EncryptableVolume",
    )?;
    Ok(rows
        .into_iter()
        .map(|v| BitLockerVolume {
            drive: clean_opt(v.drive_letter.as_deref()),
            protection: match v.protection_status {
                Some(0) => BitLockerProtection::Off,
                Some(1) => BitLockerProtection::On,
                _ => BitLockerProtection::Unknown,
            },
        })
        .collect())
}

/// Inscription MDM Intune : une sous-clé de `Enrollments` dont le fournisseur est « MS DM Server ».
fn read_intune() -> Result<bool, InventoryError> {
    let enrollments = match hklm(r"SOFTWARE\Microsoft\Enrollments") {
        Ok(k) => k,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_error("Intune", e)),
    };
    Ok(enrollments.enum_keys().filter_map(Result::ok).any(|sub| {
        enrollments
            .open_subkey(&sub)
            .ok()
            .and_then(|k| reg_string(&k, "ProviderID"))
            .is_some_and(|p| parse::is_intune_provider(&p))
    }))
}

fn read_autopilot() -> Result<AutopilotInfo, InventoryError> {
    match hklm(r"SOFTWARE\Microsoft\Provisioning\Diagnostics\Autopilot") {
        Ok(k) => Ok(parse::autopilot_from_values(
            reg_string(&k, "CloudAssignedTenantDomain").as_deref(),
            reg_string(&k, "CloudAssignedTenantId").as_deref(),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(AutopilotInfo::default()),
        Err(e) => Err(io_error("Autopilot", e)),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawLicenseProduct {
    description: Option<String>,
    license_status: Option<u32>,
}

/// Produits de licence de Windows ayant une clé installée. Requête lente (plusieurs secondes).
fn read_license() -> Result<LicenseInfo, InventoryError> {
    let part = "Licence Windows";
    let con = WMIConnection::new().map_err(|e| wmi_error(part, e))?;
    let rows: Vec<RawLicenseProduct> = query(
        &con,
        part,
        &format!(
            "SELECT Description, LicenseStatus FROM SoftwareLicensingProduct \
             WHERE ApplicationID = '{WINDOWS_APP_ID}' AND PartialProductKey IS NOT NULL"
        ),
    )?;
    let products: Vec<(Option<u32>, Option<String>)> = rows
        .into_iter()
        .map(|r| (r.license_status, r.description))
        .collect();
    Ok(parse::license_from_products(&products))
}

fn read_device_join() -> Result<DeviceJoin, InventoryError> {
    let part = "Azure AD (dsregcmd)";
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let path = root.join("System32").join("dsregcmd.exe");
    let out = process::run(&path, &["/status"], Duration::from_secs(30))
        .map_err(|e| InventoryError::process(part, e))?;
    if !out.stdout.contains("AzureAdJoined") {
        return Err(InventoryError::unreadable(part, "sortie inattendue"));
    }
    Ok(parse::parse_dsregcmd(&out.stdout))
}

// ---------- Registre : outils ----------

/// Ouvre une clé de HKLM en lecture, vue 64 bits.
fn hklm(path: &str) -> io::Result<RegKey> {
    HKLM.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
}

fn reg_string(key: &RegKey, name: &str) -> Option<String> {
    key.get_value::<String, _>(name)
        .ok()
        .and_then(|s| parse::clean_smbios(&s))
}

fn io_error(part: &str, e: io::Error) -> InventoryError {
    if e.kind() == io::ErrorKind::PermissionDenied {
        InventoryError::admin(part)
    } else {
        InventoryError::unreadable(part, e)
    }
}
