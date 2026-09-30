//! Types des disques et conversion depuis la sortie JSON de `smartctl`.
//!
//! La structure brute (`Raw*`) suit le format JSON de smartctl (`-j`). Tous les champs
//! y sont optionnels : smartctl n'émet que ce que le disque et le pont USB exposent.

use serde::{Deserialize, Serialize};

use crate::attributes::{self, AttributeStatus};
use crate::checks::{self, Check};
use crate::smartctl::SmartctlError;

/// Seule version majeure du format JSON de smartctl prise en charge.
pub const SUPPORTED_JSON_MAJOR: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Libellé français, `None` pour un attribut que la table ne connaît pas.
    pub label_fr: Option<&'static str>,
    pub status: AttributeStatus,
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

    pub fn bytes_read(&self) -> Option<u64> {
        self.data_units_read.and_then(|u| u.checked_mul(512_000))
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
    /// Norme de commandes : « ACS-4 T13/BSR INCITS 529 revision 5 », « NVMe 1.4 ».
    pub standard: Option<String>,
    /// Génération SATA déclarée par le disque (« SATA 3.2 »).
    pub sata_version: Option<String>,
    /// Vitesse actuelle du lien SATA (« 6.0 Gb/s »). Plus basse que prévu : câble, port ou pont USB.
    pub link_speed: Option<String>,
    pub form_factor: Option<String>,
    pub trim_supported: Option<bool>,
    pub bytes_written: Option<u64>,
    pub bytes_read: Option<u64>,
    pub smart_available: Option<bool>,
    pub smart_enabled: Option<bool>,
    /// Résultat global SMART du disque. `None` si le disque ne l'expose pas.
    pub smart_passed: Option<bool>,
    pub temperature_c: Option<i64>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub ata_attributes: Vec<AtaAttribute>,
    pub nvme_health: Option<NvmeHealth>,
    /// Vie restante estimée d'un SSD, de 0 à 100. `None` si le disque ne l'expose pas (disques durs).
    pub life_remaining_pct: Option<u8>,
    /// Code de sortie brut de smartctl (masque de bits).
    pub exit_status: u8,
    /// Avertissements lisibles tirés du code de sortie et des messages de smartctl.
    pub warnings: Vec<String>,
    /// Vérifications de cohérence entre compteurs (voir `checks`).
    pub checks: Vec<Check>,
}

/// Un disque listé par le scan, avec ses données ou l'erreur de lecture. Un disque illisible
/// (pont USB sans SMART, par exemple) n'empêche pas la lecture des autres.
#[derive(Debug, Clone, Serialize)]
pub struct DiskEntry {
    pub device: ScanDevice,
    pub info: Option<DiskInfo>,
    pub error: Option<SmartctlError>,
}

/// Retire les doublons d'un même disque physique. Sous Windows, avec le pilote Intel RST,
/// smartctl voit un disque deux fois : `/dev/sdX` et `/dev/csmiN,P` (interface CSMI du pilote).
/// Identité : modèle + numéro de série. On garde le chemin standard plutôt que CSMI, et l'ordre
/// du scan. Un disque sans numéro de série ou illisible est toujours gardé : rien ne prouve un doublon.
pub fn dedupe_disks(entries: Vec<DiskEntry>) -> Vec<DiskEntry> {
    let mut kept: Vec<DiskEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        let duplicate_of =
            identity(&entry).and_then(|id| kept.iter().position(|k| identity(k) == Some(id)));
        match duplicate_of {
            Some(i) if is_csmi(&kept[i]) && !is_csmi(&entry) => kept[i] = entry,
            Some(_) => {}
            None => kept.push(entry),
        }
    }
    kept
}

fn identity(entry: &DiskEntry) -> Option<(&str, &str)> {
    let info = entry.info.as_ref()?;
    let serial = info.serial.as_deref().filter(|s| !s.trim().is_empty())?;
    Some((info.model.as_deref().unwrap_or_default(), serial))
}

