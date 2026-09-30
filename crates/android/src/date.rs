//! Date civile minimale (année, mois, jour) pour dater le correctif de sécurité.
//!
//! Pas de dépendance à `chrono` : on a seulement besoin de lire « AAAA-MM-JJ », de compter des mois
//! et d'obtenir la date du jour. Les fonctions d'évaluation reçoivent la date du jour en paramètre
//! pour rester testables ; `today()` la fournit en production.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Serialize, Serializer};

/// Date du calendrier grégorien, sérialisée en « AAAA-MM-JJ ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    // Ordre des champs important : la dérivation de `Ord` compare dans cet ordre.
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    /// `None` si le mois ou le jour n'existe pas (30 février, mois 13...).
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Lit « AAAA-MM-JJ », le format de `ro.build.version.security_patch`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().splitn(3, '-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        Date::new(year, month, day)
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }

    /// Nombre de mois entiers écoulés de `self` à `later` (négatif si `later` est avant).
    /// Du 5 mai au 4 août : 2 mois ; au 5 août : 3 mois.
    pub fn months_until(self, later: Date) -> i32 {
        let months = (later.year - self.year) * 12 + i32::from(later.month) - i32::from(self.month);
        if later.day < self.day {
            months - 1
        } else {
            months
        }
    }

    /// Date correspondant à un nombre de jours depuis le 1970-01-01.
    /// Algorithme « civil_from_days » de Howard Hinnant (calendrier grégorien proleptique).
    pub fn from_unix_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097); // jour de l'ère, 0..146096
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // jour de l'année, mars = 0
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Date { year, month, day }
    }
}

/// Date du jour en temps universel. Un décalage d'un jour avec l'heure locale est sans
/// importance : l'évaluation du correctif compte en mois.
pub fn today() -> Date {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        // Horloge avant 1970 : machine déréglée, on prend l'époque plutôt que de paniquer.
        .unwrap_or(0);
    Date::from_unix_days(days)
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, j: u8) -> Date {
        Date::new(y, m, j).unwrap()
    }

    #[test]
    fn parse_and_display() {
        assert_eq!(Date::parse("2024-05-01"), Some(d(2024, 5, 1)));
        assert_eq!(Date::parse(" 2026-08-05\n"), Some(d(2026, 8, 5)));
        assert_eq!(d(2024, 5, 1).to_string(), "2024-05-01");
        assert_eq!(Date::parse("2023-02-29"), None, "2023 n'est pas bissextile");
        assert_eq!(Date::parse("2024-02-29"), Some(d(2024, 2, 29)));
        assert_eq!(Date::parse("2024-13-01"), None);
        assert_eq!(Date::parse("2024-05"), None);
        assert_eq!(Date::parse(""), None);
    }

    #[test]
    fn months_until_counts_full_months() {
        assert_eq!(d(2026, 5, 5).months_until(d(2026, 8, 4)), 2);
        assert_eq!(d(2026, 5, 5).months_until(d(2026, 8, 5)), 3);
        assert_eq!(d(2025, 9, 1).months_until(d(2026, 9, 29)), 12);
        assert_eq!(d(2026, 10, 1).months_until(d(2026, 9, 29)), -1);
    }

    #[test]
    fn unix_days_conversion() {
        assert_eq!(Date::from_unix_days(0), d(1970, 1, 1));
        assert_eq!(Date::from_unix_days(19_782), d(2024, 2, 29));
        assert_eq!(Date::from_unix_days(20_725), d(2026, 9, 29));
        assert_eq!(Date::from_unix_days(-1), d(1969, 12, 31));
    }

    #[test]
    fn serializes_as_iso_string() {
        assert_eq!(
            serde_json::to_string(&d(2026, 8, 5)).unwrap(),
            "\"2026-08-05\""
        );
    }
}
