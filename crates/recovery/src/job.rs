//! Une récupération en cours : lancement de PhotoRec, suivi, arrêt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use pccheck_core::process;
use serde::Serialize;

use crate::config::{build_args, RecoveryConfig, LOG_FILE_NAME};
use crate::disk::validate_destination;
use crate::error::RecoveryError;
use crate::log::{parse_log, read_log_tail, LogSummary};
use crate::scan::{count_found_cached, max_recup_index, FoundFile, ScanCache};
use crate::sys::ChildProcess;

/// Variable d'environnement qui force le chemin de PhotoRec (tests, développement).
pub const ENV_OVERRIDE: &str = "PCCHECK_PHOTOREC";

/// Noms du binaire embarqué, par ordre de préférence (plan, section 5).
#[cfg(windows)]
pub const BINARY_NAMES: &[&str] = &["photorec_win.exe"];
#[cfg(not(windows))]
pub const BINARY_NAMES: &[&str] = &["photorec_static", "photorec"];

/// Délai laissé à PhotoRec pour s'arrêter proprement (SIGTERM) avant de le tuer.
const GRACEFUL_STOP: Duration = Duration::from_secs(5);

/// Cherche PhotoRec : variable `PCCHECK_PHOTOREC`, puis chaque dossier, pour chaque nom de
/// `BINARY_NAMES`. Pas de repli sur le PATH : on veut la version embarquée sur la clé.
pub fn locate_photorec(dirs: &[PathBuf]) -> Result<PathBuf, RecoveryError> {
    let mut searched: Vec<PathBuf> = Vec::new();
    for name in BINARY_NAMES {
        match process::locate(ENV_OVERRIDE, dirs, name) {
            Ok(path) => return Ok(path),
            Err(process::ProcessError::NotFound { searched: s, .. }) => {
                // La variable d'environnement apparaît pour chaque nom : une seule fois suffit.
                for p in s {
                    if !searched.contains(&p) {
                        searched.push(p);
                    }
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(RecoveryError::PhotorecNotFound { searched })
}

/// État d'une récupération, pour l'écran de progression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryProgress {
    pub running: bool,
    pub elapsed_s: u64,
    pub files_found: u64,
    pub bytes_found: u64,
    /// Nombre de fichiers par extension (minuscules).
    pub by_extension: BTreeMap<String, u64>,
    /// Les 10 derniers fichiers trouvés, du plus récent au plus ancien.
    pub last_files: Vec<FoundFile>,
    /// Code de sortie de PhotoRec une fois terminé (négatif : tué par ce signal, sous Unix).
    pub exit_code: Option<i32>,
    /// Arrêt demandé depuis l'interface.
    pub stopped_by_user: bool,
    /// Informations du journal `photorec.log` de cette exécution, s'il existe.
    pub log: Option<LogSummary>,
    /// Explication lisible d'une fin anormale.
    pub problem: Option<String>,
    /// Erreur de lecture du dossier de destination (clé débranchée...).
    pub scan_error: Option<String>,
}

/// Une exécution de PhotoRec. Ce n'est pas une donnée sérialisable mais une poignée sur un
/// processus : l'interface la garde dans son état et interroge `progress`.
/// `Drop` arrête PhotoRec : fermer l'application ne laisse pas tourner un processus invisible.
pub struct RecoveryJob {
    config: RecoveryConfig,
    args: Vec<String>,
    started: Instant,
    /// Premier `recup_dir.N` de cette exécution (les précédents appartiennent à d'autres).
    first_index: u32,
    /// Taille du journal au lancement : le début concerne des exécutions précédentes.
    log_offset: u64,
    state: Mutex<JobState>,
}

struct JobState {
    child: Option<ChildProcess>,
    exit_code: Option<i32>,
    finished_at: Option<Instant>,
    stopped_by_user: bool,
    cache: ScanCache,
}

impl RecoveryJob {
    /// Vérifie la destination (autre disque que la source), crée le dossier et lance PhotoRec.
    pub fn start(
        config: RecoveryConfig,
        photorec_path: &Path,
    ) -> Result<RecoveryJob, RecoveryError> {
        if config.families.is_empty() {
            return Err(RecoveryError::NoFileFamily);
        }
        if config.destination.to_str().is_none() {
            return Err(RecoveryError::InvalidDestination {
                path: config.destination.clone(),
                reason: "le chemin contient des caractères non UTF-8".into(),
            });
        }
        let args = build_args(&config);
        Self::spawn(config, photorec_path, args)
    }

    /// Lancement avec des arguments donnés (tests : faux PhotoRec). Mêmes vérifications de
    /// destination que `start`.
    pub(crate) fn spawn(
        config: RecoveryConfig,
        program: &Path,
        args: Vec<String>,
    ) -> Result<RecoveryJob, RecoveryError> {
        let dest = &config.destination;
        // Vérifié AVANT de créer le dossier : créer un dossier sur la source serait déjà une
        // écriture sur le disque qu'on veut récupérer.
        validate_destination(config.source.disk(), dest)?;
        std::fs::create_dir_all(dest).map_err(|e| RecoveryError::CreateDestination {
            path: dest.clone(),
            reason: e.to_string(),
        })?;
        // Seconde vérification sur le dossier réel (un point de montage a pu être traversé).
        validate_destination(config.source.disk(), dest)?;

        let first_index = max_recup_index(dest)? + 1;
        let log_offset = std::fs::metadata(dest.join(LOG_FILE_NAME)).map_or(0, |m| m.len());
        let child = ChildProcess::spawn(program, &args, dest)?;
        Ok(RecoveryJob {
            config,
            args,
            started: Instant::now(),
            first_index,
            log_offset,
            state: Mutex::new(JobState {
                child: Some(child),
                exit_code: None,
                finished_at: None,
                stopped_by_user: false,
                cache: ScanCache::default(),
            }),
        })
    }

    pub fn config(&self) -> &RecoveryConfig {
        &self.config
    }

    /// Arguments passés à PhotoRec (affichés dans le rapport, utiles au débogage).
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Relève l'état : processus, fichiers trouvés, journal. Coût : un parcours du dernier
    /// dossier `recup_dir.N` (les précédents sont mémorisés).
    pub fn progress(&self) -> RecoveryProgress {
        let mut state = self.lock();
        poll(&mut state);
        let running = state.child.is_some();
        let end = state.finished_at.unwrap_or_else(Instant::now);
        let (summary, scan_error) = match count_found_cached(
            &self.config.destination,
            self.first_index,
            &mut state.cache,
        ) {
            Ok(s) => (s, None),
            Err(e) => (Default::default(), Some(e.to_string())),
        };
        let log = read_log_tail(
            &self.config.destination.join(LOG_FILE_NAME),
            self.log_offset,
        )
        .ok()
        .map(|t| parse_log(&t));
        let problem = explain_end(
            state.exit_code,
            state.stopped_by_user,
            log.as_ref(),
            summary.files_found,
        );
        RecoveryProgress {
            running,
            elapsed_s: end.duration_since(self.started).as_secs(),
            files_found: summary.files_found,
            bytes_found: summary.bytes_found,
            by_extension: summary.by_extension,
            last_files: summary.last_files,
            exit_code: state.exit_code,
            stopped_by_user: state.stopped_by_user,
            log,
            problem,
            scan_error,
        }
    }

    /// Arrête PhotoRec : arrêt propre (Unix : SIGTERM, PhotoRec ferme le fichier en cours),
    /// puis arrêt forcé après `GRACEFUL_STOP`. Sans effet si déjà terminé.
    pub fn stop(&mut self) {
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        poll(state);
        let Some(child) = state.child.as_mut() else {
            return;
        };
        state.stopped_by_user = true;
        child.request_stop();
        if wait_exit(child, GRACEFUL_STOP).is_none() {
            child.kill();
            // Récupère le code (et évite un zombie sous Unix).
            let _ = wait_exit(child, Duration::from_secs(2));
        }
        poll(state);
        // Toujours pas terminé (processus bloqué dans le noyau) : on le lâche, mais on
        // n'affiche plus « en cours ».
        if state.child.take().is_some() {
            state.finished_at = Some(Instant::now());
        }
    }

    fn lock(&self) -> MutexGuard<'_, JobState> {
        // Un fil qui a paniqué en tenant le verrou ne rend pas l'état incohérent ici.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for RecoveryJob {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Met à jour l'état si PhotoRec s'est terminé.
fn poll(state: &mut JobState) {
    let Some(child) = state.child.as_mut() else {
        return;
    };
    match child.try_wait() {
        Ok(Some(code)) => {
            state.exit_code = Some(code);
            state.finished_at = Some(Instant::now());
            state.child = None;
        }
        Ok(None) => {}
        // Poignée inutilisable : on ne peut plus suivre le processus.
        Err(_) => {
            state.finished_at = Some(Instant::now());
            state.child = None;
        }
    }
}

fn wait_exit(child: &mut ChildProcess, max: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        if let Ok(Some(code)) = child.try_wait() {
            return Some(code);
        }
        if start.elapsed() >= max {
            return None;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Message pour une fin anormale, ou `None`. Pure : testée sans PhotoRec.
pub(crate) fn explain_end(
    exit_code: Option<i32>,
    stopped_by_user: bool,
    log: Option<&LogSummary>,
    files_found: u64,
) -> Option<String> {
    let code = exit_code?;
    if stopped_by_user {
        return None;
    }
    if let Some(err) = log.and_then(|l| l.errors.last()) {
        return Some(format!("PhotoRec signale une erreur : {err}"));
    }
    if log.is_some_and(|l| l.finished_normally) {
        return None;
    }
    if code != 0 && files_found == 0 {
        // Code 1 avant toute écriture : ouverture du disque refusée (phmain.c, « Unable to
        // open file or device »), le plus souvent faute de droits administrateur.
        return Some(format!(
            "PhotoRec s'est arrêté (code {code}) sans rien trouver : vérifie que 10-4 PCCheck \
             tourne en administrateur et que le disque source est toujours branché"
        ));
    }
    if code != 0 {
        return Some(format!(
            "PhotoRec s'est arrêté de façon inattendue (code {code})"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_explanations() {
        assert_eq!(explain_end(None, false, None, 0), None);
        assert_eq!(explain_end(Some(1), true, None, 0), None);
        assert!(explain_end(Some(1), false, None, 0)
            .unwrap()
            .contains("administrateur"));
        assert!(explain_end(Some(9), false, None, 4)
            .unwrap()
            .contains("inattendue"));
        let ok = LogSummary {
            finished_normally: true,
            ..Default::default()
        };
        assert_eq!(explain_end(Some(0), false, Some(&ok), 3), None);
        let err = LogSummary {
            errors: vec!["Syntax error in command line: jpeg,enable,search".into()],
            ..Default::default()
        };
        assert!(explain_end(Some(0), false, Some(&err), 0)
            .unwrap()
            .contains("Syntax error"));
    }

    #[test]
    fn locate_lists_every_candidate_once() {
        let dirs = vec![PathBuf::from("/inexistant/a")];
        match locate_photorec(&dirs) {
            Err(RecoveryError::PhotorecNotFound { searched }) => {
                // Un chemin par nom de binaire, plus la variable d'environnement si définie.
                assert!(searched.len() >= BINARY_NAMES.len());
                let mut unique = searched.clone();
                unique.dedup();
                assert_eq!(unique.len(), searched.len());
            }
            Ok(p) => assert!(std::env::var_os(ENV_OVERRIDE).is_some(), "trouvé : {p:?}"),
            Err(e) => panic!("erreur inattendue : {e}"),
        }
    }

    // ---------- Faux PhotoRec : cycle complet lancement → progression → arrêt ----------

    use crate::config::{FileFamily, Source};
    use crate::disk::DiskId;

    fn fake_config(name: &str) -> RecoveryConfig {
        let destination = std::env::temp_dir().join(format!(
            "pccheck-recovery-fake-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&destination);
        RecoveryConfig {
            // Disque qui n'existe pas : la destination (dossier temporaire) est forcément ailleurs.
            source: Source::Disk {
                disk: if cfg!(windows) {
                    DiskId::PhysicalDrive(99)
                } else {
                    DiskId::Block("pccheck-faux-disque".into())
                },
            },
            destination,
            families: vec![FileFamily::Everything],
            paranoid: true,
        }
    }

    /// Programme et arguments d'un faux PhotoRec qui écrit un fichier dans `recup_dir.1`,
    /// puis attend `wait_s` secondes (ou sort avec `code` si `wait_s` vaut 0).
    fn fake_photorec(wait_s: u32, code: i32) -> (PathBuf, Vec<String>) {
        if cfg!(windows) {
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
            let cmd = PathBuf::from(root).join("System32").join("cmd.exe");
            let script = if wait_s > 0 {
                format!(
                    r"mkdir recup_dir.1 && echo x> recup_dir.1\f0000100.jpg && ping -n {} 127.0.0.1 > nul",
                    wait_s + 1
                )
            } else {
                format!("exit {code}")
            };
            (cmd, vec!["/c".into(), script])
        } else {
            let script = if wait_s > 0 {
                format!(
                    "mkdir recup_dir.1 && echo x > recup_dir.1/f0000100.jpg && exec sleep {wait_s}"
                )
            } else {
                format!("exit {code}")
            };
            (PathBuf::from("/bin/sh"), vec!["-c".into(), script])
        }
    }

    fn wait_for(job: &RecoveryJob, what: impl Fn(&RecoveryProgress) -> bool) -> RecoveryProgress {
        let start = Instant::now();
        loop {
            let p = job.progress();
            if what(&p) || start.elapsed() > Duration::from_secs(15) {
                return p;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn fake_run_is_followed_then_stopped() {
        let config = fake_config("suivi");
        let dest = config.destination.clone();
        let (program, args) = fake_photorec(30, 0);
        let mut job = RecoveryJob::spawn(config, &program, args).unwrap();
        assert!(dest.is_dir(), "dossier de destination créé");

        let p = wait_for(&job, |p| p.files_found == 1);
        assert!(p.running);
        assert_eq!(p.files_found, 1);
        assert_eq!(p.by_extension.get("jpg"), Some(&1));
        assert_eq!(p.last_files[0].name, "f0000100.jpg");
        assert_eq!(p.exit_code, None);

        let before = Instant::now();
        job.stop();
        assert!(before.elapsed() < Duration::from_secs(10));
        let p = job.progress();
        assert!(!p.running);
        assert!(p.stopped_by_user);
        assert!(p.exit_code.is_some());
        assert_eq!(p.problem, None, "arrêt volontaire : pas d'alerte");
        assert_eq!(p.files_found, 1, "les fichiers trouvés restent");
        // Un second arrêt (et le Drop) ne font rien.
        job.stop();
    }

    #[test]
    fn early_failure_is_explained() {
        let config = fake_config("echec");
        let (program, args) = fake_photorec(0, 1);
        let job = RecoveryJob::spawn(config, &program, args).unwrap();
        let p = wait_for(&job, |p| !p.running);
        assert!(!p.running);
        assert_eq!(p.exit_code, Some(1));
        assert!(!p.stopped_by_user);
        assert!(p.problem.unwrap_or_default().contains("administrateur"));
    }

    #[test]
    fn previous_runs_in_same_destination_are_not_counted() {
        let config = fake_config("reprise");
        let old = config.destination.join("recup_dir.1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("f0000001.jpg"), b"ancien").unwrap();
        // Le faux PhotoRec réécrit dans recup_dir.1 : hors de cette exécution (qui commence à 2).
        let (program, args) = fake_photorec(0, 0);
        let job = RecoveryJob::spawn(config, &program, args).unwrap();
        let p = wait_for(&job, |p| !p.running);
        assert_eq!(p.files_found, 0);
        assert_eq!(p.exit_code, Some(0));
    }
}
