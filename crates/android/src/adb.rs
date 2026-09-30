//! Localisation et exécution du binaire `adb` embarqué sur la clé.
//!
//! Toutes les commandes passent par `pccheck_core::process::run` : délai maximal, pas de shell
//! côté PC (programme et arguments séparés). Côté téléphone, `adb shell` transmet la commande au
//! shell d'Android ; on n'y passe que des commandes fixes, jamais de texte venu de l'utilisateur.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pccheck_core::process::{self, ProcessError};
use serde::Serialize;
use thiserror::Error;

use crate::battery::SYSFS_BATTERY_DIR;
use crate::collect::{assemble, CollectIssue, CollectStep, PhoneReport, RawCollection};
use crate::devices::{mask_serial, parse_devices, AdbDevice, DeviceState};
use crate::owners::parse_dpm_list_owners;
use crate::props::parse_getprop;

/// Variable d'environnement qui force le chemin d'adb (tests, développement).
pub const ENV_OVERRIDE: &str = "PCCHECK_ADB";

/// Délai maximal d'une commande sur le téléphone.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// Délai des commandes qui peuvent démarrer le serveur adb (premier appel : quelques secondes,
/// davantage sur une machine lente ou avec un antivirus qui analyse l'exécutable).
pub const SERVER_TIMEOUT: Duration = Duration::from_secs(45);

#[cfg(windows)]
const BINARY_NAME: &str = "adb.exe";
#[cfg(not(windows))]
const BINARY_NAME: &str = "adb";

/// Longueur maximale acceptée pour un numéro de série.
const MAX_SERIAL_LEN: usize = 128;

#[derive(Debug, Clone, Error, Serialize, PartialEq, Eq)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum AdbError {
    #[error(
        "adb introuvable (cherché dans : {})",
        pccheck_core::process::display_paths(searched)
    )]
    NotFound { searched: Vec<PathBuf> },
    #[error("impossible de lancer {path:?} : {reason}")]
    Spawn { path: PathBuf, reason: String },
    #[error("adb n'a pas répondu en {seconds} s ({args})")]
    Timeout { seconds: u64, args: String },
    #[error("numéro de série invalide")]
    InvalidSerial,
    #[error("téléphone introuvable : il a peut-être été débranché")]
    DeviceNotFound,
    #[error("téléphone pas prêt (état « {state} ») : {guidance}")]
    DeviceNotReady {
        state: DeviceState,
        guidance: String,
    },
    #[error(
        "le téléphone n'a rien répondu à « {command} » : déverrouille-le et vérifie que le \
         débogage USB est toujours autorisé"
    )]
    NoResponse { command: String },
    #[error("sortie inattendue de « {command} » : {excerpt}")]
    UnexpectedOutput { command: String, excerpt: String },
}

impl From<ProcessError> for AdbError {
    fn from(e: ProcessError) -> Self {
        match e {
            ProcessError::NotFound { searched, .. } => AdbError::NotFound { searched },
            ProcessError::Spawn { path, reason } => AdbError::Spawn { path, reason },
            ProcessError::Timeout { seconds, args, .. } => AdbError::Timeout { seconds, args },
        }
    }
}

/// Version d'adb (`adb version`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdbVersion {
    /// Version du protocole, ex. « 1.0.41 ».
    pub protocol: String,
    /// Version des platform-tools, ex. « 35.0.2-12147458 ». Absente sur les très vieux adb.
    pub release: Option<String>,
}

/// Analyse `adb version`. La ligne « Installed as ... » est ignorée : elle contient un
/// chemin local sans intérêt pour le rapport.
pub fn parse_version(text: &str) -> Option<AdbVersion> {
    let mut protocol = None;
    let mut release = None;
    for line in text.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("Android Debug Bridge version ") {
            protocol = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Version ") {
            release = Some(v.trim().to_string());
        }
    }
    Some(AdbVersion {
        protocol: protocol?,
        release,
    })
}

