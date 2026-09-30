//! Petits formatages partagés par le HTML, le PDF et le stockage.

use chrono::{DateTime, Datelike, FixedOffset, Timelike};

const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// « 29 septembre 2026 à 14 h 30 », dans le fuseau enregistré dans le rapport (celui de la
/// machine au moment de l'analyse), pas celui de la machine qui relit le rapport.
pub fn date_fr(d: &DateTime<FixedOffset>) -> String {
    let month = MONTHS.get(d.month0() as usize).copied().unwrap_or_default();
    let day = if d.day() == 1 {
        "1er".to_string()
    } else {
        d.day().to_string()
    };
    format!(
        "{day} {month} {} à {} h {:02}",
        d.year(),
        d.hour(),
        d.minute()
    )
}

/// Nom de fichier sûr depuis un titre : minuscules ASCII, accents français retirés, tout le
/// reste remplacé par des tirets, 50 caractères au plus. Jamais vide.
pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut dash = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        let mapped: &str = match c {
            'a'..='z' | '0'..='9' => {
                out.push(c);
                dash = false;
                continue;
            }
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => "a",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'î' | 'ï' | 'í' | 'ì' => "i",
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => "o",
            'ù' | 'û' | 'ü' | 'ú' => "u",
            'ÿ' | 'ý' => "y",
            'ç' => "c",
            'ñ' => "n",
            'œ' => "oe",
            'æ' => "ae",
            _ => {
                if !dash && !out.is_empty() {
                    out.push('-');
                    dash = true;
                }
                continue;
            }
        };
        out.push_str(mapped);
        dash = false;
    }
    // Tronque sur une limite de caractère (tout est ASCII ici), puis retire le tiret final.
    out.truncate(50);
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "rapport".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_date() {
        let d = DateTime::parse_from_rfc3339("2026-09-01T08:05:00-04:00").unwrap();
        assert_eq!(date_fr(&d), "1er septembre 2026 à 8 h 05");
        let d = DateTime::parse_from_rfc3339("2026-12-29T20:30:59+01:00").unwrap();
        assert_eq!(date_fr(&d), "29 décembre 2026 à 20 h 30");
    }

    #[test]
    fn slugs() {
        assert_eq!(
            slug("Portable Lenovo — Élève d'œuvre"),
            "portable-lenovo-eleve-d-oeuvre"
        );
        assert_eq!(slug("  ##  "), "rapport");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("C:\\Windows\\x"), "c-windows-x");
        assert!(slug(&"a".repeat(200)).len() <= 50);
        assert!(!slug(&format!("{}-b", "a".repeat(49))).ends_with('-'));
    }
}
