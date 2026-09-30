//! Fonctionnalités d'un disque ATA/SATA (NCQ, TRIM, AAM, APM, GPL, DevSleep, caches), lues dans
//! ses données d'identification (commande IDENTIFY DEVICE, normes ATA8-ACS à ACS-4).
//!
//! smartctl ne les donne pas en JSON : on lit la sortie texte de `smartctl --identify`, qui
//! n'affiche que les bits à 1, une ligne par bit :
//! `  76      8          1   NCQ feature set supported` (mot, bit, valeur, description).
//! Une ligne absente signifie donc « non pris en charge » (ou « désactivé »).

use std::collections::HashSet;

use serde::Serialize;

/// Une fonctionnalité : prise en charge par le disque, et activée quand la norme le précise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtaFeature {
    /// Identifiant stable (`ncq`, `trim`...).
    pub key: &'static str,
    /// Nom affiché, celui qu'utilisent aussi CrystalDiskInfo et les fiches techniques.
    pub label: &'static str,
    pub supported: bool,
    /// `None` : la norme n'a pas de bit « activé » pour cette fonctionnalité.
    pub enabled: Option<bool>,
}

/// Emplacement d'une fonctionnalité dans les données d'identification : (mot, bit).
struct FeatureBits {
    key: &'static str,
    label: &'static str,
    supported: (u16, u8),
    /// Bit « activé », quand la norme en prévoit un.
    enabled: Option<(u16, u8)>,
}

const fn bits(
    key: &'static str,
    label: &'static str,
    supported: (u16, u8),
    enabled: Option<(u16, u8)>,
) -> FeatureBits {
    FeatureBits {
        key,
        label,
        supported,
        enabled,
    }
}

const FEATURES: [FeatureBits; 9] = [
    bits("smart", "S.M.A.R.T.", (82, 0), Some((85, 0))),
    bits("ncq", "NCQ", (76, 8), None),
    bits("trim", "TRIM", (169, 0), None),
    bits("gpl", "GPL", (84, 5), None),
    bits("apm", "APM", (83, 3), Some((86, 3))),
    bits("aam", "AAM", (83, 9), Some((86, 9))),
    bits("devsleep", "DevSleep", (78, 8), Some((79, 8))),
    bits("write_cache", "Cache d'écriture", (82, 5), Some((85, 5))),
    bits(
        "read_lookahead",
        "Lecture anticipée",
        (82, 6),
        Some((85, 6)),
    ),
];

/// Fonctionnalités lues dans la sortie de `smartctl --identify`. `None` si la sortie n'est pas un
/// tableau d'identification (commande refusée, pont USB qui ne la transmet pas) : on ne sait
/// alors rien, ce qui n'est pas la même chose que « rien n'est pris en charge ».
pub fn parse_identify(text: &str) -> Option<Vec<AtaFeature>> {
    if !text.lines().any(|l| l.trim_start().starts_with("Word")) {
        return None;
    }
    let set: HashSet<(u16, u8)> = text
        .lines()
        .filter_map(|line| {
            let mut t = line.split_whitespace();
            let word = t.next()?.parse::<u16>().ok()?;
            // Bit seul (« 8 »), pas un champ de plusieurs bits (« 15:8 ») ni un mot entier (« - »).
            let bit = t.next()?.parse::<u8>().ok()?;
            (t.next()? == "1").then_some((word, bit))
        })
        .collect();
    let features = FEATURES
        .iter()
        .map(|f| {
            let supported = set.contains(&f.supported);
            AtaFeature {
                key: f.key,
                label: f.label,
                supported,
                enabled: f.enabled.filter(|_| supported).map(|e| set.contains(&e)),
            }
        })
        .collect();
    Some(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reconstruit d'après le format de smartmontools (ataidentify.cpp) : seuls les bits à 1.
    const IDENTIFY: &str = "\
smartctl 7.5 2025-04-30 r5714 [x86_64-linux-6.1.0] (local build)
=== START OF INFORMATION SECTION ===
Word     Bit    Value   Description
   0      -    0x0040   General configuration
   0      6          1   Not removable controller and/or device [OBS-6]
  76      -    0x0f0e   Serial ATA capabilities
  76      8          1   NCQ feature set supported
  76      3          1   SATA Gen3 signaling speed (6.0 Gb/s) supported
  82      -    0x746b   Commands and feature sets supported
  82      6          1   Read look-ahead supported
  82      5          1   Volatile write cache supported
  82      0          1   SMART feature set supported
  83      3          1   APM feature set supported
  84      5          1   GPL feature set supported
  85      6          1   Read look-ahead enabled
  85      5          1   Write cache enabled
  85      0          1   SMART feature set enabled
  94     15:8     0x80   Recommended AAM level [OBS-ACS-2]
 169      0          1   Trim bit in DATA SET MANAGEMENT command supported
";

    fn feature<'a>(f: &'a [AtaFeature], key: &str) -> &'a AtaFeature {
        f.iter().find(|x| x.key == key).unwrap()
    }

    #[test]
    fn supported_and_enabled_bits_are_read() {
        let f = parse_identify(IDENTIFY).unwrap();
        assert!(feature(&f, "ncq").supported);
        assert!(feature(&f, "trim").supported);
        assert!(feature(&f, "gpl").supported);
        assert_eq!(feature(&f, "smart").enabled, Some(true));
        assert_eq!(feature(&f, "write_cache").enabled, Some(true));
        // APM pris en charge mais désactivé (86.3 absent).
        let apm = feature(&f, "apm");
        assert!(apm.supported);
        assert_eq!(apm.enabled, Some(false));
        // AAM : le niveau recommandé (champ de plusieurs bits) ne vaut pas « pris en charge ».
        let aam = feature(&f, "aam");
        assert!(!aam.supported);
        assert_eq!(aam.enabled, None);
        assert!(!feature(&f, "devsleep").supported);
    }

    #[test]
    fn unreadable_output_means_unknown_not_unsupported() {
        assert_eq!(parse_identify(""), None);
        assert_eq!(
            parse_identify("Read Device Identity failed: Unknown usb device id"),
            None
        );
    }
}
