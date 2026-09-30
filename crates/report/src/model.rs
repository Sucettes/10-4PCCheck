//! Schéma versionné du rapport. C'est la source unique : l'écran, le HTML et le PDF sont tous
//! produits depuis ce JSON (plan, section 4).
//!
//! Règle d'évolution : un champ ajouté doit avoir une valeur par défaut (`#[serde(default)]`)
//! pour que les anciens rapports restent lisibles ; un changement incompatible incrémente
//! [`SCHEMA_VERSION`], et [`crate::load`] refuse un rapport plus récent que ce qu'il connaît.

use chrono::{DateTime, FixedOffset, Local, SubsecRound};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ReportError;
use crate::verdict::compute_verdict;

/// Version du schéma JSON produite par cette crate.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    /// Version de 10-4 PCCheck qui a produit le rapport.
    pub tool_version: String,
    /// Date de génération, heure locale avec son décalage (RFC 3339, ex. `2026-09-29T14:30:00-04:00`).
    pub generated_at: DateTime<FixedOffset>,
    pub title: String,
    pub subject: Subject,
    pub verdict: Verdict,
    pub sections: Vec<Section>,
    #[serde(default)]
    pub checklist: Vec<ChecklistEntry>,
    /// Outils tiers utilisés (smartctl, adb...) et leur version.
    #[serde(default)]
    pub tools: Vec<ToolVersion>,
    /// Données brutes complètes (sorties des outils), sans transformation.
    #[serde(default)]
    pub raw: serde_json::Value,
}

impl Report {
    /// Rapport vide daté de maintenant (heure locale), verdict « sans verdict » tant que
    /// [`Report::update_verdict`] n'a pas été appelé.
    pub fn new(
        tool_version: impl Into<String>,
        title: impl Into<String>,
        subject: Subject,
    ) -> Self {
        Report {
            schema_version: SCHEMA_VERSION,
            tool_version: tool_version.into(),
            generated_at: now_local(),
            title: title.into(),
            subject,
            verdict: compute_verdict(&[]),
            sections: Vec::new(),
            checklist: Vec::new(),
            tools: Vec::new(),
            raw: serde_json::Value::Null,
        }
    }

    /// Recalcule le verdict depuis les sections.
    pub fn update_verdict(&mut self) {
        self.verdict = compute_verdict(&self.sections);
    }
}

/// Heure locale actuelle, à la seconde près (les fractions n'apportent rien dans un rapport).
pub fn now_local() -> DateTime<FixedOffset> {
    Local::now().fixed_offset().trunc_subsecs(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Machine,
    Disk,
    Phone,
}

impl SubjectKind {
    pub fn label(self) -> &'static str {
        match self {
            SubjectKind::Machine => "Ordinateur",
            SubjectKind::Disk => "Disque",
            SubjectKind::Phone => "Téléphone",
        }
    }
}

/// Ce qui a été analysé.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    pub kind: SubjectKind,
    pub name: String,
    #[serde(default)]
    pub details: Vec<Detail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    pub label: String,
    pub value: String,
}

impl Detail {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Detail {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// État d'une mesure. Chaque état a un libellé texte en plus de sa couleur (plan, section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Info,
    Warn,
    Bad,
    /// Non évalué (donnée descriptive, mesure indisponible).
    Neutral,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Ok => "Bon",
            Level::Info => "Info",
            Level::Warn => "À surveiller",
            Level::Bad => "Critique",
            Level::Neutral => "—",
        }
    }

    /// Clé stable (celle du JSON), utilisée comme classe CSS et clé de palette Typst.
    pub fn key(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Bad => "bad",
            Level::Neutral => "neutral",
        }
    }
}

/// Verdict global. `level` vaut `ok`, `warn` ou `bad` ; `neutral` si aucune mesure n'a été évaluée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub level: Level,
    pub summary: String,
    pub ok: u32,
    pub warn: u32,
    pub bad: u32,
}

impl Verdict {
    /// Libellé d'achat : Bon achat, À négocier, À éviter.
    pub fn label(&self) -> &'static str {
        match self.level {
            Level::Ok => "Bon achat",
            Level::Warn => "À négocier",
            Level::Bad => "À éviter",
            Level::Info | Level::Neutral => "Sans verdict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    /// Identifiant stable (`disk`, `battery`...), pas affiché.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub tables: Vec<Table>,
}

impl Section {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Section {
            id: id.into(),
            title: title.into(),
            items: Vec::new(),
            tables: Vec::new(),
        }
    }
}

/// Une mesure affichée : libellé, valeur formatée, état et explication facultative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub label: String,
    pub value: String,
    pub level: Level,
    #[serde(default)]
    pub detail: Option<String>,
}

impl Item {
    pub fn new(label: impl Into<String>, value: impl Into<String>, level: Level) -> Self {
        Item {
            label: label.into(),
            value: value.into(),
            level,
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Tableau de données (attributs SMART, historique...). Cellules déjà formatées en texte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Nombre de colonnes à afficher : celles déclarées, ou la ligne la plus longue si aucune.
    pub fn width(&self) -> usize {
        if self.columns.is_empty() {
            self.rows.iter().map(Vec::len).max().unwrap_or(0)
        } else {
            self.columns.len()
        }
    }

    /// Lignes complétées ou tronquées à [`Table::width`], pour qu'une ligne mal formée ne
    /// décale pas les suivantes (Typst remplit les cellules à la suite).
    pub fn normalized_rows(&self) -> Vec<Vec<String>> {
        let width = self.width();
        self.rows
            .iter()
            .map(|row| {
                let mut r: Vec<String> = row.iter().take(width).cloned().collect();
                r.resize(width, String::new());
                r
            })
            .collect()
    }
}

/// Vérification manuelle faite devant le vendeur (clavier, écran, compte retiré...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistEntry {
    pub label: String,
    pub checked: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolVersion {
    pub name: String,
    pub version: String,
}

/// Octets exacts du fichier `.json` d'un rapport : `serde_json::to_vec(report)`, JSON compact
/// (sans espaces), champs dans l'ordre de déclaration des structures.
pub fn report_json(report: &Report) -> Result<Vec<u8>, ReportError> {
    serde_json::to_vec(report).map_err(|e| ReportError::Json(e.to_string()))
}

/// Empreinte SHA-256, en hexadécimal minuscule (64 caractères), de [`report_json`].
///
/// Ce qui est haché : les octets de `serde_json::to_vec(&report)`, c'est-à-dire exactement le
/// contenu du fichier `.json` écrit par [`crate::save`]. On peut donc vérifier un rapport sans
/// cet outil : `sha256sum rapport.json` (Linux) ou `certutil -hashfile rapport.json SHA256`
/// (Windows) doit redonner l'empreinte du pied de page. Relire puis réécrire le rapport donne
/// les mêmes octets (sérialisation déterministe), donc la même empreinte.
///
/// Limite : l'empreinte détecte une modification, elle ne prouve pas l'auteur (pas de signature).
///
/// La sérialisation ne peut pas échouer pour ce type (serde_json n'échoue que sur une clé de
/// map non textuelle ou un `Serialize` qui renvoie une erreur, absents ici). Si c'était le cas,
/// la fonction renverrait une chaîne vide, jamais une empreinte valide.
pub fn report_hash(report: &Report) -> String {
    report_json(report)
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
