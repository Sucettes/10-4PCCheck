//! Tâches longues en arrière-plan (scan de surface, capacité, charge CPU, RAM, PhotoRec).
//!
//! Chaque tâche a un identifiant (`surface:/dev/sda`), tourne dans son propre fil et publie :
//! - `job-progress` { id, data } pendant l'exécution ;
//! - `job-done` { id, result } à la fin (`result` : { ok } ou { error }).
//!
//! Le dernier état est gardé côté Rust : l'interface peut changer d'écran et le retrouver avec
//! `job_state`. Une seule tâche par identifiant à la fois.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Default, Serialize)]
pub struct JobState {
    pub running: bool,
    pub progress: Option<Value>,
    /// `{ "ok": ... }` ou `{ "error": ... }` une fois terminée.
    pub result: Option<Value>,
}

struct Entry {
    cancel: Arc<AtomicBool>,
    state: JobState,
}

#[derive(Default)]
pub struct Jobs(Mutex<HashMap<String, Entry>>);

#[derive(Clone, Serialize)]
struct ProgressEvent<'a> {
    id: &'a str,
    data: &'a Value,
}

#[derive(Clone, Serialize)]
struct DoneEvent<'a> {
    id: &'a str,
    result: &'a Value,
}

/// Transmis à la tâche : drapeau d'annulation et publication de la progression.
pub struct JobContext {
    id: String,
    app: AppHandle,
    jobs: Arc<Jobs>,
    pub cancel: Arc<AtomicBool>,
}

impl JobContext {
    pub fn progress<P: Serialize>(&self, data: &P) {
        let Ok(value) = serde_json::to_value(data) else {
            return;
        };
        if let Ok(mut map) = self.jobs.0.lock() {
            if let Some(entry) = map.get_mut(&self.id) {
                entry.state.progress = Some(value.clone());
            }
        }
        // Échec d'émission ignoré : fenêtre fermée pendant la tâche.
        let _ = self.app.emit(
            "job-progress",
            ProgressEvent {
                id: &self.id,
                data: &value,
            },
        );
    }
}

impl Jobs {
    /// Démarre `work` dans un fil. Erreur si une tâche du même identifiant tourne déjà.
    pub fn start<T, E>(
        self: &Arc<Self>,
        app: &AppHandle,
        id: String,
        work: impl FnOnce(&JobContext) -> Result<T, E> + Send + 'static,
    ) -> Result<(), String>
    where
        T: Serialize,
        E: Serialize,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut map = self
                .0
                .lock()
                .map_err(|_| "état des tâches illisible".to_string())?;
            if map.get(&id).is_some_and(|e| e.state.running) {
                return Err("cette tâche est déjà en cours".into());
            }
            map.insert(
                id.clone(),
                Entry {
                    cancel: cancel.clone(),
                    state: JobState {
                        running: true,
                        progress: None,
                        result: None,
                    },
                },
            );
        }
        let ctx = JobContext {
            id,
            app: app.clone(),
            jobs: self.clone(),
            cancel,
        };
        std::thread::spawn(move || {
            let result = match work(&ctx) {
                Ok(v) => serde_json::json!({ "ok": v }),
                Err(e) => serde_json::json!({ "error": e }),
            };
            if let Ok(mut map) = ctx.jobs.0.lock() {
                if let Some(entry) = map.get_mut(&ctx.id) {
                    entry.state.running = false;
                    entry.state.result = Some(result.clone());
                }
            }
            let _ = ctx.app.emit(
                "job-done",
                DoneEvent {
                    id: &ctx.id,
                    result: &result,
                },
            );
        });
        Ok(())
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(map) = self.0.lock() {
            if let Some(entry) = map.get(id) {
                entry.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    pub fn state(&self, id: &str) -> JobState {
        self.0
            .lock()
            .ok()
            .and_then(|map| map.get(id).map(|e| e.state.clone()))
            .unwrap_or_default()
    }

    pub fn any_running(&self) -> bool {
        self.0
            .lock()
            .map(|map| map.values().any(|e| e.state.running))
            .unwrap_or(false)
    }

    /// Annule tout (fermeture de l'application).
    pub fn cancel_all(&self) {
        if let Ok(map) = self.0.lock() {
            for entry in map.values() {
                entry.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}
