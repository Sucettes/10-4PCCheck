//! Auto-tests SMART : le disque vérifie lui-même son électronique (court) ou toute sa surface (long),
//! en lecture seule, pendant qu'il reste utilisable. On lance le test, puis on interroge son état.
//!
//! Formats JSON de smartctl lus ici :
//! - ATA : `ata_smart_data.self_test.status` (octet d'état, 0xF_ = en cours, bas = dizaines de %
//!   restantes), `ata_smart_data.self_test.polling_minutes`, `ata_smart_self_test_log.standard.table`.
//! - NVMe : `nvme_self_test_log.current_self_test_operation` (0 = aucun), `current_self_test_completion_percent`,
//!   `nvme_self_test_log.table` (résultat 0 = réussi, 15 = entrée inutilisée).

use serde::{Deserialize, Serialize};

use crate::disk::RawOutput;
use crate::smartctl::SmartctlError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfTestKind {
    Short,
    Long,
}

impl SelfTestKind {
    pub(crate) fn smartctl_arg(self) -> &'static str {
        match self {
            SelfTestKind::Short => "short",
            SelfTestKind::Long => "long",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelfTestResult {
    /// Type de test tel que rapporté par le disque (« Short offline », « Extended self-test »).
    pub kind: String,
    /// `None` si le test a été interrompu ou si l'état n'est pas un succès ni un échec clair.
    pub passed: Option<bool>,
    pub text: String,
    /// Heures d'utilisation du disque au moment du test.
    pub power_on_hours: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelfTestStatus {
    /// `None` si le disque ne dit pas s'il prend en charge les auto-tests.
    pub supported: Option<bool>,
    pub running: bool,
    /// Pourcentage restant du test en cours, par pas de 10 en ATA.
    pub remaining_pct: Option<u8>,
    /// Durées estimées par le fabricant (ATA seulement).
    pub short_minutes: Option<u32>,
    pub long_minutes: Option<u32>,
    /// Résultats les plus récents d'abord (au plus 5).
    pub history: Vec<SelfTestResult>,
}

// ---------- JSON brut ----------

#[derive(Debug, Deserialize)]
pub(crate) struct RawSelfTest {
    ata_smart_data: Option<RawAtaSmartData>,
    ata_smart_self_test_log: Option<RawAtaSelfTestLog>,
    nvme_self_test_log: Option<RawNvmeSelfTestLog>,
    nvme_optional_admin_commands: Option<RawNvmeAdminCommands>,
}

#[derive(Debug, Deserialize)]
struct RawAtaSmartData {
    self_test: Option<RawAtaSelfTest>,
    capabilities: Option<RawAtaCapabilities>,
}

#[derive(Debug, Deserialize)]
struct RawAtaSelfTest {
    status: Option<RawAtaSelfTestStatus>,
    polling_minutes: Option<RawPollingMinutes>,
}

#[derive(Debug, Deserialize)]
struct RawAtaSelfTestStatus {
    value: u8,
    remaining_percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct RawPollingMinutes {
    short: Option<u32>,
    extended: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawAtaCapabilities {
    self_tests_supported: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawAtaSelfTestLog {
    standard: Option<RawAtaSelfTestTable>,
}

#[derive(Debug, Deserialize)]
struct RawAtaSelfTestTable {
    table: Option<Vec<RawAtaSelfTestEntry>>,
}

#[derive(Debug, Deserialize)]
struct RawAtaSelfTestEntry {
    #[serde(rename = "type")]
    kind: RawLabel,
    status: RawAtaEntryStatus,
    lifetime_hours: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawAtaEntryStatus {
    string: String,
    passed: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawLabel {
    value: Option<u8>,
    string: String,
}

#[derive(Debug, Deserialize)]
struct RawNvmeSelfTestLog {
    current_self_test_operation: Option<RawLabel>,
    current_self_test_completion_percent: Option<u8>,
    table: Option<Vec<RawNvmeSelfTestEntry>>,
}

#[derive(Debug, Deserialize)]
struct RawNvmeSelfTestEntry {
    self_test_code: RawLabel,
    self_test_result: RawLabel,
    power_on_hours: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawNvmeAdminCommands {
    self_test: Option<bool>,
}

const HISTORY_LEN: usize = 5;
/// Résultat NVMe 0 : terminé sans erreur. 1 et 2 : interrompu (commande, réinitialisation).
/// 15 : entrée inutilisée. Le reste : échec.
const NVME_RESULT_OK: u8 = 0;
const NVME_RESULT_ABORTED: [u8; 3] = [1, 2, 3];
const NVME_RESULT_UNUSED: u8 = 15;

/// Analyse la sortie de `smartctl -c -l selftest -j`.
pub fn parse_self_test_status(json: &str) -> Result<SelfTestStatus, SmartctlError> {
    let outer = RawOutput::parse(json)?;
    outer.check_fatal()?;
    let raw: RawSelfTest = serde_json::from_str(json).map_err(|e| SmartctlError::InvalidJson {
        reason: e.to_string(),
        excerpt: json.chars().take(300).collect(),
    })?;
    Ok(match raw.nvme_self_test_log {
        Some(log) => nvme_status(log, raw.nvme_optional_admin_commands),
        None => ata_status(raw.ata_smart_data, raw.ata_smart_self_test_log),
    })
}

fn ata_status(data: Option<RawAtaSmartData>, log: Option<RawAtaSelfTestLog>) -> SelfTestStatus {
    let (self_test, supported) = match data {
        Some(d) => (
            d.self_test,
            d.capabilities.and_then(|c| c.self_tests_supported),
        ),
        None => (None, None),
    };
    let status = self_test.as_ref().and_then(|s| s.status.as_ref());
    // Octet d'état ATA : 4 bits hauts = état (15 = en cours), 4 bits bas = dizaines de % restantes.
    let running = status.is_some_and(|s| s.value >> 4 == 0xF);
    let remaining_pct = status
        .filter(|_| running)
        .map(|s| s.remaining_percent.unwrap_or((s.value & 0x0F) * 10));
    let polling = self_test.and_then(|s| s.polling_minutes);
    let history = log
        .and_then(|l| l.standard)
        .and_then(|s| s.table)
        .unwrap_or_default()
        .into_iter()
        .take(HISTORY_LEN)
        .map(|e| SelfTestResult {
            kind: e.kind.string,
            passed: e.status.passed,
            text: e.status.string,
            power_on_hours: e.lifetime_hours,
        })
        .collect();
    SelfTestStatus {
        supported,
        running,
        remaining_pct,
        short_minutes: polling.as_ref().and_then(|p| p.short),
        long_minutes: polling.and_then(|p| p.extended),
        history,
    }
}

fn nvme_status(log: RawNvmeSelfTestLog, admin: Option<RawNvmeAdminCommands>) -> SelfTestStatus {
    let running = log
        .current_self_test_operation
        .as_ref()
        .and_then(|o| o.value)
        .is_some_and(|v| v != 0);
    let remaining_pct = log
        .current_self_test_completion_percent
        .filter(|_| running)
        .map(|done| 100u8.saturating_sub(done));
    let history = log
        .table
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.self_test_result.value != Some(NVME_RESULT_UNUSED))
        .take(HISTORY_LEN)
        .map(|e| {
            let passed = match e.self_test_result.value {
                Some(NVME_RESULT_OK) => Some(true),
                Some(v) if NVME_RESULT_ABORTED.contains(&v) => None,
                Some(_) => Some(false),
                None => None,
            };
            SelfTestResult {
                kind: e.self_test_code.string,
                passed,
                text: e.self_test_result.string,
                power_on_hours: e.power_on_hours,
            }
        })
        .collect();
    SelfTestStatus {
        supported: admin.and_then(|a| a.self_test),
        running,
        remaining_pct,
        short_minutes: None,
        long_minutes: None,
        history,
    }
}
