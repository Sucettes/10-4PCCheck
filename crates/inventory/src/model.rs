//! Types de l'inventaire, envoyés tels quels à l'interface (JSON).
//!
//! Règle générale : `Option` = donnée absente ou illisible. La raison d'une absence due à une
//! erreur (droits, outil manquant) est dans `MachineInventory::errors`, pas dans le champ.

use serde::Serialize;

/// Inventaire complet d'une machine. Chaque partie est indépendante : une partie illisible
/// laisse son champ vide et ajoute une ligne dans `errors`, sans bloquer les autres.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MachineInventory {
    pub os: Option<OsInfo>,
    pub computer: Option<ComputerInfo>,
    pub bios: Option<BiosInfo>,
    pub board: Option<BoardInfo>,
    pub cpu: Option<CpuInfo>,
    pub memory: Option<MemoryInfo>,
    pub gpus: Vec<GpuInfo>,
    pub network_adapters: Vec<NetworkAdapter>,
    /// `None` : pas de batterie (poste fixe) ou batterie illisible (voir `errors`).
    pub battery: Option<BatteryInfo>,
    pub security: Option<SecurityInfo>,
    /// Capteurs de température lisibles sans pilote. Souvent vide sous Windows.
    pub temperatures: Vec<Sensor>,
    /// Parties illisibles, en français (« BitLocker : droits administrateur requis »).
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OsInfo {
    /// « Microsoft Windows 11 Pro », « Ubuntu 24.04.1 LTS ».
    pub name: Option<String>,
    /// Windows : « 10.0.26100 ». Linux : version de la distribution (« 24.04 »).
    pub version: Option<String>,
    /// Windows : numéro de build avec révision (« 26100.4061 »). Linux : version du noyau.
    pub build: Option<String>,
    /// Windows seulement : version commerciale (« 24H2 »).
    pub display_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ComputerInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    /// Numéro de série du fabricant (utile pour vérifier la garantie ou un signalement de vol).
    /// Affiché en clair (outil personnel). Sous Linux, lisible seulement en root.
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BiosInfo {
    pub vendor: Option<String>,
    pub version: Option<String>,
    /// Date au format AAAA-MM-JJ.
    pub release_date: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BoardInfo {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CpuInfo {
    pub name: Option<String>,
    /// Cœurs physiques, tous processeurs confondus.
    pub cores: Option<u32>,
    /// Processeurs logiques (fils matériels).
    pub threads: Option<u32>,
    /// Fréquence de base annoncée, en MHz (pas la fréquence turbo).
    pub base_mhz: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MemoryInfo {
    /// Mémoire vue par le système d'exploitation (un peu moins que la somme des barrettes).
    pub total_bytes: Option<u64>,
    /// Vide si la table SMBIOS est illisible (Linux sans root).
    pub modules: Vec<MemoryModule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MemoryModule {
    pub capacity_bytes: Option<u64>,
    /// Vitesse configurée si connue, sinon vitesse maximale de la barrette (MT/s, notée MHz
    /// par Windows et dmidecode).
    pub speed_mhz: Option<u32>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    /// Emplacement sur la carte mère (« DIMM A1 »).
    pub slot: Option<String>,
    /// « DDR4 », « DDR5 », « LPDDR5 »...
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GpuInfo {
    pub name: String,
    /// Windows seulement.
    pub driver_version: Option<String>,
    /// Mémoire dédiée. Windows seulement, souvent absente pour les GPU intégrés.
    pub memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkKind {
    Wifi,
    Ethernet,
    Bluetooth,
    Other,
}

/// Carte réseau physique. L'adresse MAC n'est jamais lue : elle identifie la machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkAdapter {
    /// Nom du produit sous Windows, nom d'interface sous Linux (`wlp2s0`).
    pub name: String,
    pub kind: NetworkKind,
}

/// Batterie. S'il y en a plusieurs (certains portables), les capacités sont additionnées
/// et le nombre de cycles est le plus élevé.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BatteryInfo {
    pub count: u32,
    /// Capacité d'origine (conception), en mWh.
    pub design_capacity_mwh: Option<u64>,
    /// Capacité à pleine charge aujourd'hui, en mWh.
    pub full_charge_capacity_mwh: Option<u64>,
    pub cycle_count: Option<u32>,
    /// Pleine charge / origine, en %. Peut dépasser 100 sur une batterie neuve.
    pub health_pct: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecureBootState {
    Enabled,
    Disabled,
    /// Micrologiciel BIOS classique (pas UEFI) : Secure Boot n'existe pas.
    Unsupported,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TpmInfo {
    pub present: bool,
    /// Version de la norme : « 2.0 », « 1.2 ».
    pub version: Option<String>,
    /// Code du fabricant de la puce (« INTC », « AMD », « IFX »).
    pub manufacturer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LicenseInfo {
    /// Windows activé : licence de Windows avec clé installée et état « sous licence » (1).
    pub activated: bool,
    /// État brut de `SoftwareLicensingProduct.LicenseStatus` (0 à 6).
    pub status: Option<u32>,
    pub status_label: Option<String>,
    /// Description fournie par Windows (« Windows(R) Operating System, RETAIL channel »).
    pub description: Option<String>,
    /// Canal extrait de la description : « RETAIL », « OEM_DM », « VOLUME_KMSCLIENT »...
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BitLockerProtection {
    Off,
    On,
    /// Volume verrouillé ou état indéterminé (`ProtectionStatus` = 2).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BitLockerVolume {
    /// Lettre de lecteur (« C: »), ou `None` pour un volume sans lettre.
    pub drive: Option<String>,
    pub protection: BitLockerProtection,
}

/// Résultat utile de `dsregcmd /status` : jonction à un annuaire d'entreprise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DeviceJoin {
    pub azure_ad_joined: Option<bool>,
    pub enterprise_joined: Option<bool>,
    pub domain_joined: Option<bool>,
    /// Compte professionnel ajouté par un utilisateur (ne gère pas la machine elle-même).
    pub workplace_joined: Option<bool>,
    pub domain_name: Option<String>,
    pub tenant_name: Option<String>,
    /// URL du serveur MDM (Intune : `https://enrollment.manage.microsoft.com/...`).
    pub mdm_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AutopilotInfo {
    /// Machine attribuée à une entreprise dans Autopilot : au prochain effacement, Windows
    /// imposera la connexion au compte de cette entreprise.
    pub assigned: bool,
    pub tenant_domain: Option<String>,
}

/// État de sécurité et de gestion. Sous Linux, seuls `secure_boot` et `tpm` sont remplis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SecurityInfo {
    pub windows_license: Option<LicenseInfo>,
    /// `None` : illisible (droits administrateur requis).
    pub bitlocker: Option<Vec<BitLockerVolume>>,
    pub secure_boot: Option<SecureBootState>,
    pub tpm: Option<TpmInfo>,
    pub device_join: Option<DeviceJoin>,
    /// Inscription MDM Intune trouvée dans le registre.
    pub intune_enrolled: Option<bool>,
    pub autopilot: Option<AutopilotInfo>,
    /// Synthèse : la machine est rattachée à une organisation (annuaire, MDM ou Autopilot).
    /// Critère rouge du verdict (plan §4).
    pub enterprise_managed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sensor {
    pub label: String,
    pub celsius: f64,
}
