//! Plateformes non prises en charge (macOS, BSD) : le code compile, l'inventaire reste vide.

use crate::model::MachineInventory;

pub(crate) fn collect(inv: &mut MachineInventory) {
    inv.errors
        .push("Inventaire : plateforme non prise en charge (Windows et Linux seulement)".into());
}

pub(crate) fn temperature_probe() -> impl FnMut() -> Option<f64> {
    || None
}

pub(crate) fn available_memory_bytes() -> Option<u64> {
    None
}
