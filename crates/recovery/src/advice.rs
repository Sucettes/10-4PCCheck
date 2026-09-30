//! Textes d'aide affichés à côté de la source choisie (plan, section 3 « Récupération sur SSD »).

/// Où la récupération marche bien, et pourquoi pas sur un SSD.
pub const SUPPORT_HELP: &str = "La récupération fonctionne bien sur un disque dur, une clé USB \
ou une carte SD (appareil photo, téléphone) : un fichier supprimé y reste lisible tant que son \
emplacement n'a pas été réécrit. Arrête d'utiliser le support dès que possible et n'enregistre \
jamais les fichiers récupérés dessus. Sur un SSD, la commande TRIM fait effacer les blocs \
supprimés par le disque lui-même, souvent en quelques secondes : la récupération y est presque \
toujours vaine.";

/// Avertissement à afficher pour la source, ou `None` si elle n'est pas un SSD.
/// `trim_supported` vient de smartctl (`DiskInfo::trim_supported`) : inconnu derrière beaucoup
/// de ponts USB.
pub fn trim_warning(is_ssd: bool, trim_supported: Option<bool>) -> Option<String> {
    if !is_ssd {
        return None;
    }
    let text = match trim_supported {
        Some(true) => {
            "Ce disque est un SSD avec TRIM : les fichiers supprimés sont en général effacés \
             par le disque peu après la suppression. Attends-toi à ne rien récupérer, sauf des \
             fichiers supprimés il y a très peu de temps ou si TRIM était désactivé."
        }
        Some(false) => {
            "Ce disque est un SSD qui n'annonce pas TRIM : la récupération reste possible, mais \
             moins fiable que sur un disque dur (le contrôleur du SSD peut effacer des blocs \
             de lui-même)."
        }
        None => {
            "Ce disque est un SSD, TRIM inconnu (souvent le cas derrière un boîtier USB). Si \
             TRIM était actif dans l'ordinateur d'origine, la récupération sera presque nulle."
        }
    };
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_only_for_ssd() {
        assert_eq!(trim_warning(false, Some(true)), None);
        assert_eq!(trim_warning(false, None), None);
        let with_trim = trim_warning(true, Some(true)).unwrap();
        assert!(with_trim.contains("TRIM"));
        assert!(with_trim.contains("rien récupérer"));
        assert!(trim_warning(true, Some(false))
            .unwrap()
            .contains("possible"));
        assert!(trim_warning(true, None).unwrap().contains("inconnu"));
    }

    #[test]
    fn help_mentions_good_media() {
        for media in ["disque dur", "clé USB", "carte SD", "SSD"] {
            assert!(SUPPORT_HELP.contains(media), "{media}");
        }
    }
}
