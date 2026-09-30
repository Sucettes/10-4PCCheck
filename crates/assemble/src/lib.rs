//! Construction des rapports (disque, téléphone, machine) à partir des mesures de la session.
//!
//! Chaque mesure devient un `Item` dont le niveau vient de la table de seuils du plan (§4,
//! `Thresholds`) ; le verdict global est recalculé par `pccheck-report` à partir des sections.
//! Crate séparée de l'application : fonctions pures, testables sans droits administrateur.

use std::collections::HashMap;

use pccheck_android::{ChecklistItem, Finding, FindingLevel, PhoneReport};
use pccheck_core::age::{current_year, estimate_age, HoursRating, HDD_HOURS_END, HDD_HOURS_WORN};
use pccheck_core::capacity::CapacityResult;
use pccheck_core::checks::{fmt_bytes, fmt_int};
use pccheck_core::speed::{Rating, SpeedResult};
use pccheck_core::surface::SurfaceResult;
use pccheck_core::{
    AttributeStatus, CheckLevel, DiskEntry, DiskInfo, MediaKind, Protocol, SelfTestStatus,
};
use pccheck_inventory::{
    BitLockerProtection, GpuTestResult, MachineInventory, NetworkKind, RamTestResult,
    SecureBootState, StressResult, ThrottleLevel,
};
use pccheck_report::{
    ChecklistEntry, Detail, Item, Level, Report, Section, Subject, SubjectKind, Table, Thresholds,
    Throttling, ToolVersion,
};
use serde::{Deserialize, Serialize};

mod recovery;
pub use recovery::{build_recovery_report, RecoveryMethod, RecoverySession};

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Analyse d'un téléphone telle qu'envoyée à l'interface et gardée pour le rapport.
#[derive(Debug, Clone, Serialize)]
pub struct PhoneAnalysis {
    pub report: PhoneReport,
    pub findings: Vec<Finding>,
    pub verdict: FindingLevel,
    pub checklist: Vec<ChecklistItem>,
}

/// Derniers résultats obtenus pendant la session.
#[derive(Default)]
pub struct Results {
    pub disks: Vec<DiskEntry>,
    pub inventory: Option<MachineInventory>,
    pub stress: Option<StressResult>,
    pub ram: Option<RamTestResult>,
    /// Test de charge graphique (WebGL, mesuré par l'interface).
    pub gpu: Option<GpuTestResult>,
    /// Par chemin smartctl du disque.
    pub surface: HashMap<String, SurfaceResult>,
    pub self_tests: HashMap<String, SelfTestStatus>,
    /// Test de vitesse rapide, par chemin smartctl.
    pub speed: HashMap<String, SpeedResult>,
    /// Année de fabrication lue sur l'étiquette par l'utilisateur, par chemin smartctl.
    pub label_years: HashMap<String, u16>,
    pub capacity: Option<CapacityResult>,
    pub phone: Option<PhoneAnalysis>,
    /// Dernière récupération terminée (PhotoRec ou The Sleuth Kit).
    pub recovery: Option<RecoverySession>,
}

/// Résultat d'un test interactif, envoyé par l'interface (le moteur ne le voit pas autrement).
#[derive(Debug, Clone, Deserialize)]
pub struct InteractiveEntry {
    pub label: String,
    /// `pass`, `fail` ou `skipped`.
    pub status: String,
    pub note: Option<String>,
}

fn check_level(l: CheckLevel) -> Level {
    match l {
        CheckLevel::Ok => Level::Ok,
        CheckLevel::Info => Level::Info,
        CheckLevel::Warn => Level::Warn,
    }
}

fn attr_level(s: AttributeStatus) -> Level {
    match s {
        AttributeStatus::Ok => Level::Ok,
        AttributeStatus::Watch => Level::Warn,
        AttributeStatus::Failing => Level::Bad,
    }
}

fn finding_level(l: FindingLevel) -> Level {
    match l {
        FindingLevel::Ok => Level::Ok,
        FindingLevel::Info => Level::Info,
        FindingLevel::Warn => Level::Warn,
        FindingLevel::Bad => Level::Bad,
    }
}

fn disk_name(d: &DiskInfo) -> String {
    d.model
        .clone()
        .unwrap_or_else(|| d.device.info_name.clone())
}

fn media_label(d: &DiskInfo) -> String {
    match (&d.media, &d.protocol) {
        (MediaKind::Ssd, Protocol::Nvme) => "SSD NVMe".into(),
        (MediaKind::Ssd, _) => "SSD SATA".into(),
        (MediaKind::Hdd { rpm }, _) => format!("disque dur {} tr/min", fmt_int(u64::from(*rpm))),
        (MediaKind::Unknown, _) => "type inconnu".into(),
    }
}

fn raw_attr(d: &DiskInfo, id: u8) -> Option<u64> {
    d.ata_attributes
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.raw_value)
}