/// Refuse une série vide, trop longue, avec des espaces ou commençant par `-` (qui serait lue
/// comme une option d'adb).
pub fn validate_serial(serial: &str) -> Result<(), AdbError> {
    let valid = !serial.is_empty()
        && serial.len() <= MAX_SERIAL_LEN
        && !serial.starts_with('-')
        && !serial.chars().any(|c| c.is_whitespace() || c.is_control());
    if valid {
        Ok(())
    } else {
        Err(AdbError::InvalidSerial)
    }
}

#[derive(Debug, Clone)]
pub struct Adb {
    path: PathBuf,
    timeout: Duration,
}

impl Adb {
    pub fn new(path: PathBuf) -> Self {
        Adb {
            path,
            timeout: COMMAND_TIMEOUT,
        }
    }

    /// Délai des commandes sur le téléphone (celui du serveur reste `SERVER_TIMEOUT`).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Cherche adb : variable `PCCHECK_ADB`, puis chaque dossier d'outils donné, dans l'ordre.
    /// Pas de repli sur le PATH : l'outil doit utiliser la version embarquée sur la clé.
    pub fn locate(dirs: &[PathBuf]) -> Result<Self, AdbError> {
        process::locate(ENV_OVERRIDE, dirs, BINARY_NAME)
            .map(Adb::new)
            .map_err(AdbError::from)
    }

    pub fn version(&self) -> Result<AdbVersion, AdbError> {
        let out = self.run(&["version"], self.timeout)?;
        parse_version(&out).ok_or_else(|| AdbError::UnexpectedOutput {
            command: "adb version".into(),
            // Première ligne seulement : les suivantes contiennent le chemin d'installation.
            excerpt: out
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect(),
        })
    }

    /// Démarre le serveur adb s'il ne tourne pas déjà. Facultatif : `devices` le démarre aussi,
    /// mais l'appeler tôt (à l'ouverture de l'écran Téléphone) rend la détection plus rapide.
    pub fn start_server(&self) -> Result<(), AdbError> {
        self.run(&["start-server"], SERVER_TIMEOUT).map(|_| ())
    }

    /// Téléphones vus par adb, avec leur état et une consigne s'ils ne sont pas prêts.
    /// Liste vide : voir `devices::no_device_guidance`.
    pub fn devices(&self) -> Result<Vec<AdbDevice>, AdbError> {
        Ok(parse_devices(
            &self.run(&["devices", "-l"], SERVER_TIMEOUT)?,
        ))
    }

    /// Collecte tout ce qu'ADB peut lire sans root. Seules l'absence du téléphone, son état et
    /// l'échec de `getprop` sont des erreurs ; le reste est signalé dans `PhoneReport::issues`.
    pub fn collect(&self, serial: &str) -> Result<PhoneReport, AdbError> {
        validate_serial(serial)?;
        let device = self
            .devices()?
            .into_iter()
            .find(|d| d.serial == serial)
            .ok_or(AdbError::DeviceNotFound)?;
        if device.state != DeviceState::Device {
            let guidance = device.state.guidance().unwrap_or_default();
            return Err(AdbError::DeviceNotReady {
                state: device.state,
                guidance,
            });
        }

        let getprop = self.shell(serial, &["getprop"])?;
        if parse_getprop(&getprop).is_empty() {
            return Err(AdbError::NoResponse {
                command: "getprop".into(),
            });
        }

        let mut issues = Vec::new();
        let mut step = |step: CollectStep, command: &[&str]| match self.shell(serial, command) {
            Ok(out) => Some(out),
            Err(e) => {
                issues.push(CollectIssue {
                    step,
                    message: e.to_string(),
                });
                None
            }
        };
        let which_su = step(CollectStep::Root, &["which", "su"]);
        let battery = step(CollectStep::Battery, &["dumpsys", "battery"]);
        let accounts = step(CollectStep::Accounts, &["dumpsys", "account"]);
        let owners = step(CollectStep::Owners, &["dpm", "list-owners"]);
        // `dpm list-owners` n'existe qu'à partir d'Android 12 : repli sur dumpsys.
        let device_policy = match owners.as_deref().and_then(parse_dpm_list_owners) {
            Some(_) => None,
            None => step(CollectStep::Owners, &["dumpsys", "device_policy"]),
        };
        let storage = step(CollectStep::Storage, &["df", "/data"]);

        // Fichiers sysfs : souvent illisibles sans root, leur absence n'est pas un problème.
        let sysfs = |name: &str| {
            let path = format!("{SYSFS_BATTERY_DIR}/{name}");
            self.shell(serial, &["cat", &path]).ok()
        };
        let raw = RawCollection {
            getprop,
            which_su,
            battery,
            cycle_count: sysfs("cycle_count"),
            charge_full: sysfs("charge_full"),
            charge_full_design: sysfs("charge_full_design"),
            accounts,
            owners,
            device_policy,
            storage,
        };
        let mut report = assemble(serial, &raw);
        issues.append(&mut report.issues);
        report.issues = issues;
        Ok(report)
    }

