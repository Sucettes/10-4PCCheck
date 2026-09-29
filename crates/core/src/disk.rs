//! Types des disques et conversion depuis la sortie JSON de `smartctl`.
//!
//! La structure brute (`Raw*`) suit le format JSON de smartctl (`-j`). Tous les champs
//! y sont optionnels : smartctl n'émet que ce que le disque et le pont USB exposent.

use serde::{Deserialize, Serialize};

use crate::smartctl::SmartctlError;

/// Seule version majeure du format JSON de smartctl prise en charge.
pub const SUPPORTED_JSON_MAJOR: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanDevice {
    /// Chemin passé à smartctl (`/dev/sda`, `/dev/nvme0`, `/dev/pd0`).
    pub name: String,
    pub info_name: String,
    /// Type smartctl (`sat`, `nvme`, `sntrealtek`, ...), à repasser avec `-d`.
    pub dev_type: String,
    pub protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Ata,
    Nvme,
    Scsi,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MediaKind {
    Ssd,
    Hdd { rpm: u32 },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtaAttribute {
    pub id: u8,
    pub name: String,
    pub value: u16,
    pub worst: u16,
    pub threshold: u16,
    pub raw_value: u64,
    pub raw_string: String,
    pub prefailure: bool,
    /// Rempli par smartctl quand l'attribut est ou a été sous son seuil.
    pub when_failed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NvmeHealth {
    pub critical_warning: Option<u8>,
    pub available_spare: Option<u8>,
    pub available_spare_threshold: Option<u8>,
    pub percentage_used: Option<u8>,
    /// Unités de 512 000 octets, comme dans la norme NVMe.
    pub data_units_written: Option<u64>,
    pub data_units_read: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub media_errors: Option<u64>,
    pub error_log_entries: Option<u64>,
}

impl NvmeHealth {
    /// Octets écrits, calculés depuis les unités de données NVMe.
    pub fn bytes_written(&self) -> Option<u64> {
        self.data_units_written.and_then(|u| u.checked_mul(512_000))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiskInfo {
    pub device: ScanDevice,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub protocol: Protocol,
    pub media: MediaKind,
    pub smart_available: Option<bool>,
    pub smart_enabled: Option<bool>,
    /// Résultat global SMART du disque. `None` si le disque ne l'expose pas.
    pub smart_passed: Option<bool>,
    pub temperature_c: Option<i64>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub ata_attributes: Vec<AtaAttribute>,
    pub nvme_health: Option<NvmeHealth>,
    /// Code de sortie brut de smartctl (masque de bits).
    pub exit_status: u8,
    /// Avertissements lisibles tirés du code de sortie et des messages de smartctl.
    pub warnings: Vec<String>,
}

/// Analyse la sortie de `smartctl --scan-open -j`.
pub fn parse_scan(json: &str) -> Result<Vec<ScanDevice>, SmartctlError> {
    Ok(RawOutput::parse(json)?.into_scan())
}

/// Analyse la sortie de `smartctl -a -j`. `fallback` sert si le JSON ne décrit pas le périphérique.
/// Les bits 0 et 1 du code de sortie (échec de commande ou d'ouverture) deviennent une erreur.
pub fn parse_disk(json: &str, fallback: &ScanDevice) -> Result<DiskInfo, SmartctlError> {
    let raw = RawOutput::parse(json)?;
    let status = raw.exit_status();
    if status & FATAL_EXIT_BITS != 0 {
        return Err(SmartctlError::CommandFailed {
            exit_status: status,
            messages: raw.messages(),
        });
    }
    Ok(raw.into_disk(fallback))
}

// ---------- Structure brute du JSON smartctl ----------

#[derive(Debug, Deserialize)]
pub(crate) struct RawOutput {
    json_format_version: Option<Vec<u32>>,
    smartctl: Option<RawSmartctl>,
    device: Option<RawDevice>,
    devices: Option<Vec<RawDevice>>,
    model_name: Option<String>,
    serial_number: Option<String>,
    firmware_version: Option<String>,
    user_capacity: Option<RawCapacity>,
    nvme_total_capacity: Option<u64>,
    rotation_rate: Option<u32>,
    smart_support: Option<RawSmartSupport>,
    smart_status: Option<RawSmartStatus>,
    temperature: Option<RawTemperature>,
    power_on_time: Option<RawPowerOnTime>,
    power_cycle_count: Option<u64>,
    ata_smart_attributes: Option<RawAtaAttributes>,
    nvme_smart_health_information_log: Option<RawNvmeLog>,
}

#[derive(Debug, Deserialize)]
struct RawSmartctl {
    exit_status: Option<u8>,
    messages: Option<Vec<RawMessage>>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    string: String,
}

#[derive(Debug, Deserialize)]
struct RawDevice {
    name: String,
    info_name: Option<String>,
    #[serde(rename = "type")]
    dev_type: Option<String>,
    protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCapacity {
    bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawSmartSupport {
    available: Option<bool>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawSmartStatus {
    passed: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawTemperature {
    current: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawPowerOnTime {
    hours: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawAtaAttributes {
    table: Vec<RawAtaAttribute>,
}

#[derive(Debug, Deserialize)]
struct RawAtaAttribute {
    id: u8,
    name: String,
    value: u16,
    worst: u16,
    thresh: u16,
    when_failed: Option<String>,
    flags: Option<RawAtaFlags>,
    raw: RawAtaRaw,
}

#[derive(Debug, Deserialize)]
struct RawAtaFlags {
    prefailure: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawAtaRaw {
    value: u64,
    string: String,
}

#[derive(Debug, Deserialize)]
struct RawNvmeLog {
    critical_warning: Option<u8>,
    available_spare: Option<u8>,
    available_spare_threshold: Option<u8>,
    percentage_used: Option<u8>,
    data_units_read: Option<u64>,
    data_units_written: Option<u64>,
    unsafe_shutdowns: Option<u64>,
    media_errors: Option<u64>,
    num_err_log_entries: Option<u64>,
}

// ---------- Conversion ----------

impl RawOutput {
    pub(crate) fn parse(stdout: &str) -> Result<Self, SmartctlError> {
        let raw: RawOutput =
            serde_json::from_str(stdout).map_err(|e| SmartctlError::InvalidJson {
                reason: e.to_string(),
                excerpt: stdout.chars().take(300).collect(),
            })?;
        match raw.json_format_version.as_deref() {
            Some([major, ..]) if *major == SUPPORTED_JSON_MAJOR => Ok(raw),
            Some(v) => Err(SmartctlError::UnsupportedJsonVersion(v.to_vec())),
            None => Err(SmartctlError::InvalidJson {
                reason: "champ json_format_version absent".into(),
                excerpt: stdout.chars().take(300).collect(),
            }),
        }
    }

    pub(crate) fn exit_status(&self) -> u8 {
        self.smartctl
            .as_ref()
            .and_then(|s| s.exit_status)
            .unwrap_or(0)
    }

    pub(crate) fn messages(&self) -> Vec<String> {
        self.smartctl
            .as_ref()
            .and_then(|s| s.messages.as_ref())
            .map(|m| m.iter().map(|m| m.string.clone()).collect())
            .unwrap_or_default()
    }

    pub(crate) fn into_scan(self) -> Vec<ScanDevice> {
        self.devices
            .unwrap_or_default()
            .into_iter()
            .map(ScanDevice::from)
            .collect()
    }

    pub(crate) fn into_disk(self, fallback: &ScanDevice) -> DiskInfo {
        let exit_status = self.exit_status();
        let mut warnings = exit_status_warnings(exit_status);
        warnings.extend(self.messages());

        let device = self
            .device
            .map(ScanDevice::from)
            .unwrap_or_else(|| fallback.clone());
        let protocol = Protocol::from_smartctl(&device.protocol);
        let media = match (&protocol, self.rotation_rate) {
            (Protocol::Nvme, _) | (_, Some(0)) => MediaKind::Ssd,
            (_, Some(rpm)) => MediaKind::Hdd { rpm },
            (_, None) => MediaKind::Unknown,
        };

        DiskInfo {
            device,
            model: self.model_name,
            serial: self.serial_number,
            firmware: self.firmware_version,
            capacity_bytes: self
                .user_capacity
                .and_then(|c| c.bytes)
                .or(self.nvme_total_capacity),
            protocol,
            media,
            smart_available: self.smart_support.as_ref().and_then(|s| s.available),
            smart_enabled: self.smart_support.as_ref().and_then(|s| s.enabled),
            smart_passed: self.smart_status.and_then(|s| s.passed),
            temperature_c: self.temperature.and_then(|t| t.current),
            power_on_hours: self.power_on_time.and_then(|p| p.hours),
            power_cycles: self.power_cycle_count,
            ata_attributes: self
                .ata_smart_attributes
                .map(|a| a.table.into_iter().map(AtaAttribute::from).collect())
                .unwrap_or_default(),
            nvme_health: self.nvme_smart_health_information_log.map(NvmeHealth::from),
            exit_status,
            warnings,
        }
    }
}

impl From<RawDevice> for ScanDevice {
    fn from(d: RawDevice) -> Self {
        ScanDevice {
            info_name: d.info_name.unwrap_or_else(|| d.name.clone()),
            name: d.name,
            dev_type: d.dev_type.unwrap_or_default(),
            protocol: d.protocol.unwrap_or_default(),
        }
    }
}

impl From<RawAtaAttribute> for AtaAttribute {
    fn from(a: RawAtaAttribute) -> Self {
        AtaAttribute {
            id: a.id,
            name: a.name,
            value: a.value,
            worst: a.worst,
            threshold: a.thresh,
            raw_value: a.raw.value,
            raw_string: a.raw.string,
            prefailure: a.flags.and_then(|f| f.prefailure).unwrap_or(false),
            when_failed: a.when_failed.filter(|s| !s.is_empty()),
        }
    }
}

impl From<RawNvmeLog> for NvmeHealth {
    fn from(n: RawNvmeLog) -> Self {
        NvmeHealth {
            critical_warning: n.critical_warning,
            available_spare: n.available_spare,
            available_spare_threshold: n.available_spare_threshold,
            percentage_used: n.percentage_used,
            data_units_written: n.data_units_written,
            data_units_read: n.data_units_read,
            unsafe_shutdowns: n.unsafe_shutdowns,
            media_errors: n.media_errors,
            error_log_entries: n.num_err_log_entries,
        }
    }
}

impl Protocol {
    fn from_smartctl(s: &str) -> Self {
        match s {
            "ATA" => Protocol::Ata,
            "NVMe" => Protocol::Nvme,
            "SCSI" => Protocol::Scsi,
            other => Protocol::Other(other.to_string()),
        }
    }
}

/// Bits 0 et 1 : erreurs bloquantes (ligne de commande, ouverture du périphérique).
pub(crate) const FATAL_EXIT_BITS: u8 = 0b0000_0011;

/// Traduit les bits informatifs (2 à 7) du code de sortie de smartctl (voir `man smartctl`).
pub(crate) fn exit_status_warnings(status: u8) -> Vec<String> {
    const BITS: [(u8, &str); 6] = [
        (
            2,
            "Une commande SMART a échoué ou une somme de contrôle est invalide.",
        ),
        (3, "Le disque signale un état SMART « DISK FAILING »."),
        (
            4,
            "Un attribut critique (pré-défaillance) est sous son seuil.",
        ),
        (5, "Un attribut a déjà été sous son seuil par le passé."),
        (6, "Le journal d'erreurs du disque contient des erreurs."),
        (7, "Le journal des auto-tests contient des erreurs."),
    ];
    BITS.iter()
        .filter(|(bit, _)| status & (1 << bit) != 0)
        .map(|(_, msg)| (*msg).to_string())
        .collect()
}