/// Section complète d'un disque : mesures, tests faits pendant la session, table des attributs.
fn disk_section(d: &DiskInfo, r: &Results, th: &Thresholds) -> Section {
    let mut s = Section::new(
        format!("disque-{}", d.device.name.trim_start_matches("/dev/")),
        format!("Disque · {}", disk_name(d)),
    );
    let items = &mut s.items;
    items.push(match d.smart_passed {
        Some(true) => Item::new("État SMART global", "Réussi", Level::Ok),
        Some(false) => Item::new("État SMART global", "Échec", Level::Bad)
            .with_detail("Le disque déclare lui-même une défaillance."),
        None => Item::new("État SMART global", "Non rapporté", Level::Neutral),
    });
    if let Some(p) = d.life_remaining_pct {
        let source = if d.protocol == Protocol::Nvme {
            "100 moins l'usure déclarée (Percentage Used, NVMe)."
        } else {
            "Attribut d'usure du fabricant."
        };
        items.push(Item::new("Vie restante", format!("{p} %"), th.ssd_life(p)).with_detail(source));
    }
    if let Some(n) = raw_attr(d, 5) {
        items.push(Item::new(
            "Secteurs réalloués",
            fmt_int(n),
            th.reallocated_sectors(n, false),
        ));
    }
    let pending: Option<u64> = [197, 198]
        .iter()
        .filter_map(|id| raw_attr(d, *id))
        .reduce(|a, b| a + b);
    if let Some(n) = pending {
        items.push(Item::new(
            "Secteurs en attente ou non corrigibles",
            fmt_int(n),
            th.pending_sectors(n),
        ));
    }
    if let Some(h) = &d.nvme_health {
        if let Some(n) = h.media_errors {
            items.push(Item::new(
                "Erreurs de média (NVMe)",
                fmt_int(n),
                th.pending_sectors(n),
            ));
        }
        if let Some(w) = h.critical_warning {
            let level = if w == 0 { Level::Ok } else { Level::Bad };
            items.push(Item::new(
                "Avertissement critique (NVMe)",
                if w == 0 {
                    "Aucun".into()
                } else {
                    format!("0x{w:02X}")
                },
                level,
            ));
        }
    }
    if let Some(t) = d.temperature_c {
        items.push(
            Item::new("Température", format!("{t} °C"), th.disk_temperature(t)).with_detail(
                "Mesurée pendant l'analyse ; repères du plan donnés pour un disque au repos.",
            ),
        );
    }
    items.extend(age_items(d, r));
    if let Some(c) = d.power_cycles {
        items.push(Item::new("Démarrages", fmt_int(c), Level::Info));
    }
    if let Some(b) = d.bytes_written {
        items.push(Item::new("Données écrites", fmt_bytes(b), Level::Info));
    }
    for c in &d.checks {
        // Libellé court : le résumé du verdict énumère les libellés ; la phrase va dans le détail.
        items.push(
            Item::new("Cohérence des compteurs", "", check_level(c.level))
                .with_detail(c.text.clone()),
        );
    }
    if let Some(last) = r
        .self_tests
        .get(&d.device.name)
        .and_then(|s| s.history.first())
    {
        let (value, level) = match last.passed {
            Some(true) => ("Réussi", Level::Ok),
            Some(false) => ("Échec", Level::Bad),
            None => ("Interrompu", Level::Info),
        };
        let mut item = Item::new(
            format!("Dernier auto-test SMART ({})", last.kind),
            value,
            level,
        );
        if last.passed == Some(false) {
            item = item.with_detail(last.text.clone());
        }
        items.push(item);
    }
    if let Some(sp) = r.speed.get(&d.device.name) {
        items.extend(speed_items(sp));
    }
    if let Some(sv) = r.surface.get(&d.device.name) {
        let mut item = if sv.bad_bytes > 0 {
            Item::new(
                "Scan de surface",
                format!("{} zone(s) illisible(s)", sv.bad_ranges.len()),
                Level::Bad,
            )
            .with_detail(format!("{} illisibles.", fmt_bytes(sv.bad_bytes)))
        } else if sv.slow_blocks > 0 {
            Item::new(
                "Scan de surface",
                format!("{} bloc(s) lent(s)", sv.slow_blocks),
                Level::Warn,
            )
        } else {
            Item::new("Scan de surface", "Aucune zone illisible", Level::Ok)
        };
        if sv.cancelled {
            item = item.with_detail(format!(
                "Scan arrêté après {} sur {}.",
                fmt_bytes(sv.read_bytes),
                fmt_bytes(sv.total_bytes)
            ));
        }
        items.push(item);
    }

    if !d.ata_attributes.is_empty() {
        s.tables.push(Table {
            title: "Attributs SMART".into(),
            columns: ["ID", "Attribut", "Actuel", "Pire", "Seuil", "Brut", "État"]
                .map(String::from)
                .to_vec(),
            rows: d
                .ata_attributes
                .iter()
                .map(|a| {
                    vec![
                        format!("{:02X}", a.id),
                        a.label_fr
                            .map(String::from)
                            .unwrap_or_else(|| a.name.clone()),
                        a.value.to_string(),
                        a.worst.to_string(),
                        a.threshold.to_string(),
                        a.raw_string.clone(),
                        attr_level(a.status).label().to_string(),
                    ]
                })
                .collect(),
        });
    }
    s
}

