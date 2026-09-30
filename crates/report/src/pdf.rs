//! Export PDF avec Typst embarqué (décision du plan, section 4).
//!
//! Architecture : Typst ne voit le monde qu'à travers le trait [`World`]. Notre implémentation
//! ([`ReportWorld`]) ne sert que deux fichiers virtuels, en mémoire :
//! - `/main.typ` : le gabarit, compilé dans le binaire (`report.typ`) ;
//! - `/data.json` : les données du rapport, lues par le gabarit avec `json("/data.json")`.
//!
//! Tout autre chemin renvoie « introuvable » : pas d'accès disque, pas de paquet `@preview`,
//! donc pas de réseau. Les polices sont incluses dans le binaire, aucune police système n'est
//! lue : rendu identique sur Windows et Linux.
//!
//! Anti-injection : le texte du rapport n'est jamais concaténé au balisage. Dans le gabarit,
//! une chaîne issue de `json()` est une *valeur* : l'insérer dans le document produit du texte
//! littéral, sans interprétation de `#`, `*`, `_`, `[`... (comme un paramètre SQL lié,
//! opposé à une requête construite par concaténation).

use std::sync::OnceLock;

use serde::Serialize;
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

use crate::error::ReportError;
use crate::format::date_fr;
use crate::model::{report_hash, Report};

const TEMPLATE: &str = include_str!("report.typ");

/// Rend le rapport en PDF (format lettre).
pub fn to_pdf(report: &Report) -> Result<Vec<u8>, ReportError> {
    let document = compile(report)?;
    typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|errs| ReportError::Pdf(messages(&errs)))
}

/// Mise en page du rapport (pages positionnées), avant l'export PDF.
fn compile(report: &Report) -> Result<PagedDocument, ReportError> {
    let data =
        serde_json::to_vec(&PdfData::new(report)).map_err(|e| ReportError::Json(e.to_string()))?;
    let world = ReportWorld::new(data)?;
    let compiled = typst::compile::<PagedDocument>(&world).output;
    // Le cache de Typst (comemo) est global et garde les résultats intermédiaires pour une
    // recompilation incrémentale, utile dans un éditeur, pas ici : on le vide pour ne pas
    // accumuler de mémoire dans une app qui reste ouverte.
    typst::comemo::evict(0);
    compiled.map_err(|errs| ReportError::Typst(messages(&errs)))
}

fn messages(errs: &[SourceDiagnostic]) -> Vec<String> {
    errs.iter()
        .map(|d| {
            let mut m = d.message.to_string();
            for hint in &d.hints {
                m.push_str(" (indice : ");
                m.push_str(hint.v.as_str());
                m.push(')');
            }
            m
        })
        .collect()
}

/// Ce que lit le gabarit : le rapport tel quel, plus les textes déjà formatés en français
/// (libellés, date, empreinte), pour garder le gabarit sans logique métier.
#[derive(Serialize)]
struct PdfData<'a> {
    report: &'a Report,
    hash: String,
    date: String,
    subject_kind: &'static str,
    verdict_label: &'static str,
    /// Libellé français de chaque état, par clé (`ok` → « Bon »).
    level_labels: LevelLabels,
    /// Tableaux avec lignes normalisées, dans l'ordre des sections.
    tables: Vec<Vec<PdfTable<'a>>>,
}

#[derive(Serialize)]
struct LevelLabels {
    ok: &'static str,
    info: &'static str,
    warn: &'static str,
    bad: &'static str,
    neutral: &'static str,
}

#[derive(Serialize)]
struct PdfTable<'a> {
    title: &'a str,
    columns: Vec<&'a str>,
    rows: Vec<Vec<String>>,
}

impl<'a> PdfData<'a> {
    fn new(report: &'a Report) -> Self {
        use crate::model::Level;
        let tables = report
            .sections
            .iter()
            .map(|s| {
                s.tables
                    .iter()
                    .filter(|t| t.width() > 0)
                    .map(|t| PdfTable {
                        title: &t.title,
                        columns: (0..t.width())
                            .map(|i| t.columns.get(i).map(String::as_str).unwrap_or(""))
                            .collect(),
                        rows: t.normalized_rows(),
                    })
                    .collect()
            })
            .collect();
        PdfData {
            report,
            hash: report_hash(report),
            date: date_fr(&report.generated_at),
            subject_kind: report.subject.kind.label(),
            verdict_label: report.verdict.label(),
            level_labels: LevelLabels {
                ok: Level::Ok.label(),
                info: Level::Info.label(),
                warn: Level::Warn.label(),
                bad: Level::Bad.label(),
                neutral: Level::Neutral.label(),
            },
            tables,
        }
    }
}

