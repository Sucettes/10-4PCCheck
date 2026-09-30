use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

/// Erreurs de la crate. Sérialisables pour être transmises telles quelles à l'interface.
#[derive(Debug, Clone, Error, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ReportError {
    #[error("sérialisation JSON impossible : {0}")]
    Json(String),
    #[error("compilation Typst échouée : {}", .0.join(" | "))]
    Typst(Vec<String>),
    #[error("export PDF échoué : {}", .0.join(" | "))]
    Pdf(Vec<String>),
    #[error("accès au fichier {path:?} impossible : {reason}")]
    Io { path: PathBuf, reason: String },
    #[error("rapport illisible {path:?} : {reason}")]
    InvalidReport { path: PathBuf, reason: String },
    #[error("rapport au schéma {found}, plus récent que ce que cet outil lit ({supported})")]
    UnsupportedSchema { found: u32, supported: u32 },
}

impl ReportError {
    pub(crate) fn io(path: impl Into<PathBuf>, err: std::io::Error) -> Self {
        ReportError::Io {
            path: path.into(),
            reason: err.to_string(),
        }
    }
}
