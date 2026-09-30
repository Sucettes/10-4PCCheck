//! Inventaire d'une machine (matériel, batterie, sécurité, gestion d'entreprise), test de
//! charge CPU et test RAM partiel.
//!
//! - `collect()` n'échoue jamais : chaque partie illisible laisse son champ vide et ajoute une
//!   ligne dans `MachineInventory::errors`. Lancé sans droits administrateur, l'inventaire est
//!   partiel (BitLocker, TPM, numéro de série sous Linux, barrettes via dmidecode).
//! - Toute l'analyse de texte est dans des fonctions pures (`parse`, `linux_parse`), testées
//!   sur des sorties inventées ; la collecte propre à chaque OS est dans `win` et `linux`.

pub mod error;
pub mod gpu;
pub mod linux_parse;
pub mod model;
pub mod parse;
pub mod ram;
pub mod stress;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(windows, target_os = "linux")))]
mod other;
#[cfg(windows)]
mod win;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(windows, target_os = "linux")))]
use other as platform;
#[cfg(windows)]
use win as platform;

pub use error::InventoryError;
pub use gpu::{analyse_gpu_test, gpu_sensors, GpuSample, GpuSensor, GpuTestInput, GpuTestResult};
pub use model::{
    AutopilotInfo, BatteryInfo, BiosInfo, BitLockerProtection, BitLockerVolume, BoardInfo,
    ComputerInfo, CpuInfo, DeviceJoin, GpuInfo, LicenseInfo, MachineInventory, MemoryInfo,
    MemoryModule, NetworkAdapter, NetworkKind, OsInfo, SecureBootState, SecurityInfo, Sensor,
    TpmInfo,
};
pub use ram::{
    available_memory_bytes, run_ram_test, RamPattern, RamPhase, RamProgress, RamTestResult,
};
pub use stress::{
    analyse_throttling, run_cpu_stress, StressResult, StressSample, ThrottleAnalysis, ThrottleLevel,
};

/// Collecte l'inventaire complet. Bloquant (quelques secondes sous Windows, la requête de
/// licence étant lente) : à appeler hors du fil de l'interface.
pub fn collect() -> MachineInventory {
    let mut inv = MachineInventory::default();
    platform::collect(&mut inv);
    if let Some(security) = inv.security.as_mut() {
        security.enterprise_managed = parse::is_enterprise_managed(security);
    }
    inv
}
