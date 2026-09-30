//! Lecture des attributs SMART ATA : libellé français, état de chaque attribut, volumes écrits et lus.
//!
//! Les ID des attributs 170 à 255 n'ont pas le même sens d'un fabricant à l'autre. smartctl résout ce
//! sens avec sa base de disques et l'exprime dans le nom (`Wear_Leveling_Count`, `Host_Writes_GiB`...).
//! On se fie donc au nom de smartctl, pas à l'ID, sauf pour les compteurs normalisés par la norme ATA.

use serde::Serialize;

use crate::disk::AtaAttribute;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeStatus {
    Ok,
    /// À surveiller : compteur d'erreurs non nul, ou attribut passé sous son seuil par le passé.
    Watch,
    /// Sous le seuil du fabricant en ce moment.
    Failing,
}

/// Compteurs dont toute valeur brute non nulle signale un problème physique, avec le même sens
/// chez tous les fabricants : 5 secteurs réalloués, 187 erreurs non corrigibles signalées,
/// 196 événements de réallocation, 197 secteurs en attente, 198 non corrigibles hors ligne,
/// 199 erreurs CRC du lien (câble ou connecteur).
const PROBLEM_COUNTERS: [u8; 6] = [5, 187, 196, 197, 198, 199];

pub(crate) fn status(id: u8, when_failed: Option<&str>, raw_value: u64) -> AttributeStatus {
    match when_failed {
        Some("now") => AttributeStatus::Failing,
        Some(_) => AttributeStatus::Watch,
        None if PROBLEM_COUNTERS.contains(&id) && raw_value > 0 => AttributeStatus::Watch,
        None => AttributeStatus::Ok,
    }
}

/// Libellé français d'un attribut, selon le nom donné par smartctl. `None` si inconnu : l'interface
/// affiche alors le nom anglais.
pub(crate) fn label_fr(name: &str) -> Option<&'static str> {
    Some(match name {
        "Raw_Read_Error_Rate" => "Taux d'erreurs de lecture",
        "Throughput_Performance" => "Performance de débit",
        "Spin_Up_Time" => "Temps de démarrage du moteur",
        "Start_Stop_Count" => "Démarrages et arrêts du moteur",
        "Reallocated_Sector_Ct" => "Secteurs réalloués",
        "Seek_Error_Rate" => "Taux d'erreurs de positionnement",
        "Seek_Time_Performance" => "Performance de positionnement",
        "Power_On_Hours" => "Heures de fonctionnement",
        "Spin_Retry_Count" => "Relances du moteur",
        "Calibration_Retry_Count" => "Relances de calibrage",
        "Power_Cycle_Count" => "Nombre de démarrages",
        "Wear_Leveling_Count" => "Usure (répartition)",
        "Used_Rsvd_Blk_Cnt_Tot" => "Blocs de réserve utilisés",
        "Program_Fail_Cnt_Total" | "Program_Fail_Count" => "Échecs de programmation",
        "Erase_Fail_Count_Total" | "Erase_Fail_Count" => "Échecs d'effacement",
        "Runtime_Bad_Block" => "Blocs défectueux en service",
        "Reported_Uncorrect" | "Uncorrectable_Error_Cnt" => "Erreurs non corrigibles",
        "Command_Timeout" => "Commandes expirées",
        "High_Fly_Writes" => "Écritures tête trop haute",
        "Airflow_Temperature_Cel" => "Température (flux d'air)",
        "Temperature_Celsius" => "Température",
        "G-Sense_Error_Rate" => "Chocs détectés",
        "Power-Off_Retract_Count" => "Rétractions à la coupure",
        "Load_Cycle_Count" => "Cycles de chargement des têtes",
        "Hardware_ECC_Recovered" => "Erreurs corrigées par ECC",
        "ECC_Error_Rate" => "Taux d'erreurs ECC",
        "Reallocated_Event_Count" => "Événements de réallocation",
        "Current_Pending_Sector" => "Secteurs en attente",
        "Offline_Uncorrectable" => "Secteurs non corrigibles",
        "UDMA_CRC_Error_Count" | "CRC_Error_Count" => "Erreurs CRC (câble)",
        "Multi_Zone_Error_Rate" => "Taux d'erreurs d'écriture",
        "POR_Recovery_Count" | "Unexpect_Power_Loss_Ct" | "Unsafe_Shutdown_Count" => {
            "Coupures de courant brutales"
        }
        "Head_Flying_Hours" => "Heures de vol des têtes",
        "End-to-End_Error" => "Erreurs de bout en bout",
        "Available_Reservd_Space" => "Espace de réserve disponible",
        "Percent_Lifetime_Remain" | "SSD_Life_Left" => "Vie restante",
        "Media_Wearout_Indicator" => "Indicateur d'usure",
        "Total_LBAs_Written"
        | "Host_Writes_GiB"
        | "Lifetime_Writes_GiB"
        | "Total_Writes_GiB"
        | "Host_Writes_32MiB" => "Données écrites",
        "Total_LBAs_Read" | "Host_Reads_GiB" | "Lifetime_Reads_GiB" | "Total_Reads_GiB"
        | "Host_Reads_32MiB" => "Données lues",
        _ => return None,
    })
}

const GIB: u64 = 1 << 30;
const MIB_32: u64 = 32 << 20;

/// Octets écrits, depuis l'attribut dont le nom donne l'unité. `block_size` : taille d'un LBA.
pub(crate) fn bytes_written(attrs: &[AtaAttribute], block_size: u64) -> Option<u64> {
    bytes_from(
        attrs,
        &[
            ("Total_LBAs_Written", block_size),
            ("Host_Writes_GiB", GIB),
            ("Lifetime_Writes_GiB", GIB),
            ("Total_Writes_GiB", GIB),
            ("Host_Writes_32MiB", MIB_32),
        ],
    )
}

pub(crate) fn bytes_read(attrs: &[AtaAttribute], block_size: u64) -> Option<u64> {
    bytes_from(
        attrs,
        &[
            ("Total_LBAs_Read", block_size),
            ("Host_Reads_GiB", GIB),
            ("Lifetime_Reads_GiB", GIB),
            ("Total_Reads_GiB", GIB),
            ("Host_Reads_32MiB", MIB_32),
        ],
    )
}

fn bytes_from(attrs: &[AtaAttribute], units: &[(&str, u64)]) -> Option<u64> {
    units.iter().find_map(|(name, unit)| {
        attrs
            .iter()
            .find(|a| a.name == *name)
            .and_then(|a| a.raw_value.checked_mul(*unit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_levels() {
        assert_eq!(status(5, Some("now"), 3912), AttributeStatus::Failing);
        assert_eq!(status(9, Some("past"), 0), AttributeStatus::Watch);
        assert_eq!(status(197, None, 57), AttributeStatus::Watch);
        assert_eq!(status(197, None, 0), AttributeStatus::Ok);
        // 9 (heures) : une valeur brute élevée est normale.
        assert_eq!(status(9, None, 42_611), AttributeStatus::Ok);
    }

    #[test]
    fn unknown_names_have_no_label() {
        assert_eq!(label_fr("Unknown_Attribute"), None);
        assert_eq!(
            label_fr("Reallocated_Sector_Ct"),
            Some("Secteurs réalloués")
        );
    }
}
