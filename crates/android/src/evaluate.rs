//! Évaluation d'un rapport de téléphone selon les règles du verdict du plan (section 4).
//!
//! Chaque constat a un niveau : `ok` (vert), `info` (neutre), `warn` (jaune), `bad` (rouge).
//! Verdict global : rouge si un rouge, jaune si au moins un jaune, sinon vert.
//! Les seuils sont dans des constantes pour être ajustés sur de vrais cas.

use serde::Serialize;

use crate::battery::{BatteryHealth, BatteryInfo};
use crate::collect::{PhoneReport, FLASH_WEAR_NOTE, IMEI_NOTE};
use crate::date::Date;
use crate::props::VerifiedBootState;

/// Niveaux dans l'ordre de gravité (l'ordre de déclaration sert à `Ord`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingLevel {
    Ok,
    Info,
    Warn,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub level: FindingLevel,
    pub label: String,
    pub detail: String,
}

impl Finding {
    fn new(level: FindingLevel, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Finding {
            level,
            label: label.into(),
            detail: detail.into(),
        }
    }
}

/// Correctif de moins de 3 mois : vert ; de 3 à 12 mois : jaune ; plus de 12 mois : rouge.
pub const PATCH_WARN_FROM_MONTHS: i32 = 3;
pub const PATCH_BAD_AFTER_MONTHS: i32 = 12;
/// Android 5 et moins (SDK 22) : aucun correctif mensuel, plus aucun suivi.
pub const LAST_SDK_WITHOUT_PATCH_LEVEL: u32 = 22;
/// Capacité de la batterie en % de l'origine : ≥ 80 vert, 60 à 79 jaune, < 60 rouge.
pub const BATTERY_CAPACITY_OK_PCT: u16 = 80;
pub const BATTERY_CAPACITY_WARN_PCT: u16 = 60;
/// Nombre de cycles au-delà duquel la plupart des batteries sont sous 80 % de capacité.
pub const BATTERY_CYCLES_WARN: u32 = 800;
/// Température de batterie anormale au repos, en dixièmes de °C (45 °C).
pub const BATTERY_TEMP_WARN_DECICELSIUS: i32 = 450;

/// Évalue le rapport. `today` est passé en paramètre pour rester testable (voir `date::today`).
pub fn evaluate(report: &PhoneReport, today: Date) -> Vec<Finding> {
    let mut out = Vec::new();
    security_patch(report, today, &mut out);
    accounts(report, &mut out);
    owners(report, &mut out);
    boot(report, &mut out);
    root(report, &mut out);
    match &report.battery {
        Some(b) => battery(b, &mut out),
        None => out.push(Finding::new(
            FindingLevel::Info,
            "Batterie",
            "État de la batterie illisible.",
        )),
    }
    storage(report, &mut out);
    out.push(Finding::new(FindingLevel::Info, "IMEI", IMEI_NOTE));
    out.push(Finding::new(
        FindingLevel::Info,
        "Usure de la mémoire",
        FLASH_WEAR_NOTE,
    ));
    out
}

/// Verdict global : le niveau le plus grave, `info` comptant comme vert.
pub fn verdict(findings: &[Finding]) -> FindingLevel {
    match findings.iter().map(|f| f.level).max() {
        Some(level @ (FindingLevel::Warn | FindingLevel::Bad)) => level,
        _ => FindingLevel::Ok,
    }
}

