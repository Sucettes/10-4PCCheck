//! Paramètres d'une récupération et ligne de commande PhotoRec correspondante.
//!
//! Syntaxe du mode script (« scripted run ») :
//! https://www.cgsecurity.org/wiki/Scripted_run
//! `photorec [/debug] [/log] [/logname file.log] [/d recup_dir] [/cmd <device> <command>]`,
//! les commandes étant séparées par des virgules, dans l'ordre : type de table de partitions,
//! `options,...`, `fileopt,...`, puis `search` qui lance la récupération.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::disk::DiskId;

/// Préfixe des dossiers créés par PhotoRec : `/d <dest>/recup_dir` produit `recup_dir.1`,
/// `recup_dir.2`... (500 fichiers par dossier). Si `recup_dir.1` existe déjà, PhotoRec prend le
/// numéro libre suivant (même page du wiki, option `/d`).
pub const RECUP_DIR_PREFIX: &str = "recup_dir";

/// Nom du journal écrit par `/log` dans le dossier courant de PhotoRec (mode ajout).
pub const LOG_FILE_NAME: &str = "photorec.log";

/// Ce que PhotoRec doit lire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// Le disque entier (y compris l'espace hors partitions).
    Disk { disk: DiskId },
    /// Une partition ou un volume : `device` est le chemin système (`\\.\E:` sous Windows,
    /// `/dev/sdb1` sous Linux) ; `disk` est le disque physique qui la porte, pour la règle
    /// « destination sur un autre disque ».
    Partition { device: String, disk: DiskId },
}

impl Source {
    /// Chemin passé à PhotoRec après `/cmd`.
    pub fn device(&self) -> String {
        match self {
            Source::Disk { disk } => disk.photorec_device(),
            Source::Partition { device, .. } => device.clone(),
        }
    }

    /// Disque physique lu.
    pub fn disk(&self) -> &DiskId {
        match self {
            Source::Disk { disk } | Source::Partition { disk, .. } => disk,
        }
    }
}

/// Familles de fichiers proposées dans l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFamily {
    /// JPEG, PNG, HEIC, RAW d'appareils photo...
    Photos,
    /// PDF, Office (anciens et récents), texte.
    Documents,
    /// MP4, MOV, AVI, MKV...
    Videos,
    /// MP3, WAV, FLAC, OGG...
    Audio,
    /// ZIP, 7z, RAR...
    Archives,
    /// Tous les formats activés par défaut dans PhotoRec.
    Everything,
}

