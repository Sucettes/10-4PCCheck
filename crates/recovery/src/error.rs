//! Erreurs de la récupération. Sérialisables : l'interface affiche `code` et le message.

use std::path::PathBuf;

use pccheck_core::process::ProcessError;
use serde::Serialize;
use thiserror::Error;

use crate::disk::DiskId;

#[derive(Debug, Clone, Error, Serialize, PartialEq, Eq)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum RecoveryError {
    #[error(
        "PhotoRec introuvable (cherché dans : {})",
        pccheck_core::process::display_paths(searched)
    )]
    PhotorecNotFound { searched: Vec<PathBuf> },

    #[error(
        "The Sleuth Kit introuvable (cherché dans : {})",
        pccheck_core::process::display_paths(searched)
    )]
    TskNotFound { searched: Vec<PathBuf> },

    #[error("{tool} : {message}")]
    ToolFailed { tool: String, message: String },

    #[error("impossible de lancer {} : {reason}", path.display())]
    Spawn { path: PathBuf, reason: String },

    #[error(
        "la destination {} est sur le même disque que la source ({disk}) : \
         choisis un autre disque (clé USB, disque externe), sinon la récupération écraserait \
         les fichiers que tu cherches à récupérer",
        path.display()
    )]
    SameDisk { path: PathBuf, disk: DiskId },

    #[error(
        "impossible de savoir sur quel disque se trouve {} ({reason}) : \
         par sécurité, choisis une destination sur un autre disque identifiable",
        path.display()
    )]
    DestinationDiskUnknown { path: PathBuf, reason: String },

    #[error(
        "impossible de savoir sur quel disque physique se trouve le volume {} ({reason}) : \
         sans cela, on ne peut pas garantir que la destination est ailleurs",
        path.display()
    )]
    SourceDiskUnknown { path: PathBuf, reason: String },

    #[error("destination invalide {} : {reason}", path.display())]
    InvalidDestination { path: PathBuf, reason: String },

    #[error("impossible de créer le dossier {} : {reason}", path.display())]
    CreateDestination { path: PathBuf, reason: String },

    #[error("choisis au moins un type de fichiers à récupérer")]
    NoFileFamily,

    #[error("nom de disque non pris en charge pour PhotoRec : {name}")]
    UnsupportedDeviceName { name: String },

    #[error("lecture de {} impossible : {reason}", path.display())]
    Io { path: PathBuf, reason: String },
}

impl RecoveryError {
    pub(crate) fn io(path: impl Into<PathBuf>, e: &std::io::Error) -> Self {
        RecoveryError::Io {
            path: path.into(),
            reason: e.to_string(),
        }
    }
}

impl From<ProcessError> for RecoveryError {
    fn from(e: ProcessError) -> Self {
        match e {
            ProcessError::NotFound { searched, .. } => RecoveryError::PhotorecNotFound { searched },
            ProcessError::Spawn { path, reason } => RecoveryError::Spawn { path, reason },
            // Le programme a bien démarré mais n'a pas fini à temps (fls sur un très gros volume
            // ou un disque qui relit ses secteurs défectueux).
            ProcessError::Timeout { path, seconds, .. } => RecoveryError::ToolFailed {
                tool: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                message: format!(
                    "pas de réponse après {} min (disque très lent ou abîmé ?)",
                    seconds.div_ceil(60)
                ),
            },
        }
    }
}