fn security_patch(report: &PhoneReport, today: Date, out: &mut Vec<Finding>) {
    const LABEL: &str = "Correctif de sécurité";
    let Some(patch) = report.security.security_patch else {
        let old = report
            .identity
            .sdk
            .is_some_and(|s| s <= LAST_SDK_WITHOUT_PATCH_LEVEL);
        out.push(if old {
            Finding::new(
                FindingLevel::Bad,
                LABEL,
                "Android 5 ou plus ancien : plus aucune mise à jour de sécurité depuis des années.",
            )
        } else {
            Finding::new(
                FindingLevel::Info,
                LABEL,
                match &report.security.security_patch_raw {
                    Some(raw) => format!("Date du correctif illisible (« {raw} »)."),
                    None => "Date du correctif non indiquée par le téléphone.".to_string(),
                },
            )
        });
        return;
    };
    // Horloge du PC en retard sur le téléphone : on ne compte pas d'âge négatif.
    let months = patch.months_until(today).max(0);
    let age = match months {
        0 => "moins d'un mois".to_string(),
        m => format!("{m} mois"),
    };
    let (level, advice) = if months < PATCH_WARN_FROM_MONTHS {
        (FindingLevel::Ok, "À jour.")
    } else if months <= PATCH_BAD_AFTER_MONTHS {
        (
            FindingLevel::Warn,
            "Cherche une mise à jour (Paramètres > Mise à jour logicielle) : s'il n'y en a pas, \
             le fabricant a peut-être arrêté le suivi de ce modèle.",
        )
    } else {
        (
            FindingLevel::Bad,
            "Plus d'un an sans correctif : le téléphone ne reçoit probablement plus de mises à \
             jour de sécurité.",
        )
    };
    out.push(Finding::new(
        level,
        LABEL,
        format!("Correctif du {patch}, il y a {age}. {advice}"),
    ));
}

fn accounts(report: &PhoneReport, out: &mut Vec<Finding>) {
    const LABEL: &str = "Comptes";
    let Some(list) = &report.accounts else {
        out.push(Finding::new(
            FindingLevel::Info,
            LABEL,
            "Liste des comptes illisible : regarde dans Paramètres > Comptes.",
        ));
        return;
    };
    if list.is_empty() {
        out.push(Finding::new(
            FindingLevel::Ok,
            LABEL,
            "Aucun compte connecté.",
        ));
        return;
    }
    for a in list.iter().filter(|a| a.activation_lock) {
        let name = a.label.unwrap_or(a.account_type.as_str());
        out.push(Finding::new(
            FindingLevel::Warn,
            format!("Compte {name} connecté"),
            format!(
                "{} compte(s) {name}. Le vendeur doit le retirer devant toi (Paramètres > Comptes) \
                 avant la réinitialisation d'usine : sinon le téléphone restera bloqué sur ce \
                 compte au redémarrage (protection FRP chez Google, verrou de réactivation chez \
                 Samsung).",
                a.count
            ),
        ));
    }
    let others: Vec<String> = list
        .iter()
        .filter(|a| !a.activation_lock)
        .map(|a| format!("{} {}", a.count, a.label.unwrap_or(a.account_type.as_str())))
        .collect();
    if !others.is_empty() {
        out.push(Finding::new(
            FindingLevel::Info,
            "Autres comptes",
            format!(
                "{}. Effacés par la réinitialisation d'usine.",
                others.join(", ")
            ),
        ));
    }
}

fn owners(report: &PhoneReport, out: &mut Vec<Finding>) {
    const LABEL: &str = "Gestion d'entreprise";
    let Some(owners) = &report.owners else {
        out.push(Finding::new(
            FindingLevel::Info,
            LABEL,
            "État inconnu : regarde dans Paramètres > Sécurité > Applis d'administration de \
             l'appareil.",
        ));
        return;
    };
    if let Some(pkg) = &owners.device_owner {
        out.push(Finding::new(
            FindingLevel::Bad,
            LABEL,
            format!(
                "Téléphone géré par une organisation (application {pkg}). Même réinitialisé, il \
                 peut se réinscrire tout seul (Knox Mobile Enrollment, inscription sans contact). \
                 N'achète pas sans preuve écrite que l'entreprise l'a libéré."
            ),
        ));
    }
    for p in &owners.profile_owners {
        out.push(Finding::new(
            FindingLevel::Warn,
            "Profil professionnel",
            format!(
                "Profil géré par l'application {} (utilisateur {}). Il doit être supprimé avant \
                 la vente ; demande au vendeur d'où il vient.",
                p.package, p.user_id
            ),
        ));
    }
    if owners.is_empty() {
        out.push(Finding::new(
            FindingLevel::Ok,
            LABEL,
            "Aucune gestion d'entreprise (ni propriétaire d'appareil, ni profil géré).",
        ));
    }
}

