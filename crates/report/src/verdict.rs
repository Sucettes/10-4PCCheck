//! Règles du verdict (plan, section 4). Les seuils sont une configuration sérialisable, pas des
//! constantes de l'interface : on pourra les ajuster (ou les charger d'un fichier) sans toucher
//! au code des écrans. Les valeurs par défaut sont celles du plan, encore « à valider ».

use serde::{Deserialize, Serialize};

use crate::model::{Level, Section, Verdict};

/// Seuils du verdict. Convention des bornes : un seuil « min » est inclus dans la meilleure
/// classe (santé ≥ 90 % → vert), un seuil « max » est inclus dans la classe jaune
/// (température ≤ 60 °C → jaune, au-delà → rouge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    /// Vie restante d'un SSD : vert à partir de cette valeur (%).
    pub ssd_life_ok_min: u8,
    /// Vie restante d'un SSD : jaune à partir de cette valeur, rouge en dessous (%).
    pub ssd_life_warn_min: u8,
    /// Secteurs réalloués : jaune de 1 à cette valeur, rouge au-delà (ou si le compteur monte).
    pub reallocated_warn_max: u64,
    /// Température disque au repos : vert en dessous de cette valeur (°C).
    pub disk_temp_ok_below: i64,
    /// Température disque au repos : jaune jusqu'à cette valeur incluse, rouge au-delà (°C).
    pub disk_temp_warn_max: i64,
    /// Santé batterie (capacité actuelle / d'origine) : vert à partir de cette valeur (%).
    pub battery_ok_min: f64,
    /// Santé batterie : jaune à partir de cette valeur, rouge en dessous (%).
    pub battery_warn_min: f64,
    /// Âge du correctif de sécurité Android : vert en dessous (mois).
    pub android_patch_ok_below_months: u32,
    /// Âge du correctif Android : jaune jusqu'à cette valeur incluse, rouge au-delà (mois).
    pub android_patch_warn_max_months: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            ssd_life_ok_min: 90,
            ssd_life_warn_min: 70,
            reallocated_warn_max: 50,
            disk_temp_ok_below: 50,
            disk_temp_warn_max: 60,
            battery_ok_min: 80.0,
            battery_warn_min: 60.0,
            android_patch_ok_below_months: 3,
            android_patch_warn_max_months: 12,
        }
    }
}

/// Bridage thermique du CPU pendant le test de charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Throttling {
    None,
    Brief,
    Sustained,
}

impl Thresholds {
    /// Vie restante d'un SSD en % (100 = neuf).
    pub fn ssd_life(&self, pct: u8) -> Level {
        if pct >= self.ssd_life_ok_min {
            Level::Ok
        } else if pct >= self.ssd_life_warn_min {
            Level::Warn
        } else {
            Level::Bad
        }
    }

    /// Secteurs réalloués. `increasing` : le compteur a monté depuis une mesure précédente,
    /// signe d'une surface qui se dégrade en ce moment, rouge quel que soit le nombre.
    pub fn reallocated_sectors(&self, count: u64, increasing: bool) -> Level {
        if increasing || count > self.reallocated_warn_max {
            Level::Bad
        } else if count > 0 {
            Level::Warn
        } else {
            Level::Ok
        }
    }

    /// Secteurs en attente de réallocation ou non corrigibles : le moindre est critique.
    pub fn pending_sectors(&self, count: u64) -> Level {
        if count > 0 {
            Level::Bad
        } else {
            Level::Ok
        }
    }

    /// Température d'un disque au repos (°C).
    pub fn disk_temperature(&self, celsius: i64) -> Level {
        if celsius < self.disk_temp_ok_below {
            Level::Ok
        } else if celsius <= self.disk_temp_warn_max {
            Level::Warn
        } else {
            Level::Bad
        }
    }

    /// Santé de la batterie en % de la capacité d'origine. Peut dépasser 100 sur une batterie
    /// neuve. Une valeur non finie (division par une capacité d'origine nulle) est « non évaluée ».
    pub fn battery_health(&self, pct: f64) -> Level {
        if !pct.is_finite() {
            Level::Neutral
        } else if pct >= self.battery_ok_min {
            Level::Ok
        } else if pct >= self.battery_warn_min {
            Level::Warn
        } else {
            Level::Bad
        }
    }

    pub fn cpu_throttling(&self, throttling: Throttling) -> Level {
        match throttling {
            Throttling::None => Level::Ok,
            Throttling::Brief => Level::Warn,
            Throttling::Sustained => Level::Bad,
        }
    }

    /// Gestion d'entreprise (Intune, Autopilot, Device Owner Android) : l'appareil peut être
    /// verrouillé à distance par l'ancienne organisation.
    pub fn enterprise_management(&self, present: bool) -> Level {
        if present {
            Level::Bad
        } else {
            Level::Ok
        }
    }

    /// Âge du correctif de sécurité Android, en mois entiers.
    pub fn android_patch_age(&self, months: u32) -> Level {
        if months < self.android_patch_ok_below_months {
            Level::Ok
        } else if months <= self.android_patch_warn_max_months {
            Level::Warn
        } else {
            Level::Bad
        }
    }

