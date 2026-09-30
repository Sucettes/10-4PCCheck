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

    #[error("impossible de lancer {path:?} : {reason}")]
    Spawn { path: PathBuf, reason: String },

    #[error(
        "la destination {path:?} est sur le même disque que la source ({disk}) : \
         choisis un autre disque (clé USB, disque externe), sinon PhotoRec écraserait \
         les fichiers que tu cherches à récupérer"
    )]
    SameDisk { path: PathBuf, disk: DiskId },

    #[error(
        "impossible de savoir sur quel disque se trouve {path:?} ({reason}) : \
         par sécurité, choisis une destination sur un autre disque identifiable"
    )]
    DestinationDiskUnknown { path: PathBuf, reason: String },

    #[error("destination invalide {path:?} : {reason}")]
    InvalidDestination { path: PathBuf, reason: String },

    #[error("impossible de créer le dossier {path:?} : {reason}")]
    CreateDestination { path: PathBuf, reason: String },

    #[error("choisis au moins un type de fichiers à récupérer")]
    NoFileFamily,

    #[error("nom de disque non pris en charge pour PhotoRec : {name}")]
    UnsupportedDeviceName { name: String },

    #[error("lecture de {path:?} impossible : {reason}")]
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
            // `locate` ne produit jamais de délai dépassé ; conversion gardée pour être exhaustif.
            ProcessError::Timeout { path, seconds, .. } => RecoveryError::Spawn {
                path,
                reason: format!("délai de {seconds} s dépassé"),
            },
        }
    }
}
