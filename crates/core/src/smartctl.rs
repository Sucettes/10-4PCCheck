//! Localisation et exécution du binaire `smartctl` embarqué sur la clé.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

use crate::disk::{
    dedupe_disks, parse_disk, parse_scan, DiskEntry, DiskInfo, RawOutput, ScanDevice,
    FATAL_EXIT_BITS,
};
use crate::selftest::{parse_self_test_status, SelfTestKind, SelfTestStatus};

/// Variable d'environnement qui force le chemin de smartctl (tests, développement).
pub const ENV_OVERRIDE: &str = "PCCHECK_SMARTCTL";

/// Délai maximal d'un appel. Certains ponts USB bloquent au lieu de répondre.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(windows)]
const BINARY_NAME: &str = "smartctl.exe";
#[cfg(not(windows))]
const BINARY_NAME: &str = "smartctl";

#[derive(Debug, Clone, Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum SmartctlError {
    #[error("smartctl introuvable (cherché dans : {searched:?})")]
    NotFound { searched: Vec<PathBuf> },
    #[error("impossible de lancer {path:?} : {reason}")]
    Spawn { path: PathBuf, reason: String },
    #[error("smartctl n'a pas répondu en {seconds} s ({args})")]
    Timeout { seconds: u64, args: String },
    #[error("sortie JSON invalide : {reason}")]
    InvalidJson { reason: String, excerpt: String },
    #[error("version du format JSON non prise en charge : {0:?}")]
    UnsupportedJsonVersion(Vec<u32>),
    #[error("smartctl a échoué (code {exit_status}) : {}", messages.join(" | "))]
    CommandFailed {
        exit_status: u8,
        messages: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Smartctl {
    path: PathBuf,
    timeout: Duration,
}

impl Smartctl {
    pub fn new(path: PathBuf) -> Self {
        Smartctl {
            path,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Cherche smartctl : variable `PCCHECK_SMARTCTL`, puis chaque dossier donné, dans l'ordre.
    /// Pas de repli sur le PATH : l'outil doit utiliser la version embarquée sur la clé.
    pub fn locate(dirs: &[PathBuf]) -> Result<Self, SmartctlError> {
        let env = std::env::var_os(ENV_OVERRIDE).map(PathBuf::from);
        Self::locate_with(env, dirs)
    }

    fn locate_with(env: Option<PathBuf>, dirs: &[PathBuf]) -> Result<Self, SmartctlError> {
        let mut searched = Vec::new();
        let candidates = env
            .into_iter()
            .chain(dirs.iter().map(|d| d.join(BINARY_NAME)));
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(Smartctl::new(candidate));
            }
            searched.push(candidate);
        }
        Err(SmartctlError::NotFound { searched })
    }

    /// Première ligne de `smartctl --version`, ex. « smartctl 7.5 2025-04-30 r5714 ».
    pub fn version(&self) -> Result<String, SmartctlError> {
        let out = self.run(&["--version"])?;
        Ok(out.lines().next().unwrap_or_default().trim().to_string())
    }

    /// Liste des disques que smartctl sait ouvrir.
    pub fn scan(&self) -> Result<Vec<ScanDevice>, SmartctlError> {
        parse_scan(&self.run(&["--scan-open", "-j"])?)
    }

    /// Toutes les informations SMART d'un disque.
    pub fn info(&self, device: &ScanDevice) -> Result<DiskInfo, SmartctlError> {
        parse_disk(&self.run(&device_args(&["-a", "-j"], device))?, device)
    }

    /// Lance un auto-test. Le disque le fait seul ; `self_test_status` en suit la progression.
    pub fn start_self_test(
        &self,
        device: &ScanDevice,
        kind: SelfTestKind,
    ) -> Result<(), SmartctlError> {
        let out = self.run(&device_args(&["-t", kind.smartctl_arg(), "-j"], device))?;
        // Bit 2 : le disque a refusé la commande (test non pris en charge, déjà en cours...).
        RawOutput::parse(&out)?.check_exit(FATAL_EXIT_BITS | 0b100)
    }

    /// Interrompt l'auto-test en cours.
    pub fn abort_self_test(&self, device: &ScanDevice) -> Result<(), SmartctlError> {
        let out = self.run(&device_args(&["-X", "-j"], device))?;
        RawOutput::parse(&out)?.check_exit(FATAL_EXIT_BITS | 0b100)
    }

    /// État de l'auto-test en cours, durées estimées et derniers résultats.
    pub fn self_test_status(&self, device: &ScanDevice) -> Result<SelfTestStatus, SmartctlError> {
        parse_self_test_status(&self.run(&device_args(&["-c", "-l", "selftest", "-j"], device))?)
    }

    /// Liste les disques puis lit chacun, sans doublons (voir `dedupe_disks`).
    /// Seul l'échec du scan est une erreur ; l'échec d'un disque est rendu avec lui.
    pub fn scan_all(&self) -> Result<Vec<DiskEntry>, SmartctlError> {
        let entries = self
            .scan()?
            .into_iter()
            .map(|device| match self.info(&device) {
                Ok(info) => DiskEntry {
                    device,
                    info: Some(info),
                    error: None,
                },
                Err(error) => DiskEntry {
                    device,
                    info: None,
                    error: Some(error),
                },
            })
            .collect();
        Ok(dedupe_disks(entries))
    }

    /// Lance smartctl et renvoie sa sortie standard. Le code de sortie n'est pas une erreur ici :
    /// smartctl l'utilise comme masque de bits, interprété par l'appelant à partir du JSON.
    fn run(&self, args: &[&str]) -> Result<String, SmartctlError> {
        let mut cmd = Command::new(&self.path);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| SmartctlError::Spawn {
            path: self.path.clone(),
            reason: e.to_string(),
        })?;

        // Lecture dans un fil séparé : un tuyau plein bloquerait smartctl avant sa fin.
        let mut stdout = child.stdout.take().expect("stdout configuré en pipe");
        let reader = thread::spawn(move || {
            let mut buf = String::new();
            stdout.read_to_string(&mut buf).map(|_| buf)
        });

        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() >= deadline => {
                    // Échec du kill ignoré à dessein : le processus peut s'être terminé entre-temps.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SmartctlError::Timeout {
                        seconds: self.timeout.as_secs(),
                        args: args.join(" "),
                    });
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(e) => {
                    return Err(SmartctlError::Spawn {
                        path: self.path.clone(),
                        reason: e.to_string(),
                    })
                }
            }
        }

        match reader.join() {
            Ok(Ok(buf)) => Ok(buf),
            Ok(Err(e)) => Err(SmartctlError::Spawn {
                path: self.path.clone(),
                reason: e.to_string(),
            }),
            Err(_) => Err(SmartctlError::Spawn {
                path: self.path.clone(),
                reason: "le fil de lecture de la sortie a paniqué".into(),
            }),
        }
    }
}

