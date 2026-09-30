//! Commandes de l'analyse complète : inventaire, test de charge CPU, test RAM partiel.

use std::sync::Arc;
use std::time::Duration;

use pccheck_inventory::{
    available_memory_bytes, collect, run_cpu_stress, run_ram_test, MachineInventory,
};
use tauri::{AppHandle, State};

use crate::cache::Cache;
use crate::jobs::Jobs;
use crate::{blocking, CommandError};

/// Part de la mémoire disponible testée : assez pour être utile, sans faire basculer le
/// système sur le fichier d'échange.
const RAM_TEST_SHARE: f64 = 0.5;
const CPU_STRESS_DEFAULT_S: u64 = 300;

/// Inventaire de la machine. Environ 10 s sous Windows (licence) : gardé en cache, `refresh`
/// force une nouvelle lecture.
#[tauri::command]
pub async fn machine_inventory(
    refresh: bool,
    cache: State<'_, Cache>,
) -> Result<MachineInventory, CommandError> {
    if !refresh {
        if let Some(inv) = cache.lock().inventory.clone() {
            return Ok(inv);
        }
    }
    let inv = blocking(collect).await?;
    cache.lock().inventory = Some(inv.clone());
    Ok(inv)
}

/// Test de charge CPU. Identifiant de tâche : `cpu`.
#[tauri::command]
pub fn start_cpu_stress(
    seconds: Option<u64>,
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
    cache: State<'_, Cache>,
) -> Result<String, CommandError> {
    let cache = cache.inner().clone();
    let duration = Duration::from_secs(seconds.unwrap_or(CPU_STRESS_DEFAULT_S).clamp(10, 1800));
    jobs.start(&app, "cpu".into(), move |ctx| {
        let result = run_cpu_stress(duration, &ctx.cancel, |s| ctx.progress(s));
        cache.lock().stress = Some(result.clone());
        Ok::<_, ()>(result)
    })
    .map_err(CommandError::Internal)?;
    Ok("cpu".into())
}

/// Test RAM partiel sur la moitié de la mémoire disponible. Identifiant : `ram`.
#[tauri::command]
pub fn start_ram_test(
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
    cache: State<'_, Cache>,
) -> Result<String, CommandError> {
    let cache = cache.inner().clone();
    let bytes = available_memory_bytes()
        .map(|b| (b as f64 * RAM_TEST_SHARE) as u64)
        .ok_or_else(|| CommandError::Internal("mémoire disponible illisible".into()))?;
    jobs.start(&app, "ram".into(), move |ctx| {
        let result = run_ram_test(bytes, &ctx.cancel, |p| ctx.progress(&p));
        cache.lock().ram = Some(result.clone());
        Ok::<_, ()>(result)
    })
    .map_err(CommandError::Internal)?;
    Ok("ram".into())
}
