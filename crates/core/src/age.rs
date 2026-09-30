//! Âge d'un disque et intensité de son usage.
//!
//! - Heures d'utilisation : comptées par le disque lui-même (fiable). Repères pour un disque dur,
//!   d'après les statistiques publiques de Backblaze : la mécanique s'use surtout au-delà de
//!   20 000 h, et la fin de vie devient probable au-delà de 40 000 h. Sur un SSD, les heures
//!   comptent peu : c'est l'usure des cellules (vie restante) qui fait foi.
//! - Âge réel : un disque n'enregistre presque jamais sa date de fabrication. Deux sources :
//!   l'année imprimée sur l'étiquette (saisie par l'utilisateur, exacte) ou l'année de sortie
//!   du modèle (table ci-dessous, donc un âge MAXIMAL : le disque a pu être fabriqué plus tard).
//! - Intensité : heures d'utilisation divisées par l'âge. 24 h/24 pendant 10 ans n'use pas
//!   comme 2 h par jour.

use serde::Serialize;

use crate::disk::{DiskInfo, MediaKind};

/// Heures au-delà desquelles un disque dur est usé, puis en fin de vie probable.
pub const HDD_HOURS_WORN: u64 = 20_000;
pub const HDD_HOURS_END: u64 = 40_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoursRating {
    /// Moins de 20 000 h.
    Young,
    /// 20 000 à 40 000 h.
    Worn,
    /// Plus de 40 000 h.
    EndOfLife,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiskAge {
    pub power_on_hours: Option<u64>,
    /// Disques durs seulement.
    pub hours_rating: Option<HoursRating>,
    /// Année lue sur l'étiquette par l'utilisateur.
    pub label_year: Option<u16>,
    /// Année de sortie du modèle, si connue.
    pub model_year: Option<u16>,
    /// Âge en années (milieu de l'année de fabrication).
    pub age_years: Option<f64>,
    /// `true` si l'âge vient de l'année du modèle : c'est un maximum.
    pub age_is_maximum: bool,
    /// Heures d'utilisation par jour en moyenne (un minimum si l'âge est un maximum).
    pub hours_per_day: Option<f64>,
}

pub fn hours_rating(media: &MediaKind, hours: u64) -> Option<HoursRating> {
    match media {
        MediaKind::Hdd { .. } => Some(if hours >= HDD_HOURS_END {
            HoursRating::EndOfLife
        } else if hours >= HDD_HOURS_WORN {
            HoursRating::Worn
        } else {
            HoursRating::Young
        }),
        _ => None,
    }
}

/// Âge et intensité d'usage. `current_year` : année en cours (paramètre pour les tests).
pub fn estimate_age(info: &DiskInfo, label_year: Option<u16>, current_year: u16) -> DiskAge {
    let model_year = info.model.as_deref().and_then(model_release_year);
    let (year, is_max) = match (label_year, model_year) {
        (Some(y), _) => (Some(y), false),
        (None, Some(y)) => (Some(y), true),
        (None, None) => (None, false),
    };
    // Milieu de l'année de fabrication ; au moins six mois (disque de l'année en cours).
    let age_years = year
        .filter(|y| *y <= current_year)
        .map(|y| f64::from(current_year - y) + 0.5);
    let hours_per_day = match (info.power_on_hours, age_years) {
        (Some(h), Some(a)) if a > 0.0 => {
            let per_day = h as f64 / (a * 365.25);
            // Plus de 24 h par jour : l'année saisie est fausse (ou le compteur).
            (per_day <= 24.5).then_some(per_day.min(24.0))
        }
        _ => None,
    };
    DiskAge {
        power_on_hours: info.power_on_hours,
        hours_rating: info
            .power_on_hours
            .and_then(|h| hours_rating(&info.media, h)),
        label_year,
        model_year,
        age_years,
        age_is_maximum: is_max && label_year.is_none(),
        hours_per_day,
    }
}

/// Année de sortie de familles de disques courantes (préfixe du modèle, sans tenir compte de la
/// casse). Volontairement courte : seules des années sûres, arrondies à l'année de lancement.
const MODEL_YEARS: &[(&str, u16)] = &[
    // Seagate Barracuda 7200.11, 7200.12, 7200.14, puis la génération suivante.
    ("ST31500341AS", 2008),
    ("ST31000340AS", 2008),
    ("ST3500320AS", 2008),
    ("ST3750330AS", 2008),
    ("ST3500418AS", 2009),
    ("ST31000528AS", 2009),
    ("ST3250318AS", 2009),
    ("ST1000DM003", 2011),
    ("ST2000DM001", 2011),
    ("ST3000DM001", 2011),
    ("ST500DM002", 2011),
    ("ST1000DM010", 2016),
    ("ST2000DM008", 2017),
    ("ST1000LM035", 2016),
    ("ST2000LM015", 2016),
    // Western Digital.
    ("WDC WD5000AAKS", 2007),
    ("WDC WD5000AAKX", 2010),
    ("WDC WD10EARS", 2010),
    ("WDC WD20EARS", 2010),
    ("WDC WD10EALX", 2010),
    ("WDC WD20EARX", 2011),
    ("WDC WD10EZEX", 2013),
    ("WDC WD10JPVX", 2013),
    ("WDC WD10EZRZ", 2015),
    ("WDC WD20EZRZ", 2015),
    ("WDC WD10SPZX", 2016),
    // Toshiba.
    ("TOSHIBA DT01ACA", 2012),
    // SSD Samsung.
    ("Samsung SSD 840 PRO", 2012),
    ("Samsung SSD 840 EVO", 2013),
    ("Samsung SSD 850 PRO", 2014),
    ("Samsung SSD 850 EVO", 2014),
    ("Samsung SSD 860 EVO", 2018),
    ("Samsung SSD 860 QVO", 2018),
    ("Samsung SSD 870 QVO", 2020),
    ("Samsung SSD 870 EVO", 2021),
    ("Samsung SSD 970 EVO Plus", 2019),
    ("Samsung SSD 970 EVO", 2018),
    ("Samsung SSD 980 PRO", 2020),
    ("Samsung SSD 990 PRO", 2022),
    // SSD Crucial et Kingston.
    ("M4-CT", 2011),
    ("Crucial_CT", 2014),
    ("CT250MX500", 2018),
    ("CT500MX500", 2018),
    ("CT1000MX500", 2018),
    ("CT2000MX500", 2018),
    ("CT240BX500", 2018),
    ("CT480BX500", 2018),
    ("KINGSTON SA400", 2017),
];

pub fn model_release_year(model: &str) -> Option<u16> {
    let m = model.trim().to_ascii_uppercase();
    MODEL_YEARS
        .iter()
        // Le préfixe le plus long l'emporte (« 970 EVO Plus » avant « 970 EVO »).
        .filter(|(prefix, _)| m.starts_with(&prefix.to_ascii_uppercase()))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, year)| *year)
}

