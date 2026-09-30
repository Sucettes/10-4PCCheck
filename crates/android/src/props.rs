//! Propriétés système (`adb shell getprop`) : identité du téléphone et état de sécurité.
//!
//! Format : une ligne `[clé]: [valeur]` par propriété. Selon la version d'Android, une même
//! information existe sous plusieurs noms (`ro.product.model`, `ro.product.vendor.model`...) :
//! on prend le premier nom présent et non vide dans une liste ordonnée.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::date::Date;

/// Propriétés brutes, triées par nom.
pub type Props = BTreeMap<String, String>;

/// Analyse la sortie complète de `getprop`. Les lignes mal formées sont ignorées ; une valeur
/// sur plusieurs lignes (rare) est perdue.
pub fn parse_getprop(text: &str) -> Props {
    let mut props = Props::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix('[') else {
            continue;
        };
        let Some((key, value)) = rest.split_once("]: [") else {
            continue;
        };
        let Some(value) = value.strip_suffix(']') else {
            continue;
        };
        props.insert(key.to_string(), value.to_string());
    }
    props
}

/// Identité du téléphone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceIdentity {
    pub manufacturer: Option<String>,
    pub brand: Option<String>,
    /// Référence commerciale (`SM-G973W`, `Pixel 7`).
    pub model: Option<String>,
    /// Nom de produit (`beyond1ltevl`) : distingue les variantes régionales.
    pub product_name: Option<String>,
    /// Nom de code de l'appareil (`beyond1`, `panther`).
    pub device_code: Option<String>,
    pub android_version: Option<String>,
    pub sdk: Option<u32>,
    /// Niveau d'API à la sortie d'usine : indique l'âge du modèle.
    pub first_api_level: Option<u32>,
    pub build_fingerprint: Option<String>,
    /// Code vendeur Samsung (CSC) : pays et opérateur d'origine (`XAC` = Canada, débloqué).
    pub sales_code: Option<String>,
}

/// État de démarrage vérifié (Android Verified Boot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifiedBootState {
    /// Chargeur verrouillé, système signé par le fabricant.
    Green,
    /// Chargeur verrouillé, système signé par une autre clé (ROM personnalisée).
    Yellow,
    /// Chargeur déverrouillé : intégrité non vérifiée.
    Orange,
    /// Système corrompu.
    Red,
    Other(String),
}

impl VerifiedBootState {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "green" => VerifiedBootState::Green,
            "yellow" => VerifiedBootState::Yellow,
            "orange" => VerifiedBootState::Orange,
            "red" => VerifiedBootState::Red,
            other => VerifiedBootState::Other(other.to_string()),
        }
    }
}

/// Indices d'un téléphone rooté ou d'un système modifié. Aucun n'est une preuve à lui seul.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct RootHints {
    /// `ro.debuggable=1` : système de développement (normalement 0 sur un téléphone du commerce).
    pub debuggable: Option<bool>,
    /// `ro.build.type` : `user` dans le commerce, `userdebug` ou `eng` sinon.
    pub build_type: Option<String>,
    /// `ro.build.tags` contient `test-keys` : système signé avec des clés de test.
    pub test_keys: Option<bool>,
    /// `which su` trouve un binaire `su`. `None` si la commande n'a pas pu être lancée.
    pub su_found: Option<bool>,
}

impl RootHints {
    /// Raisons lisibles, vide si aucun indice.
    pub fn reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.su_found == Some(true) {
            out.push("commande « su » présente (accès root)".to_string());
        }
        if self.test_keys == Some(true) {
            out.push("système signé avec des clés de test (ROM modifiée)".to_string());
        }
        if let Some(t) = self
            .build_type
            .as_deref()
            .filter(|t| *t == "userdebug" || *t == "eng")
        {
            out.push(format!("système de type « {t} » au lieu de « user »"));
        }
        if self.debuggable == Some(true) {
            out.push("ro.debuggable=1 (système de développement)".to_string());
        }
        out
    }
}

/// État de sécurité tiré des propriétés système.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecurityState {
    /// Date du dernier correctif de sécurité (Android 6 et plus).
    pub security_patch: Option<Date>,
    /// Valeur brute, gardée si elle n'est pas une date lisible.
    pub security_patch_raw: Option<String>,
    pub verified_boot: Option<VerifiedBootState>,
    /// `false` si une des propriétés dit « déverrouillé ».
    pub bootloader_locked: Option<bool>,
    /// Samsung : compteur Knox déclenché (`ro.boot.warranty_bit=1`), irréversible.
    pub knox_warranty_void: Option<bool>,
    pub root: RootHints,
}