fn is_csmi(entry: &DiskEntry) -> bool {
    entry.device.name.starts_with("/dev/csmi")
}

/// Analyse la sortie de `smartctl --scan-open -j`.
pub fn parse_scan(json: &str) -> Result<Vec<ScanDevice>, SmartctlError> {
    Ok(RawOutput::parse(json)?.into_scan())
}

/// Analyse la sortie de `smartctl -a -j`. `fallback` sert si le JSON ne décrit pas le périphérique.
/// Les bits 0 et 1 du code de sortie (échec de commande ou d'ouverture) deviennent une erreur.
pub fn parse_disk(json: &str, fallback: &ScanDevice) -> Result<DiskInfo, SmartctlError> {
    let raw = RawOutput::parse(json)?;
    raw.check_fatal()?;
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
    ata_version: Option<RawString>,
    nvme_version: Option<RawString>,
    sata_version: Option<RawString>,
    interface_speed: Option<RawInterfaceSpeed>,
    form_factor: Option<RawFormFactor>,
    trim: Option<RawTrim>,
    logical_block_size: Option<u64>,
}

// Champs imbriqués facultatifs : un seul champ absent ferait refuser tout le document par serde,
// et le disque entier deviendrait illisible pour un détail d'affichage.
#[derive(Debug, Deserialize)]
struct RawString {
    #[serde(default)]
    string: String,
}

#[derive(Debug, Deserialize)]
struct RawInterfaceSpeed {
    current: Option<RawString>,
}