    /// Compte Google connecté : à retirer devant l'acheteur (protection contre la
    /// réinitialisation), donc jaune, jamais rouge.
    pub fn google_account(&self, signed_in: bool) -> Level {
        if signed_in {
            Level::Warn
        } else {
            Level::Ok
        }
    }
}

/// Nombre maximal d'éléments nommés dans le résumé avant « et N autre(s) ».
const SUMMARY_MAX_NAMES: usize = 4;

/// Verdict global : rouge si un élément est rouge, jaune si au moins un est jaune, sinon vert.
/// Seuls les états `ok`, `warn` et `bad` comptent ; `info` et `neutral` sont descriptifs.
/// Sans aucune mesure évaluée, le verdict est `neutral` (« Sans verdict ») plutôt qu'un vert
/// qui laisserait croire que tout a été vérifié.
pub fn compute_verdict(sections: &[Section]) -> Verdict {
    let mut ok = 0u32;
    let mut warn_names: Vec<String> = Vec::new();
    let mut bad_names: Vec<String> = Vec::new();
    let (mut warn, mut bad) = (0u32, 0u32);
    // Libellé présent dans plusieurs sections (« Secteurs en attente » de deux disques) : le
    // résumé précise la section, sinon on ne saurait pas quel disque est en cause.
    let ambiguous = |label: &str| {
        sections
            .iter()
            .filter(|s| s.items.iter().any(|i| i.label.trim() == label.trim()))
            .count()
            > 1
    };
    let name = |section: &Section, label: &str| {
        if ambiguous(label) {
            format!("{} ({})", label.trim(), section.title)
        } else {
            label.trim().to_string()
        }
    };
    for (section, item) in sections
        .iter()
        .flat_map(|s| s.items.iter().map(move |i| (s, i)))
    {
        match item.level {
            Level::Ok => ok += 1,
            Level::Warn => {
                warn += 1;
                push_unique(&mut warn_names, name(section, &item.label));
            }
            Level::Bad => {
                bad += 1;
                push_unique(&mut bad_names, name(section, &item.label));
            }
            Level::Info | Level::Neutral => {}
        }
    }
    let level = if bad > 0 {
        Level::Bad
    } else if warn > 0 {
        Level::Warn
    } else if ok > 0 {
        Level::Ok
    } else {
        Level::Neutral
    };
    let summary = if level == Level::Neutral {
        "Aucune mesure évaluée.".to_string()
    } else {
        summary_text(&bad_names, &warn_names)
    };
    Verdict {
        level,
        summary,
        ok,
        warn,
        bad,
    }
}

fn push_unique(names: &mut Vec<String>, label: String) {
    if !label.is_empty() && !names.contains(&label) {
        names.push(label);
    }
}

/// Ex. « Aucun problème critique. À surveiller : santé de la batterie et clavier. »
fn summary_text(bad: &[String], warn: &[String]) -> String {
    let mut out = match bad.len() {
        0 => "Aucun problème critique.".to_string(),
        1 => format!("Problème critique : {}.", enumerate(bad)),
        _ => format!("Problèmes critiques : {}.", enumerate(bad)),
    };
    if !warn.is_empty() {
        out.push_str(&format!(" À surveiller : {}.", enumerate(warn)));
    } else if bad.is_empty() {
        out.push_str(" Aucun point à surveiller.");
    }
    out
}

/// « a », « a et b », « a, b et c », « a, b, c, d et 2 autres ».
fn enumerate<S: AsRef<str>>(names: &[S]) -> String {
    let mut parts: Vec<String> = names
        .iter()
        .take(SUMMARY_MAX_NAMES)
        .map(|n| lower_first(n.as_ref().trim_end_matches('.')))
        .collect();
    let rest = names.len().saturating_sub(SUMMARY_MAX_NAMES);
    if rest > 0 {
        parts.push(if rest == 1 {
            "1 autre".to_string()
        } else {
            format!("{rest} autres")
        });
    }
    match parts.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, head)) => format!("{} et {last}", head.join(", ")),
    }
}