/// Heures d'utilisation (avec repères pour un disque dur) et âge estimé, avec l'intensité d'usage.
fn age_items(d: &DiskInfo, r: &Results) -> Vec<Item> {
    let label_year = r.label_years.get(&d.device.name).copied();
    let age = estimate_age(d, label_year, current_year());
    let mut items = Vec::new();
    if let Some(h) = age.power_on_hours {
        let days = format!("Environ {} jours allumé.", fmt_int(h / 24));
        let item = match age.hours_rating {
            Some(rating) => {
                let (word, level) = match rating {
                    HoursRating::Young => ("peu utilisé", Level::Ok),
                    HoursRating::Worn => ("usé", Level::Warn),
                    HoursRating::EndOfLife => ("fin de vie probable", Level::Bad),
                };
                Item::new(
                    "Heures d'utilisation",
                    format!("{} h · {word}", fmt_int(h)),
                    level,
                )
                .with_detail(format!(
                    "{days} Repères pour un disque dur : moins de {} h peu utilisé, {} à {} h usé, \
                     plus de {} h fin de vie probable (statistiques Backblaze).",
                    fmt_int(HDD_HOURS_WORN),
                    fmt_int(HDD_HOURS_WORN),
                    fmt_int(HDD_HOURS_END),
                    fmt_int(HDD_HOURS_END)
                ))
            }
            None => Item::new("Heures d'utilisation", format!("{} h", fmt_int(h)), Level::Info)
                .with_detail(format!(
                    "{days} Sur un SSD, l'usure se lit dans la vie restante plutôt que dans les heures."
                )),
        };
        items.push(item);
    }
    let years = |a: f64| format!("{} ans", a.round() as u64);
    let (value, mut detail) = match (age.age_years, label_year, age.model_year) {
        (Some(a), Some(y), _) => (format!("≈ {} (étiquette : {y})", years(a)), String::new()),
        (Some(a), None, Some(y)) => (
            format!("au plus ≈ {} (modèle sorti vers {y})", years(a)),
            "Le disque a pu être fabriqué après la sortie du modèle. Saisis l'année imprimée sur \
             l'étiquette pour un âge exact."
                .to_string(),
        ),
        _ => (
            "Inconnu".to_string(),
            "Saisis l'année imprimée sur l'étiquette du disque pour connaître son âge et son \
             intensité d'usage."
                .to_string(),
        ),
    };
    if let Some(per_day) = age.hours_per_day {
        let prefix = if age.age_is_maximum { "au moins " } else { "" };
        let per_day = format!("{per_day:.1}").replace('.', ",");
        detail = format!("Utilisé {prefix}{per_day} h par jour en moyenne. {detail}");
    }
    let level = if age.age_years.is_some() {
        Level::Info
    } else {
        Level::Neutral
    };
    items.push(Item::new("Âge estimé", value, level).with_detail(detail.trim().to_string()));
    items
}

fn rating_level(r: Rating) -> (&'static str, Level) {
    match r {
        Rating::Good => ("bon", Level::Ok),
        Rating::Acceptable => ("acceptable", Level::Ok),
        Rating::Weak => ("faible", Level::Warn),
        Rating::LimitedByLink => ("bridé par le port", Level::Info),
    }
}

fn mbps_text(v: f64) -> String {
    format!("{} Mo/s", fmt_int(v.round() as u64))
}