#[derive(Debug, Deserialize)]
struct RawFormFactor {
    /// Absent quand smartctl ne connaît pas le code (seul `ata_value` est alors écrit).
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawTrim {
    supported: Option<bool>,
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

    /// Erreur si un des bits de `mask` est levé dans le code de sortie.
    pub(crate) fn check_exit(&self, mask: u8) -> Result<(), SmartctlError> {
        let status = self.exit_status();
        if status & mask != 0 {
            return Err(SmartctlError::CommandFailed {
                exit_status: status,
                messages: self.messages(),
            });
        }
        Ok(())
    }

    pub(crate) fn check_fatal(&self) -> Result<(), SmartctlError> {
        self.check_exit(FATAL_EXIT_BITS)
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
        let ata_attributes: Vec<AtaAttribute> = self
            .ata_smart_attributes
            .map(|a| a.table.into_iter().map(AtaAttribute::from).collect())
            .unwrap_or_default();
        let nvme_health = self.nvme_smart_health_information_log.map(NvmeHealth::from);
        let life_remaining_pct = life_remaining(&media, nvme_health.as_ref(), &ata_attributes);
        let block_size = self.logical_block_size.unwrap_or(512);
        let (bytes_written, bytes_read) = match &nvme_health {
            Some(h) => (h.bytes_written(), h.bytes_read()),
            None => (
                attributes::bytes_written(&ata_attributes, block_size),
                attributes::bytes_read(&ata_attributes, block_size),
            ),
        };
        let standard = match (self.nvme_version, self.ata_version) {
            (Some(v), _) => Some(format!("NVMe {}", v.string)),
            (None, Some(v)) => Some(v.string),
            (None, None) => None,
        };

        let mut info = DiskInfo {
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
            standard,
            sata_version: self
                .sata_version
                .map(|v| v.string)
                .filter(|s| !s.is_empty()),
            link_speed: self
                .interface_speed
                .and_then(|i| i.current)
                .map(|c| c.string)
                .filter(|s| !s.is_empty()),
            form_factor: self.form_factor.and_then(|f| f.name),
            trim_supported: self.trim.and_then(|t| t.supported),
            bytes_written,
            bytes_read,
            smart_available: self.smart_support.as_ref().and_then(|s| s.available),
            smart_enabled: self.smart_support.as_ref().and_then(|s| s.enabled),
            smart_passed: self.smart_status.and_then(|s| s.passed),
            // smartctl renvoie 0 quand le disque n'expose pas de température (disques virtuels,
            // certains SCSI). Un disque en marche à exactement 0 °C n'est pas plausible.
            temperature_c: self.temperature.and_then(|t| t.current).filter(|&c| c != 0),
            power_on_hours: self.power_on_time.and_then(|p| p.hours),
            power_cycles: self.power_cycle_count,
            ata_attributes,
            nvme_health,
            life_remaining_pct,
            exit_status,
            warnings,
            checks: Vec::new(),
        };
        info.checks = checks::run(&info);
        info
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
        let when_failed = a.when_failed.filter(|s| !s.is_empty());
        AtaAttribute {
            label_fr: attributes::label_fr(&a.name),
            status: attributes::status(a.id, when_failed.as_deref(), a.raw.value),
            id: a.id,
            name: a.name,
            value: a.value,
            worst: a.worst,
            threshold: a.thresh,
            raw_value: a.raw.value,
            raw_string: a.raw.string,
            prefailure: a.flags.and_then(|f| f.prefailure).unwrap_or(false),
            when_failed,
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

/// Attributs ATA dont la valeur normalisée est la vie restante en %, selon le fabricant :
/// 177 Samsung, 202 Crucial/Micron, 231 Kingston/SandForce, 233 Intel. Premier trouvé, dans cet ordre.
const ATA_LIFE_ATTRIBUTES: [u8; 4] = [177, 202, 231, 233];

/// Vie restante d'un SSD en %. NVMe : `100 - percentage_used` (la norme permet plus de 100 % d'usure,
/// d'où le plancher à 0). SATA : attribut propre au fabricant, lu seulement sur un SSD, car les mêmes
/// ID ont un autre sens sur un disque dur (202 = erreurs d'adresse chez WD, par exemple).
pub(crate) fn life_remaining(
    media: &MediaKind,
    nvme: Option<&NvmeHealth>,
    ata: &[AtaAttribute],
) -> Option<u8> {
    if let Some(used) = nvme.and_then(|h| h.percentage_used) {
        return Some(100u8.saturating_sub(used));
    }
    if *media != MediaKind::Ssd {
        return None;
    }
    ATA_LIFE_ATTRIBUTES
        .iter()
        .find_map(|id| ata.iter().find(|a| a.id == *id))
        .and_then(|a| u8::try_from(a.value).ok())
        .filter(|&v| v <= 100)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn nvme_used(pct: u8) -> NvmeHealth {
        NvmeHealth {
            critical_warning: None,
            available_spare: None,
            available_spare_threshold: None,
            percentage_used: Some(pct),
            data_units_written: None,
            data_units_read: None,
            unsafe_shutdowns: None,
            media_errors: None,
            error_log_entries: None,
        }
    }

    fn ata(id: u8, value: u16) -> AtaAttribute {
        AtaAttribute {
            id,
            name: String::new(),
            value,
            worst: value,
            threshold: 0,
            raw_value: 0,
            raw_string: String::new(),
            prefailure: false,
            when_failed: None,
            label_fr: None,
            status: AttributeStatus::Ok,
        }
    }

    #[test]
    fn nvme_wear_beyond_100_percent_floors_at_zero() {
        let h = nvme_used(143);
        assert_eq!(life_remaining(&MediaKind::Ssd, Some(&h), &[]), Some(0));
    }

    #[test]
    fn vendor_attribute_is_ignored_on_hdd() {
        // 202 sur un disque dur WD : erreurs d'adresse, pas une usure.
        let hdd = MediaKind::Hdd { rpm: 7200 };
        assert_eq!(life_remaining(&hdd, None, &[ata(202, 100)]), None);
        assert_eq!(
            life_remaining(&MediaKind::Ssd, None, &[ata(202, 88)]),
            Some(88)
        );
    }
}