/// Ressources partagées entre les compilations : bibliothèque standard, polices et leur
/// index. Construites une fois (analyse des polices) puis réutilisées.
struct Assets {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let fonts: Vec<Font> = [
            dejavu::sans::regular(),
            dejavu::sans::bold(),
            dejavu::sans_mono::regular(),
        ]
        .into_iter()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
        Assets {
            library: LazyHash::new(Library::builder().build()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

/// Monde Typst minimal : deux fichiers en mémoire, polices embarquées, rien d'autre.
struct ReportWorld {
    main_id: FileId,
    data_id: FileId,
    main: Source,
    data: Bytes,
    assets: &'static Assets,
}

impl ReportWorld {
    fn new(data: Vec<u8>) -> Result<Self, ReportError> {
        let main_id = virtual_file("/main.typ")?;
        Ok(ReportWorld {
            main_id,
            data_id: virtual_file("/data.json")?,
            main: Source::new(main_id, TEMPLATE.to_string()),
            data: Bytes::new(data),
            assets: assets(),
        })
    }
}

fn virtual_file(path: &str) -> Result<FileId, ReportError> {
    let vpath = VirtualPath::new(path)
        .map_err(|e| ReportError::Typst(vec![format!("chemin virtuel {path} : {e:?}")]))?;
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

impl World for ReportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.assets.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.assets.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.main.clone())
        } else if id == self.data_id {
            Err(FileError::NotSource)
        } else {
            Err(not_found(id))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.data_id {
            Ok(self.data.clone())
        } else if id == self.main_id {
            Ok(Bytes::from_string(TEMPLATE))
        } else {
            Err(not_found(id))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.assets.fonts.get(index).cloned()
    }

    /// Le gabarit n'utilise pas `datetime.today()` (la date vient du rapport) : `None` fait
    /// échouer un éventuel appel au lieu de mettre dans le PDF une date autre que l'analyse.
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn not_found(id: FileId) -> FileError {
    FileError::NotFound(format!("{:?}", id.vpath()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Item, Level, Section, Table};
    use crate::sample::sample_report;
    use typst::layout::{Frame, FrameItem};

    /// Texte posé sur les pages, dans l'ordre des éléments (pour vérifier le rendu littéral).
    fn page_text(doc: &PagedDocument) -> String {
        fn walk(frame: &Frame, out: &mut String) {
            for (_, item) in frame.items() {
                match item {
                    FrameItem::Group(g) => walk(&g.frame, out),
                    FrameItem::Text(t) => {
                        out.push_str(t.text.as_str());
                        out.push(' ');
                    }
                    _ => {}
                }
            }
        }
        let mut out = String::new();
        for page in doc.pages() {
            walk(&page.frame, &mut out);
        }
        out
    }

    #[test]
    fn template_compiles_without_warnings() {
        // Un avertissement Typst (police introuvable, glyphe manquant) ne bloque pas le PDF
        // mais dégrade le rendu : on les refuse tous sur l'exemple.
        let data = serde_json::to_vec(&PdfData::new(&sample_report())).unwrap();
        let world = ReportWorld::new(data).unwrap();
        let warned = typst::compile::<PagedDocument>(&world);
        let warnings: Vec<String> = warned
            .warnings
            .iter()
            .map(|w| w.message.to_string())
            .collect();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(warned.output.is_ok());
    }

    #[test]
    fn sample_has_pages_and_repeated_footer() {
        let doc = compile(&sample_report()).unwrap();
        assert!(!doc.pages().is_empty());
        let text = page_text(&doc);
        assert!(text.contains("négocier"), "{text}");
        assert!(text.contains(&report_hash(&sample_report())));
    }

    #[test]
    fn user_text_is_never_evaluated_as_typst() {
        let mut r = sample_report();
        r.title = "#panic(\"injection\") ]#pagebreak()[ $x^2$ *gras* _it_ @ref <lbl> \\ é".into();
        r.subject.name = "\")+panic()+(\"".into();
        r.sections[0].items.push(Item::new(
            "#set page(width: 1pt)",
            "#{ panic() }",
            Level::Ok,
        ));
        let doc = compile(&r).unwrap();
        let text = page_text(&doc);
        // Le titre apparaît tel quel (Typst découpe le texte en mots, d'où la recherche par morceaux).
        for needle in [
            "#panic(",
            "]#pagebreak()[",
            "$x^2$",
            "*gras*",
            "_it_",
            "<lbl>",
        ] {
            assert!(text.contains(needle), "{needle} absent de : {text}");
        }
    }

    #[test]
    fn long_and_irregular_tables_paginate() {
        let mut r = sample_report();
        let mut s = Section::new("big", "Grand tableau");
        s.tables.push(Table {
            title: "Lignes irrégulières".into(),
            columns: vec!["A".into(), "B".into(), "C".into()],
            rows: (0..300)
                .map(|i| match i % 3 {
                    0 => vec![i.to_string()],
                    1 => vec![i.to_string(), "x".repeat(120), "y".into(), "trop".into()],
                    _ => vec![i.to_string(), "b".into(), "c".into()],
                })
                .collect(),
        });
        s.tables.push(Table {
            title: "Sans colonnes".into(),
            columns: vec![],
            rows: vec![vec!["seule".into()]],
        });
        s.tables.push(Table {
            title: "Vide".into(),
            columns: vec![],
            rows: vec![],
        });
        r.sections.push(s);
        let doc = compile(&r).unwrap();
        assert!(doc.pages().len() >= 3, "{} pages", doc.pages().len());
    }
}