/// Arguments d'une commande sur un disque : `base`, puis `-d <type>` si connu, puis le chemin.
fn device_args<'a>(base: &[&'a str], device: &'a ScanDevice) -> Vec<&'a str> {
    let mut args = base.to_vec();
    if !device.dev_type.is_empty() {
        args.extend(["-d", device.dev_type.as_str()]);
    }
    args.push(device.name.as_str());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_reports_every_searched_path() {
        let dirs = vec![
            PathBuf::from("/inexistant/a"),
            PathBuf::from("/inexistant/b"),
        ];
        match Smartctl::locate_with(None, &dirs) {
            Err(SmartctlError::NotFound { searched }) => assert_eq!(searched.len(), 2),
            other => panic!("attendu NotFound, obtenu {other:?}"),
        }
    }

    #[test]
    fn locate_prefers_env_override() {
        let dir = tempdir("locate_env");
        let bin = dir.join(BINARY_NAME);
        std::fs::write(&bin, b"").unwrap();
        let other = tempdir("locate_env_other");
        std::fs::write(other.join(BINARY_NAME), b"").unwrap();
        let found = Smartctl::locate_with(Some(bin.clone()), &[other]).unwrap();
        assert_eq!(found.path(), bin);
    }

    #[test]
    fn locate_falls_back_to_dirs_when_env_missing() {
        let dir = tempdir("locate_dirs");
        std::fs::write(dir.join(BINARY_NAME), b"").unwrap();
        let found = Smartctl::locate_with(
            Some(PathBuf::from("/inexistant/smartctl")),
            std::slice::from_ref(&dir),
        )
        .unwrap();
        assert_eq!(found.path(), dir.join(BINARY_NAME));
    }

    #[cfg(unix)]
    #[test]
    fn run_times_out_and_kills_process() {
        let script = fake_smartctl("timeout", "sleep 5");
        let s = Smartctl::new(script).with_timeout(Duration::from_millis(300));
        let start = Instant::now();
        assert!(matches!(s.version(), Err(SmartctlError::Timeout { .. })));
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn info_maps_fatal_exit_bits_to_error() {
        let json = r#"{"json_format_version":[1,0],"smartctl":{"exit_status":2,"messages":[{"string":"Unknown USB bridge","severity":"error"}]}}"#;
        let script = fake_smartctl("fatal", &format!("echo '{json}'; exit 2"));
        let dev = ScanDevice {
            name: "/dev/sdb".into(),
            info_name: "/dev/sdb".into(),
            dev_type: String::new(),
            protocol: String::new(),
        };
        match Smartctl::new(script).info(&dev) {
            Err(SmartctlError::CommandFailed {
                exit_status: 2,
                messages,
            }) => assert_eq!(messages, vec!["Unknown USB bridge"]),
            other => panic!("attendu CommandFailed, obtenu {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_calls_do_not_interfere() {
        let json = r#"{"json_format_version":[1,0],"devices":[{"name":"/dev/sda","type":"sat","protocol":"ATA"}]}"#;
        let script = fake_smartctl("concurrent", &format!("echo '{json}'"));
        let s = Smartctl::new(script);
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let s = s.clone();
                thread::spawn(move || s.scan())
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap().unwrap().len(), 1);
        }
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pccheck-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn fake_smartctl(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = tempdir(name).join("smartctl");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}