fn boot(report: &PhoneReport, out: &mut Vec<Finding>) {
    let s = &report.security;
    const BOOT: &str = "Démarrage vérifié";
    let (level, detail) = match &s.verified_boot {
        Some(VerifiedBootState::Green) => (
            FindingLevel::Ok,
            "État vert : système d'origine du fabricant.".to_string(),
        ),
        Some(VerifiedBootState::Yellow) => (
            FindingLevel::Warn,
            "État jaune : système signé par une autre clé que celle du fabricant (ROM \
             personnalisée)."
                .to_string(),
        ),
        Some(VerifiedBootState::Orange) => (
            FindingLevel::Warn,
            "État orange : chargeur de démarrage déverrouillé, l'intégrité du système n'est pas \
             vérifiée."
                .to_string(),
        ),
        Some(VerifiedBootState::Red) => (
            FindingLevel::Bad,
            "État rouge : système corrompu ou modifié.".to_string(),
        ),
        Some(VerifiedBootState::Other(v)) => (FindingLevel::Info, format!("État « {v} ».")),
        None => (
            FindingLevel::Info,
            "État non indiqué par le téléphone.".to_string(),
        ),
    };
    out.push(Finding::new(level, BOOT, detail));

    const LOADER: &str = "Chargeur de démarrage";
    out.push(match s.bootloader_locked {
        Some(true) => Finding::new(FindingLevel::Ok, LOADER, "Verrouillé."),
        Some(false) => Finding::new(
            FindingLevel::Warn,
            LOADER,
            "Déverrouillé : le système a pu être modifié. Demande pourquoi au vendeur ; des \
             applications bancaires et le paiement sans contact peuvent refuser de fonctionner.",
        ),
        None => Finding::new(FindingLevel::Info, LOADER, "État non indiqué."),
    });

    if s.knox_warranty_void == Some(true) {
        out.push(Finding::new(
            FindingLevel::Warn,
            "Knox déclenché",
            "Le téléphone a déjà été déverrouillé ou modifié (compteur Knox à 1, irréversible) : \
             Samsung Wallet et Dossier sécurisé ne fonctionnent plus.",
        ));
    }
}

fn root(report: &PhoneReport, out: &mut Vec<Finding>) {
    let reasons = report.security.root.reasons();
    if reasons.is_empty() {
        out.push(Finding::new(
            FindingLevel::Ok,
            "Root",
            "Aucun indice de root ni de système modifié.",
        ));
    } else {
        out.push(Finding::new(
            FindingLevel::Warn,
            "Indices de root",
            format!(
                "{}. Un système modifié peut cacher des problèmes ou des logiciels indésirables.",
                capitalize(&reasons.join(", "))
            ),
        ));
    }
}