    /// `adb -s <série> shell <commande>`. Le numéro de série est masqué dans les erreurs.
    fn shell(&self, serial: &str, command: &[&str]) -> Result<String, AdbError> {
        let mut args = vec!["-s", serial, "shell"];
        args.extend_from_slice(command);
        match self.run(&args, self.timeout) {
            Err(AdbError::Timeout { seconds, args }) => Err(AdbError::Timeout {
                seconds,
                args: args.replace(serial, &mask_serial(serial)),
            }),
            other => other,
        }
    }

    /// Lance adb et renvoie sa sortie standard. Le code de sortie n'est pas une erreur ici :
    /// une commande du téléphone qui échoue (`which su` sans résultat) le rend non nul.
    fn run(&self, args: &[&str], timeout: Duration) -> Result<String, AdbError> {
        Ok(process::run(&self.path, args, timeout)?.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_validation() {
        assert!(validate_serial("R58N00000XX").is_ok());
        assert!(validate_serial("192.168.1.50:5555").is_ok());
        assert!(validate_serial("adb-R58N00000XX-AbCdEf._adb-tls-connect._tcp").is_ok());
        assert_eq!(validate_serial(""), Err(AdbError::InvalidSerial));
        assert_eq!(validate_serial("-H"), Err(AdbError::InvalidSerial));
        assert_eq!(validate_serial("a b"), Err(AdbError::InvalidSerial));
        assert_eq!(
            validate_serial(&"x".repeat(200)),
            Err(AdbError::InvalidSerial)
        );
    }

    #[test]
    fn locate_reports_searched_paths() {
        // La variable d'environnement, si elle est définie sur la machine, fausserait le test.
        if std::env::var_os(ENV_OVERRIDE).is_some() {
            return;
        }
        let dirs = vec![
            PathBuf::from("/inexistant/a"),
            PathBuf::from("/inexistant/b"),
        ];
        match Adb::locate(&dirs) {
            Err(AdbError::NotFound { searched }) => assert_eq!(searched.len(), 2),
            other => panic!("attendu NotFound, obtenu {other:?}"),
        }
    }

    #[test]
    fn missing_binary_is_a_spawn_error() {
        let adb = Adb::new(PathBuf::from("/inexistant/adb"));
        assert!(matches!(adb.version(), Err(AdbError::Spawn { .. })));
        assert!(matches!(adb.collect("-oups"), Err(AdbError::InvalidSerial)));
    }

    #[test]
    fn errors_serialize_with_code() {
        let json = serde_json::to_value(AdbError::DeviceNotReady {
            state: DeviceState::Unauthorized,
            guidance: "x".into(),
        })
        .unwrap();
        assert_eq!(json["code"], "device_not_ready");
        assert_eq!(json["detail"]["state"], "unauthorized");
        assert!(AdbError::DeviceNotReady {
            state: DeviceState::Unauthorized,
            guidance: "x".into()
        }
        .to_string()
        .contains("« unauthorized »"));
    }
}
