//! Partie dépendante du système : lancement de PhotoRec, disque physique d'un dossier, liste
//! des volumes. Chaque plateforme expose les mêmes noms :
//! - `ChildProcess` (lancement, attente non bloquante, arrêt) ;
//! - `locate_path(&Path) -> DestinationLocation` (chemin existant) ;
//! - `list_volumes() -> Vec<Volume>`.

pub(crate) mod cmdline;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{list_volumes, locate_path, ChildProcess};

#[cfg(unix)]
mod unix_process;
#[cfg(unix)]
pub(crate) use self::unix_process::ChildProcess;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use self::linux::{list_volumes, locate_path};

#[cfg(not(any(windows, target_os = "linux")))]
mod other;
#[cfg(not(any(windows, target_os = "linux")))]
pub(crate) use self::other::{list_volumes, locate_path};