/// Année en cours d'après l'horloge du système (calendrier grégorien, sans dépendance).
pub fn current_year() -> u16 {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    // Algorithme « civil_from_days » (H. Hinnant), réduit à l'année.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    u16::try_from(year).unwrap_or(2026)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{Protocol, ScanDevice};

    fn disk(model: &str, media: MediaKind, hours: u64) -> DiskInfo {
        DiskInfo {
            device: ScanDevice {
                name: "/dev/sda".into(),
                info_name: "/dev/sda".into(),
                dev_type: String::new(),
                protocol: String::new(),
            },
            model: Some(model.into()),
            serial: None,
            firmware: None,
            capacity_bytes: None,
            protocol: Protocol::Ata,
            media,
            standard: None,
            sata_version: None,
            link_speed: None,
            form_factor: None,
            trim_supported: None,
            bytes_written: None,
            bytes_read: None,
            smart_available: None,
            smart_enabled: None,
            smart_passed: None,
            temperature_c: None,
            power_on_hours: Some(hours),
            power_cycles: None,
            ata_attributes: Vec::new(),
            nvme_health: None,
            life_remaining_pct: None,
            exit_status: 0,
            warnings: Vec::new(),
            checks: Vec::new(),
        }
    }

    #[test]
    fn friend_disk_is_worn_and_about_four_hours_a_day() {
        let d = disk("ST31500341AS", MediaKind::Hdd { rpm: 7200 }, 22_622);
        let a = estimate_age(&d, None, 2026);
        assert_eq!(a.hours_rating, Some(HoursRating::Worn));
        assert_eq!(a.model_year, Some(2008));
        assert!(a.age_is_maximum);
        assert!((a.age_years.unwrap() - 18.5).abs() < 1e-9);
        let per_day = a.hours_per_day.unwrap();
        assert!((3.0..4.0).contains(&per_day), "{per_day}");

        // L'étiquette fait foi et l'âge n'est plus un maximum.
        let a = estimate_age(&d, Some(2010), 2026);
        assert!(!a.age_is_maximum);
        assert!((a.age_years.unwrap() - 16.5).abs() < 1e-9);
    }

    #[test]
    fn hours_ratings_only_for_hard_drives() {
        let hdd = MediaKind::Hdd { rpm: 5400 };
        assert_eq!(hours_rating(&hdd, 5_000), Some(HoursRating::Young));
        assert_eq!(hours_rating(&hdd, 20_000), Some(HoursRating::Worn));
        assert_eq!(hours_rating(&hdd, 45_000), Some(HoursRating::EndOfLife));
        assert_eq!(hours_rating(&MediaKind::Ssd, 45_000), None);
    }

    #[test]
    fn impossible_year_gives_no_intensity() {
        // 30 000 h en 1 an : plus de 24 h par jour, l'année saisie est fausse.
        let d = disk("X", MediaKind::Hdd { rpm: 7200 }, 30_000);
        assert_eq!(estimate_age(&d, Some(2026), 2026).hours_per_day, None);
    }

    #[test]
    fn longest_model_prefix_wins() {
        assert_eq!(
            model_release_year("Samsung SSD 970 EVO Plus 1TB"),
            Some(2019)
        );
        assert_eq!(model_release_year("Samsung SSD 970 EVO 500GB"), Some(2018));
        assert_eq!(model_release_year("wdc wd10ezex-08wn4a0"), Some(2013));
        assert_eq!(model_release_year("Lexar SSD NM790 2TB"), None);
    }

    #[test]
    fn current_year_is_plausible() {
        assert!((2026..2100).contains(&current_year()));
    }
}
