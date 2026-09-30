//! 10-4 PCCheck : rapport d'analyse.
//!
//! - [`Report`] : schéma JSON versionné ([`SCHEMA_VERSION`]), source unique de l'écran, du HTML
//!   et du PDF.
//! - [`Thresholds`] et [`compute_verdict`] : règles du verdict (plan, section 4).
//! - [`to_html`] : fichier HTML autonome ; [`to_pdf`] : PDF lettre avec Typst embarqué.
//! - [`save`], [`list`], [`load`] : dossier de rapports.
//!
//! La crate est générique : elle ne dépend d'aucune autre crate du projet. Les modules de
//! collecte (disques, inventaire, Android) construisent des [`Section`] avec les fonctions de
//! [`Thresholds`] pour évaluer leurs mesures.

mod error;
mod format;
mod html;
mod model;
mod pdf;
mod sample;
mod store;
mod verdict;

pub use error::ReportError;
pub use format::{date_fr, slug};
pub use html::{escape_html, to_html};
pub use model::{
    now_local, report_hash, report_json, ChecklistEntry, Detail, Item, Level, Report, Section,
    Subject, SubjectKind, Table, ToolVersion, Verdict, SCHEMA_VERSION,
};
pub use pdf::to_pdf;
pub use sample::sample_report;
pub use store::{list, load, save, ReportSummary, SavedReport};
pub use verdict::{compute_verdict, Thresholds, Throttling};
