//! Test de vitesse rapide d'un disque et âge estimé (voir `pccheck_core::speed` et `age`).

use std::io;
use std::sync::Arc;

use pccheck_core::age::{current_year, estimate_age, DiskAge};
use pccheck_core::rawio;
use pccheck_core::speed::{
    read_test, speed_scale, write_test, SpeedResult, WriteSkip, WRITE_BYTES, WRITE_SIZES_GIB,
};
use pccheck_core::DiskInfo;
use tauri::{AppHandle, State};

use crate::cache::Cache;
use crate::jobs::Jobs;
use crate::CommandError;

fn disk_info(cache: &Cache, device: &str) -> Option<DiskInfo> {
    cache
        .lock()
        .disks
        .iter()
        .find_map(|e| e.info.clone().filter(|i| i.device.name == device))
}

/// Test de vitesse (lecture, temps d'accès, écriture dans l'espace libre). Identifiant :
/// `speed:<chemin smartctl>`. Environ une minute avec 1 Gio écrit. `write_gib` : 1, 5 ou 10
/// (1 par défaut, analyse complète).
#[tauri::command]
pub fn start_speed_test(
    device: String,
    write_gib: Option<u64>,
    app: AppHandle,
    jobs: State<'_, Arc<Jobs>>,
    cache: State<'_, Cache>,
) -> Result<String, CommandError> {
    let info = disk_info(&cache, &device)
        .ok_or_else(|| CommandError::Tool("disque inconnu : actualise la liste".into()))?;
    let total = info
        .capacity_bytes
        .ok_or_else(|| CommandError::Tool("capacité du disque inconnue".into()))?;
    let path = rawio::raw_device_path(&device).ok_or_else(|| {
        CommandError::Tool(format!(
            "ce disque ne peut pas être lu directement ({device})"
        ))
    })?;
    let write_bytes = match write_gib {
        None => WRITE_BYTES,
        Some(g) if WRITE_SIZES_GIB.contains(&g) => g << 30,
        Some(g) => {
            return Err(CommandError::Tool(format!(
                "taille d'écriture non prévue : {g} Go"
            )))
        }
    };
    let id = format!("speed:{device}");
    let cache = cache.inner().clone();
    jobs.start(&app, id.clone(), move |ctx| {
        let ssd = matches!(info.media, pccheck_core::MediaKind::Ssd)
            || info.protocol == pccheck_core::Protocol::Nvme;
        let (read, read_error) = match read_test(
            || rawio::open_device_read(&path),
            total,
            ssd,
            // Taille choisie : autant de lecture directe que d'écriture.
            write_gib.map(|_| write_bytes),
            &ctx.cancel,
            |p| ctx.progress(p),
        ) {
            Ok(r) => (Some(r), None),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => (
                None,
                Some("accès refusé : relance l'outil en administrateur".to_string()),
            ),
            Err(e) => (None, Some(format!("lecture impossible : {e}"))),
        };

        // Écriture : volume de ce disque avec le plus d'espace libre (volumes en lecture seule
        // exclus par la liste).
        let disk = pccheck_recovery::disk_from_smartctl_name(&info.device.name).ok();
        let volume = pccheck_recovery::list_volumes()
            .into_iter()
            .filter(|v| disk.is_some() && v.disk == disk)
            .max_by_key(|v| v.free_bytes);
        let (write, write_skipped) = if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            (None, Some(WriteSkip::Cancelled.to_string()))
        } else {
            match volume {
                None => (None, Some(WriteSkip::NoVolume.to_string())),
                Some(v) => match write_test(&v.path, write_bytes, &ctx.cancel, |p| ctx.progress(p))
                {
                    Ok(w) => (Some(w), None),
                    Err(e) => (None, Some(e.to_string())),
                },
            }
        };
        let result = SpeedResult {
            read,
            read_error,
            write,
            write_skipped,
            cancelled: ctx.cancel.load(std::sync::atomic::Ordering::Relaxed),
            scale: speed_scale(&info),
        };
        cache
            .lock()
            .speed
            .insert(info.device.name.clone(), result.clone());
        Ok::<_, ()>(result)
    })
    .map_err(CommandError::Internal)?;
    Ok(id)
}

/// Année imprimée sur l'étiquette du disque (`None` pour l'effacer).
#[tauri::command]
pub fn set_disk_year(
    device: String,
    year: Option<u16>,
    cache: State<'_, Cache>,
) -> Result<DiskAge, CommandError> {
    let now = current_year();
    if let Some(y) = year {
        if !(1980..=now).contains(&y) {
            return Err(CommandError::Tool(format!(
                "année invalide : entre 1980 et {now}"
            )));
        }
    }
    {
        let mut c = cache.lock();
        match year {
            Some(y) => c.label_years.insert(device.clone(), y),
            None => c.label_years.remove(&device),
        };
    }
    disk_age(device, cache)
}

/// Âge estimé et heures d'utilisation d'un disque.
#[tauri::command]
pub fn disk_age(device: String, cache: State<'_, Cache>) -> Result<DiskAge, CommandError> {
    let info = disk_info(&cache, &device)
        .ok_or_else(|| CommandError::Tool("disque inconnu : actualise la liste".into()))?;
    let year = cache.lock().label_years.get(&device).copied();
    Ok(estimate_age(&info, year, current_year()))
}

/// Dernier résultat du test de vitesse d'un disque (après un changement d'écran).
#[tauri::command]
pub fn speed_result(device: String, cache: State<'_, Cache>) -> Option<SpeedResult> {
    cache.lock().speed.get(&device).cloned()
}