impl FileFamily {
    /// Identifiants PhotoRec (`fileopt,<id>,enable`) de la famille. Ce sont les champs
    /// `extension` des `file_hint_t` des sources (src/file_*.c,
    /// https://github.com/cgsecurity/testdisk/tree/master/src). Un identifiant couvre souvent
    /// plusieurs formats, d'où certains recouvrements :
    /// - `mov` : mov, mp4, 3gp, m4a, **heic** et cr3 (file_mov.c) ;
    /// - `tif` : TIFF et RAW nef, cr2, dng, pef... (file_tiff.c) ;
    /// - `riff` : avi, wav, webp (file_riff.c) ;
    /// - `zip` : zip, mais aussi docx, xlsx, odt, epub (file_zip.c) ;
    /// - `doc` : Office 97-2003 doc, xls, ppt (file_doc.c) ;
    /// - `txt` : texte, html, csv, scripts ; `tx?` : rtf, xml... (file_txt.c) ;
    /// - `asf` : wmv, wma.
    ///
    /// Un identifiant inconnu fait échouer toute la commande (« Syntax error », phcli.c) :
    /// n'ajouter ici que des noms vérifiés dans les sources.
    pub fn photorec_formats(self) -> &'static [&'static str] {
        match self {
            FileFamily::Photos => &[
                "jpg", "png", "gif", "bmp", "tif", "mov", "psd", "crw", "raf", "orf", "rw2", "mrw",
                "x3f",
            ],
            FileFamily::Documents => &["pdf", "doc", "zip", "txt", "tx?"],
            FileFamily::Videos => &["mov", "riff", "mkv", "asf", "mpg"],
            FileFamily::Audio => &["mp3", "riff", "flac", "ogg", "asf", "mov", "ape", "wv"],
            FileFamily::Archives => &["zip", "7z", "rar", "gz", "bz2", "xz"],
            FileFamily::Everything => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryConfig {
    pub source: Source,
    /// Dossier (sur un autre disque) où PhotoRec crée ses `recup_dir.N` et son journal.
    pub destination: PathBuf,
    pub families: Vec<FileFamily>,
    /// Mode « paranoid » de PhotoRec (actif par défaut chez lui) : vérifie la structure des
    /// fichiers et jette ceux qui sont incohérents. Désactivé : plus de fichiers, dont des
    /// fichiers corrompus.
    pub paranoid: bool,
}

/// Arguments de PhotoRec (sans le programme) :
/// `/log /d <dest>/recup_dir /cmd <device> partition_none,options,paranoid,fileopt,...,search`.
///
/// - `partition_none` : le périphérique est lu comme un seul bloc, sans chercher de table de
///   partitions. Sans lui, en mode `/cmd`, PhotoRec sélectionne la **première partition** d'un
///   disque partitionné (phcli.c, `menu_photorec_cli`) au lieu du disque entier.
/// - Familles vides : traité comme « tout » (l'appelant doit refuser une liste vide avant).
/// - Chemin non UTF-8 : converti avec perte ; `RecoveryJob::start` le refuse avant.
pub fn build_args(config: &RecoveryConfig) -> Vec<String> {
    let recup = config.destination.join(RECUP_DIR_PREFIX);
    vec![
        "/log".to_string(),
        "/d".to_string(),
        recup.to_string_lossy().into_owned(),
        "/cmd".to_string(),
        config.source.device(),
        command_list(config),
    ]
}

/// Formats désactivés même en mode « tout » : trop de faux positifs, aucun intérêt à l'achat.
const NOISY_FORMATS: [&str; 1] = ["dovecot"];

/// Liste de commandes après le périphérique, séparées par des virgules.
fn command_list(config: &RecoveryConfig) -> String {
    let mut cmds: Vec<&str> = vec!["partition_none", "options"];
    cmds.push(if config.paranoid {
        "paranoid"
    } else {
        "paranoid_no"
    });
    cmds.push("fileopt");
    let everything =
        config.families.is_empty() || config.families.contains(&FileFamily::Everything);
    if everything {
        cmds.extend(["everything", "enable"]);
        // Faux positifs : PhotoRec prend les zones remplies de zéros pour des index de courriel
        // Dovecot (des centaines de fichiers inutiles sur une image de test de 64 Mo).
        cmds.extend(NOISY_FORMATS.iter().flat_map(|f| [*f, "disable"]));
    } else {
        cmds.extend(["everything", "disable"]);
        for format in formats_of(&config.families) {
            cmds.extend([format, "enable"]);
        }
    }
    cmds.push("search");
    cmds.join(",")
}

/// Identifiants PhotoRec des familles, sans doublon, dans l'ordre d'apparition.
fn formats_of(families: &[FileFamily]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for format in families.iter().flat_map(|f| f.photorec_formats()) {
        if !out.contains(format) {
            out.push(format);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_deduplicated_in_order() {
        let f = formats_of(&[FileFamily::Videos, FileFamily::Audio]);
        assert_eq!(
            f,
            vec!["mov", "riff", "mkv", "asf", "mpg", "mp3", "flac", "ogg", "ape", "wv"]
        );
    }

    #[test]
    fn every_family_but_everything_has_formats() {
        for fam in [
            FileFamily::Photos,
            FileFamily::Documents,
            FileFamily::Videos,
            FileFamily::Audio,
            FileFamily::Archives,
        ] {
            let formats = fam.photorec_formats();
            assert!(!formats.is_empty());
            // Une virgule dans un identifiant casserait la liste de commandes.
            assert!(formats.iter().all(|f| !f.contains(',') && !f.is_empty()));
        }
    }
}