fn battery(b: &BatteryInfo, out: &mut Vec<Finding>) {
    if b.updates_stopped {
        out.push(Finding::new(
            FindingLevel::Warn,
            "Valeurs de batterie figées",
            "Quelqu'un a figé les valeurs de la batterie (commande « dumpsys battery set ») : \
             niveau et température affichés peuvent être faux.",
        ));
    }

    const HEALTH: &str = "Santé de la batterie";
    if let Some(h) = b.health {
        let label = h.label();
        let (level, detail) = match h {
            BatteryHealth::Good => (
                FindingLevel::Ok,
                format!("{label} selon Android (aucune panne détectée ; ce n'est pas une mesure d'usure)."),
            ),
            BatteryHealth::Overheat | BatteryHealth::Cold => (
                FindingLevel::Warn,
                format!("{label} : laisse le téléphone revenir à température ambiante et vérifie de nouveau."),
            ),
            BatteryHealth::Dead
            | BatteryHealth::OverVoltage
            | BatteryHealth::UnspecifiedFailure => (
                FindingLevel::Bad,
                format!("{label} : batterie à remplacer."),
            ),
            BatteryHealth::Unknown | BatteryHealth::Other(_) => {
                (FindingLevel::Info, format!("{label}."))
            }
        };
        out.push(Finding::new(level, HEALTH, detail));
    }

    const CAPACITY: &str = "Capacité de la batterie";
    match (b.capacity_pct, b.charge_full_uah, b.charge_full_design_uah) {
        (Some(pct), Some(full), Some(design)) => {
            let level = if pct >= BATTERY_CAPACITY_OK_PCT {
                FindingLevel::Ok
            } else if pct >= BATTERY_CAPACITY_WARN_PCT {
                FindingLevel::Warn
            } else {
                FindingLevel::Bad
            };
            out.push(Finding::new(
                level,
                CAPACITY,
                format!(
                    "{pct} % de la capacité d'origine ({} mAh sur {} mAh).",
                    full / 1000,
                    design / 1000
                ),
            ));
        }
        _ => out.push(Finding::new(
            FindingLevel::Info,
            CAPACITY,
            "Non lisible sans root sur ce téléphone. Regarde dans les paramètres (Batterie > \
             Santé de la batterie) ou, chez Samsung, dans Samsung Members > Diagnostics > \
             État de la batterie.",
        )),
    }

    if let Some(cycles) = b.cycle_count {
        let level = if cycles > BATTERY_CYCLES_WARN {
            FindingLevel::Warn
        } else {
            FindingLevel::Info
        };
        out.push(Finding::new(
            level,
            "Cycles de charge",
            format!("{cycles} cycles de charge complets."),
        ));
    }

    if let Some(t) = b
        .temperature_decicelsius
        .filter(|t| *t > BATTERY_TEMP_WARN_DECICELSIUS)
    {
        out.push(Finding::new(
            FindingLevel::Warn,
            "Température de la batterie",
            format!(
                "{} °C : trop chaud au repos. Vérifie de nouveau après quelques minutes sans charge.",
                fmt_tenths(t)
            ),
        ));
    }
}

fn storage(report: &PhoneReport, out: &mut Vec<Finding>) {
    if let Some(s) = report.storage {
        out.push(Finding::new(
            FindingLevel::Info,
            "Stockage",
            format!(
                "{} utilisés sur {}, {} libres (partition des données).",
                fmt_bytes(s.used_bytes),
                fmt_bytes(s.total_bytes),
                fmt_bytes(s.free_bytes)
            ),
        ));
    }
}

/// 471 → « 47,1 ».
fn fmt_tenths(t: i32) -> String {
    let sign = if t < 0 { "-" } else { "" };
    format!("{sign}{},{}", t.abs() / 10, t.abs() % 10)
}

/// Octets en unités décimales (Go), virgule décimale.
fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let text = if unit == 0 || value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}").replace('.', ",")
    };
    format!("{text} {}", UNITS[unit])
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_helpers() {
        assert_eq!(fmt_tenths(471), "47,1");
        assert_eq!(fmt_tenths(-25), "-2,5");
        assert_eq!(fmt_bytes(113_348_235_264), "113 Go");
        assert_eq!(fmt_bytes(4_294_967_296), "4,3 Go");
        assert_eq!(capitalize("élan"), "Élan");
    }

    #[test]
    fn verdict_rules() {
        let f = |level| Finding::new(level, "x", "y");
        assert_eq!(verdict(&[]), FindingLevel::Ok);
        assert_eq!(
            verdict(&[f(FindingLevel::Ok), f(FindingLevel::Info)]),
            FindingLevel::Ok
        );
        assert_eq!(
            verdict(&[f(FindingLevel::Warn), f(FindingLevel::Info)]),
            FindingLevel::Warn
        );
        assert_eq!(
            verdict(&[f(FindingLevel::Warn), f(FindingLevel::Bad)]),
            FindingLevel::Bad
        );
    }
}
