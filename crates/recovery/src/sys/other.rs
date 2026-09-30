//! Autres systèmes (macOS...) : non pris en charge par le projet. Destination toujours
//! refusée (disque inconnu), aucun volume proposé.

use std::path::Path;

use crate::disk::DestinationLocation;
use crate::volumes::Volume;

pub(crate) fn locate_path(_path: &Path) -> DestinationLocation {
    DestinationLocation::Unknown {
        reason: "système non pris en charge".into(),
    }
}

pub(crate) fn list_volumes() -> Vec<Volume> {
    Vec::new()
}