/// Met la première lettre en minuscule au milieu d'une phrase, sauf pour un sigle
/// (« SSD », « CPU », « A1 ») : on ne touche que si la 2e lettre est une minuscule.
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match (chars.next(), chars.clone().next()) {
        (Some(first), Some(second)) if first.is_uppercase() && second.is_lowercase() => {
            first.to_lowercase().chain(chars).collect()
        }
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Item;

    fn section(items: Vec<Item>) -> Section {
        Section {
            id: "s".into(),
            title: "S".into(),
            items,
            tables: Vec::new(),
        }
    }

    #[test]
    fn thresholds_match_plan_boundaries() {
        let t = Thresholds::default();
        assert_eq!(t.ssd_life(90), Level::Ok);
        assert_eq!(t.ssd_life(89), Level::Warn);
        assert_eq!(t.ssd_life(70), Level::Warn);
        assert_eq!(t.ssd_life(69), Level::Bad);

        assert_eq!(t.reallocated_sectors(0, false), Level::Ok);
        assert_eq!(t.reallocated_sectors(1, false), Level::Warn);
        assert_eq!(t.reallocated_sectors(50, false), Level::Warn);
        assert_eq!(t.reallocated_sectors(51, false), Level::Bad);
        assert_eq!(t.reallocated_sectors(2, true), Level::Bad);

        assert_eq!(t.pending_sectors(0), Level::Ok);
        assert_eq!(t.pending_sectors(1), Level::Bad);

        assert_eq!(t.disk_temperature(49), Level::Ok);
        assert_eq!(t.disk_temperature(50), Level::Warn);
        assert_eq!(t.disk_temperature(60), Level::Warn);
        assert_eq!(t.disk_temperature(61), Level::Bad);

        assert_eq!(t.battery_health(80.0), Level::Ok);
        assert_eq!(t.battery_health(79.9), Level::Warn);
        assert_eq!(t.battery_health(60.0), Level::Warn);
        assert_eq!(t.battery_health(59.9), Level::Bad);
        assert_eq!(t.battery_health(f64::NAN), Level::Neutral);

        assert_eq!(t.cpu_throttling(Throttling::None), Level::Ok);
        assert_eq!(t.cpu_throttling(Throttling::Brief), Level::Warn);
        assert_eq!(t.cpu_throttling(Throttling::Sustained), Level::Bad);

        assert_eq!(t.enterprise_management(false), Level::Ok);
        assert_eq!(t.enterprise_management(true), Level::Bad);

        assert_eq!(t.android_patch_age(2), Level::Ok);
        assert_eq!(t.android_patch_age(3), Level::Warn);
        assert_eq!(t.android_patch_age(12), Level::Warn);
        assert_eq!(t.android_patch_age(13), Level::Bad);

        assert_eq!(t.google_account(false), Level::Ok);
        assert_eq!(t.google_account(true), Level::Warn);
    }

    #[test]
    fn thresholds_partial_config_keeps_defaults() {
        let t: Thresholds =
            serde_json::from_str(r#"{"disk_temp_warn_max": 55}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(t.disk_temp_warn_max, 55);
        assert_eq!(t.ssd_life_ok_min, 90);
        assert_eq!(t.disk_temperature(56), Level::Bad);
    }

    #[test]
    fn verdict_levels_and_counters() {
        let green = compute_verdict(&[section(vec![
            Item::new("Vie restante", "98 %", Level::Ok),
            Item::new("Heures", "1 200 h", Level::Info),
        ])]);
        assert_eq!(green.level, Level::Ok);
        assert_eq!(green.label(), "Bon achat");
        assert_eq!((green.ok, green.warn, green.bad), (1, 0, 0));
        assert_eq!(
            green.summary,
            "Aucun problème critique. Aucun point à surveiller."
        );

        let yellow = compute_verdict(&[
            section(vec![Item::new("Santé de la batterie", "72 %", Level::Warn)]),
            section(vec![
                Item::new("Clavier", "2 touches sans réponse", Level::Warn),
                Item::new("SSD", "98 %", Level::Ok),
            ]),
        ]);
        assert_eq!(yellow.level, Level::Warn);
        assert_eq!(yellow.label(), "À négocier");
        assert_eq!(
            yellow.summary,
            "Aucun problème critique. À surveiller : santé de la batterie et clavier."
        );

        let red = compute_verdict(&[section(vec![
            Item::new("Secteurs en attente", "3", Level::Bad),
            Item::new("SSD usé", "65 %", Level::Bad),
            Item::new("Température", "55 °C", Level::Warn),
        ])]);
        assert_eq!(red.level, Level::Bad);
        assert_eq!(red.label(), "À éviter");
        assert_eq!((red.ok, red.warn, red.bad), (0, 1, 2));
        assert_eq!(
            red.summary,
            "Problèmes critiques : secteurs en attente et SSD usé. À surveiller : température."
        );
    }

    #[test]
    fn verdict_without_measures_is_neutral() {
        let v = compute_verdict(&[section(vec![Item::new("Modèle", "X", Level::Neutral)])]);
        assert_eq!(v.level, Level::Neutral);
        assert_eq!(v.label(), "Sans verdict");
    }

    #[test]
    fn long_lists_are_shortened() {
        let names = ["A1", "Bb", "Cc", "Dd", "Ee", "Ff"];
        assert_eq!(enumerate(&names), "A1, bb, cc, dd et 2 autres");
        assert_eq!(enumerate(&names[..1]), "A1");
    }

    #[test]
    fn label_shared_by_two_disks_names_the_disk() {
        let mut a = Section::new("disque-a", "Samsung 860 EVO");
        a.items
            .push(Item::new("Secteurs en attente", "0", Level::Ok));
        let mut b = Section::new("disque-b", "WD Blue");
        b.items
            .push(Item::new("Secteurs en attente", "8", Level::Bad));
        let v = compute_verdict(&[a, b]);
        assert_eq!(
            v.summary,
            "Problème critique : secteurs en attente (WD Blue)."
        );
    }
}