const MANUFACTURER: [&str; 3] = [
    "ro.product.manufacturer",
    "ro.product.vendor.manufacturer",
    "ro.product.system.manufacturer",
];
const BRAND: [&str; 2] = ["ro.product.brand", "ro.product.vendor.brand"];
const MODEL: [&str; 3] = [
    "ro.product.model",
    "ro.product.vendor.model",
    "ro.product.system.model",
];
const PRODUCT_NAME: [&str; 2] = ["ro.product.name", "ro.product.vendor.name"];
const DEVICE_CODE: [&str; 2] = ["ro.product.device", "ro.product.vendor.device"];
const ANDROID_VERSION: [&str; 2] = [
    "ro.build.version.release",
    "ro.build.version.release_or_codename",
];
const FINGERPRINT: [&str; 2] = ["ro.build.fingerprint", "ro.vendor.build.fingerprint"];
/// Code vendeur Samsung : le nom varie selon la génération (propriété à confirmer sur de vrais
/// appareils, voir le plan).
const SALES_CODE: [&str; 3] = [
    "ro.csc.sales_code",
    "persist.omc.sales_code",
    "ro.boot.sales_code",
];
const KNOX_WARRANTY: [&str; 2] = ["ro.boot.warranty_bit", "ro.warranty_bit"];

/// Première valeur non vide parmi `keys`, dans l'ordre.
fn get(props: &Props, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|k| props.get(*k))
        .map(|v| v.trim())
        .find(|v| !v.is_empty())
        .map(str::to_string)
}

fn get_u32(props: &Props, key: &str) -> Option<u32> {
    get(props, &[key]).and_then(|v| v.parse().ok())
}

fn get_flag(props: &Props, keys: &[&str]) -> Option<bool> {
    match get(props, keys)?.as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

pub fn identity_from_props(props: &Props) -> DeviceIdentity {
    DeviceIdentity {
        manufacturer: get(props, &MANUFACTURER),
        brand: get(props, &BRAND),
        model: get(props, &MODEL),
        product_name: get(props, &PRODUCT_NAME),
        device_code: get(props, &DEVICE_CODE),
        android_version: get(props, &ANDROID_VERSION),
        sdk: get_u32(props, "ro.build.version.sdk"),
        first_api_level: get_u32(props, "ro.product.first_api_level"),
        build_fingerprint: get(props, &FINGERPRINT),
        sales_code: get(props, &SALES_CODE),
    }
}

/// État de sécurité. `root.su_found` reste `None` : il vient d'une autre commande (`which su`).
pub fn security_from_props(props: &Props) -> SecurityState {
    let patch_raw = get(props, &["ro.build.version.security_patch"]);
    SecurityState {
        security_patch: patch_raw.as_deref().and_then(Date::parse),
        security_patch_raw: patch_raw,
        verified_boot: get(props, &["ro.boot.verifiedbootstate"])
            .map(|v| VerifiedBootState::parse(&v)),
        bootloader_locked: bootloader_locked(props),
        knox_warranty_void: get_flag(props, &KNOX_WARRANTY),
        root: RootHints {
            debuggable: get_flag(props, &["ro.debuggable"]),
            build_type: get(props, &["ro.build.type"]),
            test_keys: get(props, &["ro.build.tags"]).map(|t| t.contains("test-keys")),
            su_found: None,
        },
    }
}

/// Deux sources : `ro.boot.flash.locked` (1/0) et `ro.boot.vbmeta.device_state`
/// (locked/unlocked). Au moindre « déverrouillé », on retient déverrouillé.
fn bootloader_locked(props: &Props) -> Option<bool> {
    let flash = get_flag(props, &["ro.boot.flash.locked"]);
    let vbmeta = get(props, &["ro.boot.vbmeta.device_state"]).and_then(|v| {
        match v.to_ascii_lowercase().as_str() {
            "locked" => Some(true),
            "unlocked" => Some(false),
            _ => None,
        }
    });
    match (flash, vbmeta) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), _) | (_, Some(true)) => Some(true),
        _ => None,
    }
}

/// Analyse la sortie de `which su` : présent si une ligne est un chemin absolu finissant par
/// `/su`. Les messages d'erreur des vieux systèmes (« which: not found ») comptent comme absent.
pub fn parse_which_su(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|l| l.starts_with('/') && l.ends_with("/su"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getprop_line_format() {
        let p = parse_getprop(
            "[ro.product.model]: [Pixel 7]\r\n[empty]: []\nligne parasite\n[a]: [b: [c]]\n",
        );
        assert_eq!(
            p.get("ro.product.model").map(String::as_str),
            Some("Pixel 7")
        );
        assert_eq!(p.get("empty").map(String::as_str), Some(""));
        assert_eq!(p.get("a").map(String::as_str), Some("b: [c]"));
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn fallback_keys_and_empty_values() {
        let p = parse_getprop("[ro.product.model]: []\n[ro.product.vendor.model]: [SM-A515W]\n");
        assert_eq!(identity_from_props(&p).model.as_deref(), Some("SM-A515W"));
    }

    #[test]
    fn bootloader_unlocked_wins() {
        let p = parse_getprop(
            "[ro.boot.flash.locked]: [1]\n[ro.boot.vbmeta.device_state]: [unlocked]\n",
        );
        assert_eq!(bootloader_locked(&p), Some(false));
        assert_eq!(bootloader_locked(&Props::new()), None);
    }

    #[test]
    fn which_su_detection() {
        assert!(parse_which_su("/system/xbin/su\n"));
        assert!(!parse_which_su(""));
        assert!(!parse_which_su("/system/bin/sh: which: not found\n"));
    }
}
