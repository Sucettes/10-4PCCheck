//! Derniers résultats obtenus pendant la session (voir `pccheck_assemble::Results`), partagés
//! entre les commandes et les tâches de fond.

use std::sync::{Arc, Mutex, MutexGuard};

pub use pccheck_assemble::Results;

#[derive(Clone, Default)]
pub struct Cache(Arc<Mutex<Results>>);

impl Cache {
    /// Verrou empoisonné (panique dans un autre fil pendant une écriture) : on récupère quand
    /// même les données, un résultat partiel vaut mieux qu'un rapport impossible.
    pub fn lock(&self) -> MutexGuard<'_, Results> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}
