//! Erreurs de lecture d'une partie de l'inventaire. Elles ne remontent jamais comme échec
//! global : `collect` les convertit en lignes de `MachineInventory::errors`.

use std::fmt::Display;

use pccheck_core::ProcessError;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Error, Serialize, PartialEq, Eq)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum InventoryError {
    /// Lecture refusée : relancer l'outil en administrateur (Windows) ou en root (Linux).
    #[error("{part} : droits administrateur requis")]
    AdminRequired { part: String },
    #[error("{part} : {reason}")]
    Unreadable { part: String, reason: String },
    #[error("{part} : {error}")]
    Process { part: String, error: ProcessError },
}

impl InventoryError {
    pub fn admin(part: &str) -> Self {
        InventoryError::AdminRequired { part: part.into() }
    }

    pub fn unreadable(part: &str, reason: impl Display) -> Self {
        InventoryError::Unreadable {
            part: part.into(),
            reason: reason.to_string(),
        }
    }

    pub fn process(part: &str, error: ProcessError) -> Self {
        InventoryError::Process {
            part: part.into(),
            error,
        }
    }
}

/// Garde la valeur, ou note l'erreur dans la liste et rend `None`.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
pub(crate) fn record<T>(errors: &mut Vec<String>, r: Result<T, InventoryError>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(e.to_string());
            None
        }
    }
}