/// Lecture, temps d'accès et écriture du test rapide, placés sur l'échelle du type de disque.
fn speed_items(sp: &SpeedResult) -> Vec<Item> {
    let mut items = Vec::new();
    let scale = sp.scale.as_ref();
    let band_text = |good: f64, ok: f64, unit: &str, higher: bool| {
        let (g, a) = if higher {
            ("≥", "≥")
        } else {
            ("≤", "≤")
        };
        format!(
            "bon {g} {} {unit}, acceptable {a} {} {unit}",
            fmt_int(good as u64),
            fmt_int(ok as u64)
        )
    };
    match (&sp.read, &sp.read_error) {
        (Some(read), _) => {
            if let Some(first) = read.zones.first() {
                let zones = read
                    .zones
                    .iter()
                    .map(|z| {
                        let at = match z.position_pct {
                            0 => "début",
                            50 => "milieu",
                            _ => "fin",
                        };
                        format!("{at} {}", mbps_text(z.mbps))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let (value, level, mut detail) = match scale {
                    Some(sc) => {
                        let (word, level) = rating_level(sc.rate_throughput(&sc.read, first.mbps));
                        (
                            format!("{} · {word}", mbps_text(first.mbps)),
                            level,
                            format!(
                                "{zones}. Repères {} (début du disque) : {}.",
                                sc.class,
                                band_text(sc.read.good, sc.read.acceptable, "Mo/s", true)
                            ),
                        )
                    }
                    None => (mbps_text(first.mbps), Level::Info, format!("{zones}.")),
                };
                // Seulement si le port bride vraiment ce type de disque (jamais un disque dur).
                if let Some(cap) = scale.and_then(|s| s.link_cap_mbps.filter(|c| *c < s.read.good))
                {
                    detail.push_str(&format!(
                        " Port SATA ancien : environ {} au maximum, quel que soit le disque.",
                        mbps_text(cap)
                    ));
                }
                items.push(Item::new("Lecture", value, level).with_detail(detail));
            }
            if let Some(acc) = &read.access {
                let ms = format!("{:.1} ms", acc.avg_ms).replace('.', ",");
                let item = match scale.and_then(|s| s.access) {
                    Some(band) => {
                        let (word, level) = rating_level(band.rate(acc.avg_ms));
                        Item::new("Temps d'accès", format!("{ms} · {word}"), level).with_detail(
                            format!(
                                "Moyenne de {} lectures au hasard (maximum {:.0} ms). C'est la \
                                 lenteur ressentie à l'ouverture de nombreux petits fichiers. \
                                 Repères : {}.",
                                acc.samples,
                                acc.max_ms,
                                band_text(band.good, band.acceptable, "ms", false)
                            ),
                        )
                    }
                    None => Item::new("Temps d'accès", ms, Level::Info),
                };
                items.push(item);
            }
        }
        (None, Some(e)) => items.push(
            Item::new("Test de vitesse", "Impossible", Level::Neutral).with_detail(e.clone()),
        ),
        (None, None) => {}
    }
    match (&sp.write, &sp.write_skipped) {
        (Some(w), _) => {
            let detail = format!(
                "Fichier neuf de {} Go écrit dans l'espace libre de {}, relu puis supprimé. Débit \
                 de {} au plus bas à {} au plus haut (sur un SSD, la chute marque la fin de son \
                 cache rapide).",
                // Même unité que le choix de taille à l'écran (1, 5, 10 Go = × 1024³ octets).
                w.bytes >> 30,
                w.volume,
                mbps_text(w.min_mbps),
                mbps_text(w.max_mbps)
            );
            let item = match scale {
                Some(sc) => {
                    let (word, level) = rating_level(sc.rate_throughput(&sc.write, w.mbps));
                    Item::new("Écriture", format!("{} · {word}", mbps_text(w.mbps)), level)
                        .with_detail(format!(
                            "{detail} Repères {} : {}.",
                            sc.class,
                            band_text(sc.write.good, sc.write.acceptable, "Mo/s", true)
                        ))
                }
                None => Item::new("Écriture", mbps_text(w.mbps), Level::Info).with_detail(detail),
            };
            items.push(item);
            if let Some(rb) = &w.readback {
                let detail = "Lecture du fichier qui vient d'être écrit : des données réelles. \
                              Plus fiable que la lecture directe sur un disque neuf ou SMR, qui \
                              répond sans lire sur une zone jamais écrite.";
                let item = match scale {
                    Some(sc) => {
                        let (word, level) = rating_level(sc.rate_throughput(&sc.read, rb.mbps));
                        Item::new(
                            "Relecture (données réelles)",
                            format!("{} · {word}", mbps_text(rb.mbps)),
                            level,
                        )
                        .with_detail(detail)
                    }
                    None => Item::new(
                        "Relecture (données réelles)",
                        mbps_text(rb.mbps),
                        Level::Info,
                    )
                    .with_detail(detail),
                };
                items.push(item);
            }
            if w.readback_errors > 0 {
                items.push(
                    Item::new(
                        "Données relues différentes",
                        format!("{} bloc(s) de 8 Mo", fmt_int(w.readback_errors)),
                        Level::Bad,
                    )
                    .with_detail(
                        "Le disque a rendu d'autres données que celles écrites : défaut grave \
                         (mémoire, contrôleur ou surface).",
                    ),
                );
            }
            if let Some(e) = &w.readback_error {
                items.push(
                    Item::new("Relecture", "Impossible", Level::Warn)
                        .with_detail(format!("Le fichier de test n'a pas pu être relu : {e}")),
                );
            }
        }
        (None, Some(why)) => items
            .push(Item::new("Écriture", "Non mesurée", Level::Neutral).with_detail(why.clone())),
        (None, None) => {}
    }
    if sp.cancelled {
        items.push(Item::new(
            "Test de vitesse",
            "Arrêté avant la fin",
            Level::Neutral,
        ));
    }
    items
}

fn disk_details(d: &DiskInfo) -> Vec<Detail> {
    let mut v = vec![
        Detail::new("Type", media_label(d)),
        Detail::new(
            "Capacité",
            d.capacity_bytes
                .map(fmt_bytes)
                .unwrap_or_else(|| "Inconnue".into()),
        ),
    ];
    if let Some(f) = &d.firmware {
        v.push(Detail::new("Firmware", f.clone()));
    }
    if let Some(s) = &d.serial {
        v.push(Detail::new("Numéro de série", s.clone()));
    }
    if let Some(s) = &d.standard {
        v.push(Detail::new("Norme", s.clone()));
    }
    v
}

// ---------- Disque ----------

pub fn build_disk_report(
    r: &Results,
    device: &str,
    tools: Vec<ToolVersion>,
) -> Result<Report, String> {
    let d = r
        .disks
        .iter()
        .find_map(|e| e.info.as_ref().filter(|i| i.device.name == device))
        .ok_or_else(|| "disque inconnu : actualise la liste des disques".to_string())?;
    let th = Thresholds::default();
    let name = disk_name(d);
    let mut rep = Report::new(
        TOOL_VERSION,
        format!("Disque {name}"),
        Subject {
            kind: SubjectKind::Disk,
            name: name.clone(),
            details: disk_details(d),
        },
    );
    rep.sections.push(disk_section(d, r, &th));
    rep.tools = tools;
    rep.raw = serde_json::json!({
        "disk": d,
        "self_test": r.self_tests.get(device),
        "surface": r.surface.get(device),
        "speed": r.speed.get(device),
        "label_year": r.label_years.get(device),
    });
    Ok(rep)
}

// ---------- Téléphone ----------

pub fn build_phone_report(r: &Results, checklist: Vec<ChecklistEntry>) -> Result<Report, String> {
    let a = r
        .phone
        .as_ref()
        .ok_or_else(|| "aucune analyse de téléphone : lance l'analyse d'abord".to_string())?;
    let rep_phone = &a.report;
    let id = &rep_phone.identity;
    let name = [
        id.manufacturer.as_deref().or(id.brand.as_deref()),
        id.model.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    let name = if name.is_empty() {
        "Téléphone Android".to_string()
    } else {
        name
    };
    let mut details = vec![Detail::new("Numéro de série", rep_phone.serial.clone())];
    if let Some(v) = &id.android_version {
        details.push(Detail::new("Android", v.clone()));
    }
    if let Some(p) = &rep_phone.security.security_patch_raw {
        details.push(Detail::new("Patch de sécurité", p.clone()));
    }
    if let Some(s) = &rep_phone.storage {
        details.push(Detail::new(
            "Stockage",
            format!(
                "{} utilisés sur {}",
                fmt_bytes(s.used_bytes),
                fmt_bytes(s.total_bytes)
            ),
        ));
    }

    let mut rep = Report::new(
        TOOL_VERSION,
        format!("Téléphone {name}"),
        Subject {
            kind: SubjectKind::Phone,
            name,
            details,
        },
    );
    let mut findings = Section::new("constats", "Constats");
    findings.items = a
        .findings
        .iter()
        .map(|f| {
            Item::new(f.label.clone(), "", finding_level(f.level)).with_detail(f.detail.clone())
        })
        .collect();
    rep.sections.push(findings);

    if let Some(b) = &rep_phone.battery {
        let mut s = Section::new("batterie", "Batterie");
        if let Some(h) = &b.health_label {
            s.items
                .push(Item::new("Santé Android", h.clone(), Level::Neutral));
        }
        if let Some(c) = b.cycle_count {
            s.items
                .push(Item::new("Cycles", c.to_string(), Level::Neutral));
        }
        if let Some(p) = b.capacity_pct {
            s.items.push(Item::new(
                "Capacité réelle",
                format!("{p} % de l'origine"),
                Level::Neutral,
            ));
        }
        if let Some(t) = b.temperature_c {
            s.items.push(Item::new(
                "Température",
                format!("{t:.1} °C"),
                Level::Neutral,
            ));
        }
        rep.sections.push(s);
    }
    let mut notes = Section::new("notes", "À savoir");
    notes.items.push(Item::new(
        "IMEI",
        rep_phone.imei_note.clone(),
        Level::Neutral,
    ));
    notes.items.push(Item::new(
        "Usure de la mémoire",
        rep_phone.flash_wear_note.clone(),
        Level::Neutral,
    ));
    rep.sections.push(notes);
    rep.checklist = checklist;
    rep.raw = serde_json::to_value(a).unwrap_or_default();
    Ok(rep)
}

// ---------- Machine (analyse complète) ----------

fn machine_name(inv: &MachineInventory) -> String {
    let join = |a: Option<&str>, b: Option<&str>| {
        [a, b].into_iter().flatten().collect::<Vec<_>>().join(" ")
    };
    // PC monté : le modèle est souvent absent (« System Product Name » écarté par l'inventaire) ;
    // la carte mère identifie alors mieux la machine.
    match (&inv.computer, &inv.board) {
        (Some(c), _) if c.model.is_some() => join(c.manufacturer.as_deref(), c.model.as_deref()),
        (_, Some(b)) if b.product.is_some() => {
            join(b.manufacturer.as_deref(), b.product.as_deref())
        }
        (Some(c), _) if c.manufacturer.is_some() => join(c.manufacturer.as_deref(), None),
        _ => "Ordinateur".into(),
    }
}

fn machine_details(inv: &MachineInventory) -> Vec<Detail> {
    let mut v = Vec::new();
    if let Some(cpu) = inv.cpu.as_ref().and_then(|c| c.name.clone()) {
        v.push(Detail::new("Processeur", cpu));
    }
    if let Some(total) = inv.memory.as_ref().and_then(|m| m.total_bytes) {
        v.push(Detail::new("Mémoire", fmt_bytes(total)));
    }
    if let Some(os) = &inv.os {
        let text = [os.name.as_deref(), os.display_version.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            v.push(Detail::new("Système", text));
        }
    }
    if let Some(s) = inv.computer.as_ref().and_then(|c| c.serial_number.clone()) {
        v.push(Detail::new("Numéro de série", s));
    }
    if let Some(b) = &inv.bios {
        if let Some(ver) = &b.version {
            v.push(Detail::new("BIOS", ver.clone()));
        }
    }
    v
}

fn battery_section(inv: &MachineInventory, th: &Thresholds) -> Option<Section> {
    let b = inv.battery.as_ref()?;
    let mut s = Section::new("batterie", "Batterie");
    match b.health_pct {
        Some(p) => {
            let mut item = Item::new(
                "Santé de la batterie",
                format!("{p:.0} %"),
                th.battery_health(p),
            );
            if let (Some(full), Some(design)) = (b.full_charge_capacity_mwh, b.design_capacity_mwh)
            {
                item = item.with_detail(format!(
                    "Capacité actuelle / d'origine : {:.1} Wh / {:.1} Wh.",
                    full as f64 / 1000.0,
                    design as f64 / 1000.0
                ));
            }
            s.items.push(item);
        }
        None => s.items.push(Item::new(
            "Santé de la batterie",
            "Non rapportée",
            Level::Neutral,
        )),
    }
    if let Some(c) = b.cycle_count {
        s.items
            .push(Item::new("Cycles de charge", c.to_string(), Level::Info));
    }
    Some(s)
}

fn cpu_section(inv: &MachineInventory, r: &Results, th: &Thresholds) -> Section {
    let mut s = Section::new("processeur", "Processeur");
    if let Some(cpu) = &inv.cpu {
        let desc = [
            cpu.name.clone(),
            cpu.cores.map(|c| format!("{c} cœurs")),
            cpu.threads.map(|t| format!("{t} fils")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        s.items.push(Item::new("Modèle", desc, Level::Neutral));
    }
    match &r.stress {
        None => s
            .items
            .push(Item::new("Test de charge", "Non lancé", Level::Neutral)),
        Some(st) => {
            let (throttling, value) = throttling(st.throttling.level);
            let mut detail = format!(
                "{} s de charge sur {} fils, baisse de débit {:.0} % par rapport à la référence.",
                st.duration_ms / 1000,
                st.threads,
                st.throttling.drop_pct.max(0.0)
            );
            if let Some(t) = st.max_celsius {
                detail.push_str(&format!(" Température maximale : {t:.0} °C."));
            }
            if st.cancelled {
                detail.push_str(" Test arrêté avant la fin.");
            }
            s.items.push(
                Item::new("Test de charge", value, th.cpu_throttling(throttling))
                    .with_detail(detail),
            );
            if st.computation_errors > 0 {
                s.items.push(
                    Item::new(
                        "Erreurs de calcul",
                        fmt_int(st.computation_errors),
                        Level::Bad,
                    )
                    .with_detail(
                        "Des résultats faux sous charge : processeur ou alimentation instable.",
                    ),
                );
            }
        }
    }
    s
}

/// Niveau de bridage mesuré (processeur ou carte graphique) → seuil du rapport et libellé.
fn throttling(level: ThrottleLevel) -> (Throttling, &'static str) {
    match level {
        ThrottleLevel::None => (Throttling::None, "Aucun bridage"),
        ThrottleLevel::Brief => (Throttling::Brief, "Bridage bref"),
        ThrottleLevel::Sustained => (Throttling::Sustained, "Bridage soutenu"),
    }
}

/// Carte graphique : modèle, puis test de charge WebGL (bridage, erreurs de rendu, température).
fn gpu_section(inv: &MachineInventory, r: &Results, th: &Thresholds) -> Section {
    let mut s = Section::new("graphique", "Carte graphique");
    for g in &inv.gpus {
        let mut v = g.name.clone();
        if let Some(m) = g.memory_bytes {
            v.push_str(&format!(" · {}", fmt_bytes(m)));
        }
        s.items.push(Item::new("Modèle", v, Level::Neutral));
    }
    let Some(g) = &r.gpu else {
        s.items.push(Item::new(
            "Test de charge graphique",
            "Non lancé",
            Level::Neutral,
        ));
        return s;
    };
    let (level, value) = throttling(g.throttling.level);
    let mut detail = format!(
        "{:.0} s de rendu 3D intensif, baisse de débit {:.0} % par rapport à la référence.",
        g.duration_s,
        g.throttling.drop_pct.max(0.0)
    );
    if let Some(t) = g.max_temperature_c {
        detail.push_str(&format!(" Température maximale : {t:.0} °C."));
    }
    if g.input.cancelled {
        detail.push_str(" Test arrêté avant la fin.");
    }
    s.items.push(
        Item::new("Test de charge graphique", value, th.cpu_throttling(level)).with_detail(detail),
    );
    s.items.push(if g.input.render_errors > 0 {
        Item::new(
            "Erreurs de rendu",
            format!("{} sur {} contrôles", g.input.render_errors, g.input.checks),
            Level::Bad,
        )
        .with_detail(
            "La même image, rendue plusieurs fois, a changé : carte instable, surchauffe ou mémoire vidéo défectueuse.",
        )
    } else {
        Item::new(
            "Erreurs de rendu",
            format!("Aucune sur {} contrôles", g.input.checks),
            Level::Ok,
        )
    });
    s
}

fn memory_section(inv: &MachineInventory, r: &Results) -> Section {
    let mut s = Section::new("memoire", "Mémoire");
    if let Some(m) = &inv.memory {
        if let Some(t) = m.total_bytes {
            s.items
                .push(Item::new("Mémoire totale", fmt_bytes(t), Level::Neutral));
        }
        if !m.modules.is_empty() {
            s.tables.push(Table {
                title: "Barrettes".into(),
                columns: [
                    "Emplacement",
                    "Capacité",
                    "Type",
                    "Vitesse",
                    "Fabricant",
                    "Référence",
                ]
                .map(String::from)
                .to_vec(),
                rows: m
                    .modules
                    .iter()
                    .map(|b| {
                        vec![
                            b.slot.clone().unwrap_or_default(),
                            b.capacity_bytes.map(fmt_bytes).unwrap_or_default(),
                            b.memory_type.clone().unwrap_or_default(),
                            b.speed_mhz.map(|v| format!("{v} MHz")).unwrap_or_default(),
                            b.manufacturer.clone().unwrap_or_default(),
                            b.part_number.clone().unwrap_or_default(),
                        ]
                    })
                    .collect(),
            });
        }
    }
    match &r.ram {
        None => s
            .items
            .push(Item::new("Test RAM partiel", "Non lancé", Level::Neutral)),
        Some(t) if t.errors > 0 => s.items.push(
            Item::new(
                "Test RAM partiel",
                format!("{} erreur(s)", fmt_int(t.errors)),
                Level::Bad,
            )
            .with_detail("Barrette défectueuse probable : confirme avec MemTest86+ au démarrage."),
        ),
        Some(t) => s.items.push(
            Item::new(
                "Test RAM partiel",
                format!("Aucune erreur sur {}", fmt_bytes(t.tested_bytes)),
                Level::Ok,
            )
            .with_detail(
                "Mémoire libre seulement : un test complet exige MemTest86+ au démarrage.",
            ),
        ),
    }
    s
}

fn security_section(inv: &MachineInventory, th: &Thresholds) -> Option<Section> {
    let sec = inv.security.as_ref()?;
    let mut s = Section::new("securite", "Licence et sécurité");
    if let Some(l) = &sec.windows_license {
        let level = if l.activated { Level::Ok } else { Level::Warn };
        let mut item = Item::new(
            "Licence Windows",
            if l.activated {
                "Activée"
            } else {
                "Non activée"
            },
            level,
        );
        if let Some(d) = &l.description {
            item = item.with_detail(d.clone());
        }
        s.items.push(item);
    }
    let mut managed = Item::new(
        "Gestion d'entreprise (Intune, Autopilot, domaine)",
        if sec.enterprise_managed {
            "Présente"
        } else {
            "Aucune"
        },
        th.enterprise_management(sec.enterprise_managed),
    );
    if sec.enterprise_managed {
        let mut why = Vec::new();
        if sec.intune_enrolled == Some(true) {
            why.push("inscrit dans Intune".to_string());
        }
        if let Some(a) = sec.autopilot.as_ref().filter(|a| a.assigned) {
            why.push(format!(
                "attribué par Autopilot{}",
                a.tenant_domain
                    .as_ref()
                    .map(|t| format!(" à {t}"))
                    .unwrap_or_default()
            ));
        }
        if let Some(j) = &sec.device_join {
            if j.azure_ad_joined == Some(true) {
                why.push(format!(
                    "joint à Azure AD{}",
                    j.tenant_name
                        .as_ref()
                        .map(|t| format!(" ({t})"))
                        .unwrap_or_default()
                ));
            }
            if j.domain_joined == Some(true) {
                why.push(format!(
                    "membre du domaine {}",
                    j.domain_name.clone().unwrap_or_default()
                ));
            }
        }
        managed = managed.with_detail(format!(
            "Machine {} : elle peut être verrouillée à distance par l'entreprise.",
            why.join(", ")
        ));
    }
    s.items.push(managed);
    if let Some(sb) = &sec.secure_boot {
        let v = match sb {
            SecureBootState::Enabled => "Actif",
            SecureBootState::Disabled => "Désactivé",
            SecureBootState::Unsupported => "Non pris en charge (BIOS ancien mode)",
        };
        s.items.push(Item::new("Secure Boot", v, Level::Info));
    }
    if let Some(t) = &sec.tpm {
        let v = if t.present {
            format!(
                "Présent{}",
                t.version
                    .as_ref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default()
            )
        } else {
            "Absent".into()
        };
        s.items.push(Item::new("TPM", v, Level::Info));
    }
    if let Some(vols) = &sec.bitlocker {
        for v in vols {
            let state = match v.protection {
                BitLockerProtection::On => "Chiffré",
                BitLockerProtection::Off => "Non chiffré",
                BitLockerProtection::Unknown => "Inconnu",
            };
            s.items.push(Item::new(
                format!("BitLocker {}", v.drive.clone().unwrap_or_default()),
                state,
                Level::Info,
            ));
        }
    }
    Some(s)
}

fn hardware_section(inv: &MachineInventory) -> Section {
    let mut s = Section::new("materiel", "Réseau et carte mère");
    for n in &inv.network_adapters {
        let kind = match n.kind {
            NetworkKind::Wifi => "Wi-Fi",
            NetworkKind::Ethernet => "Ethernet",
            NetworkKind::Bluetooth => "Bluetooth",
            NetworkKind::Other => "Réseau",
        };
        s.items
            .push(Item::new(kind, n.name.clone(), Level::Neutral));
    }
    if let Some(b) = &inv.board {
        let v = [b.manufacturer.as_deref(), b.product.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        if !v.is_empty() {
            s.items.push(Item::new("Carte mère", v, Level::Neutral));
        }
    }
    for t in &inv.temperatures {
        s.items.push(Item::new(
            format!("Température · {}", t.label),
            format!("{:.0} °C", t.celsius),
            Level::Neutral,
        ));
    }
    s
}

fn interactive_section(entries: &[InteractiveEntry]) -> Option<Section> {
    if entries.is_empty() {
        return None;
    }
    let mut s = Section::new("tests-interactifs", "Tests interactifs");
    for e in entries {
        let (value, level) = match e.status.as_str() {
            "pass" => ("Réussi", Level::Ok),
            "fail" => ("Échec", Level::Warn),
            _ => ("Non fait", Level::Neutral),
        };
        let mut item = Item::new(e.label.clone(), value, level);
        if let Some(n) = &e.note {
            item = item.with_detail(n.clone());
        }
        s.items.push(item);
    }
    Some(s)
}

pub fn build_machine_report(
    r: &Results,
    interactive: &[InteractiveEntry],
    checklist: Vec<ChecklistEntry>,
    tools: Vec<ToolVersion>,
) -> Result<Report, String> {
    let inv = r
        .inventory
        .as_ref()
        .ok_or_else(|| "inventaire absent : lance l'analyse complète d'abord".to_string())?;
    let th = Thresholds::default();
    let name = machine_name(inv);
    let mut rep = Report::new(
        TOOL_VERSION,
        name.clone(),
        Subject {
            kind: SubjectKind::Machine,
            name,
            details: machine_details(inv),
        },
    );

    // Vue d'ensemble des disques, puis le détail de chacun.
    let disks: Vec<&DiskInfo> = r.disks.iter().filter_map(|e| e.info.as_ref()).collect();
    if !disks.is_empty() {
        let mut overview = Section::new("disques", "Disques");
        for d in &disks {
            let summary = [
                d.life_remaining_pct.map(|p| format!("{p} % de vie")),
                d.power_on_hours.map(|h| format!("{} h", fmt_int(h))),
                d.bytes_written.map(|b| format!("{} écrits", fmt_bytes(b))),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            // Niveau neutre : l'état du disque est compté une seule fois, dans sa section détaillée.
            overview.items.push(Item::new(
                format!("{} ({})", disk_name(d), media_label(d)),
                summary,
                Level::Neutral,
            ));
        }
        rep.sections.push(overview);
        for d in &disks {
            rep.sections.push(disk_section(d, r, &th));
        }
    }
    if let Some(s) = battery_section(inv, &th) {
        rep.sections.push(s);
    }
    rep.sections.push(cpu_section(inv, r, &th));
    rep.sections.push(memory_section(inv, r));
    if let Some(s) = security_section(inv, &th) {
        rep.sections.push(s);
    }
    rep.sections.push(gpu_section(inv, r, &th));
    rep.sections.push(hardware_section(inv));
    if let Some(s) = interactive_section(interactive) {
        rep.sections.push(s);
    }
    rep.checklist = checklist;
    rep.tools = tools;
    rep.raw = serde_json::json!({
        "inventory": inv,
        "disks": r.disks,
        "cpu_stress": r.stress,
        "gpu_stress": r.gpu,
        "ram_test": r.ram,
        "surface": r.surface,
        "self_tests": r.self_tests,
        "disk_speed": r.speed,
        "label_years": r.label_years,
    });
    Ok(rep)
}
