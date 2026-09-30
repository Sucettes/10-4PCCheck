//! Vérifications manuelles : ce qu'ADB ne peut pas mesurer. L'interface les coche et le rapport
//! enregistre le résultat de chacune par son `id` (stable : ne pas renommer).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChecklistItem {
    pub id: &'static str,
    pub label: &'static str,
    pub help: &'static str,
}

const ITEMS: [ChecklistItem; 12] = [
    ChecklistItem {
        id: "imei_blacklist",
        label: "IMEI et liste noire",
        help: "Compose *#06# : l'IMEI doit correspondre à celui de la boîte et du tiroir SIM. \
               Vérifie-le sur un service de liste noire (au Canada : devicecheck.ca) : un \
               téléphone déclaré volé peut être bloqué par les opérateurs.",
    },
    ChecklistItem {
        id: "screen",
        label: "Écran : pixels morts et marquage",
        help: "Affiche des images plein écran blanc, noir, rouge, vert et bleu, luminosité au \
               maximum. Cherche les points fixes, taches, lignes, teinte inégale et image \
               fantôme (marquage des écrans OLED).",
    },
    ChecklistItem {
        id: "touch",
        label: "Tactile sur toute la surface",
        help: "Dans une appli de dessin, ou en déplaçant une icône, glisse lentement le doigt \
               partout, bords et coins compris : aucune zone ne doit être morte.",
    },
    ChecklistItem {
        id: "buttons",
        label: "Boutons",
        help: "Marche/arrêt, volume haut et bas, et bouton latéral s'il y en a un : chaque appui \
               doit avoir un clic net et une action visible.",
    },
    ChecklistItem {
        id: "cameras",
        label: "Caméras avant et arrière",
        help: "Prends une photo et une courte vidéo avec chaque objectif (grand-angle, \
               ultra grand-angle, téléobjectif) et la caméra avant. Vérifie la mise au point, le \
               flash, et l'absence de poussière ou de buée derrière la vitre.",
    },
    ChecklistItem {
        id: "audio",
        label: "Haut-parleurs et micro",
        help: "Enregistre ta voix avec l'enregistreur puis réécoute-la. Fais un appel d'essai : \
               écouteur du haut, haut-parleur du bas et mode mains libres.",
    },
    ChecklistItem {
        id: "charging_port",
        label: "Port de charge",
        help: "Branche un chargeur : la charge doit démarrer sans avoir à tenir le câble, et le \
               câble ne doit pas avoir de jeu.",
    },
    ChecklistItem {
        id: "sim_tray",
        label: "Tiroir SIM et réseau",
        help: "Insère ta carte SIM : réseau, appel et données mobiles doivent fonctionner \
               (téléphone déverrouillé pour ton opérateur). Le tiroir doit être intact.",
    },
    ChecklistItem {
        id: "biometrics",
        label: "Empreinte et reconnaissance faciale",
        help: "Enregistre une empreinte (et ton visage si offert), puis déverrouille plusieurs \
               fois de suite.",
    },
    ChecklistItem {
        id: "wireless",
        label: "Wi-Fi et Bluetooth",
        help: "Connecte-toi à un réseau Wi-Fi et appaire un appareil Bluetooth (écouteurs, \
               montre).",
    },
    ChecklistItem {
        id: "body_battery_swelling",
        label: "État physique et batterie gonflée",
        help: "Regarde le téléphone de côté : un écran ou un dos qui se soulève signale une \
               batterie gonflée, dangereuse. Note les fissures, chocs et traces d'humidité.",
    },
    ChecklistItem {
        id: "factory_reset",
        label: "Réinitialisation devant toi et compte retiré",
        help: "Le vendeur retire son compte Google (et Samsung s'il y a lieu) puis fait la \
               réinitialisation d'usine devant toi. Au redémarrage, l'assistant de configuration \
               ne doit demander aucun compte précédent.",
    },
];

/// Liste des vérifications manuelles, dans l'ordre d'affichage.
pub fn manual_checklist() -> Vec<ChecklistItem> {
    ITEMS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_items_complete() {
        let items = manual_checklist();
        let mut ids: Vec<&str> = items.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), items.len());
        assert!(items
            .iter()
            .all(|i| !i.label.is_empty() && !i.help.is_empty()));
        assert!(items.iter().any(|i| i.help.contains("*#06#")));
    }
}
