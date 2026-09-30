//! Stockage de la partition utilisateur (`adb shell df /data`).
//!
//! Deux formats :
//! - toybox (Android 6 et plus), en blocs de 1 Kio :
//!   `Filesystem 1K-blocks Used Available Use% Mounted on`
//! - toolbox (Android 5 et moins), en tailles lisibles :
//!   `Filesystem Size Used Free Blksize` avec des valeurs comme `12.5G`.
//!
//! L'usure de la mémoire flash (UFS/eMMC) exige le root : elle n'est pas mesurée.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StorageInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Clone, Copy)]
enum Unit {
    /// Nombre de blocs de cette taille en octets.
    Blocks(u64),
    /// Tailles avec suffixe K, M, G, T (puissances de 1024).
    Human,
}

/// Analyse la sortie de `df /data`. `None` si l'en-tête ou la ligne de données manque.
pub fn parse_df(text: &str) -> Option<StorageInfo> {
    let mut lines = text.lines();
    let header = lines.find(|l| l.trim_start().starts_with("Filesystem"))?;
    let unit = if let Some(size) = header
        .split_whitespace()
        .find_map(|t| t.strip_suffix("-blocks"))
    {
        Unit::Blocks(block_size(size)?)
    } else if header.contains("Size") {
        Unit::Human
    } else {
        return None;
    };
    // Un nom de système de fichiers long peut être renvoyé seul sur sa ligne : on enchaîne les
    // jetons des lignes suivantes plutôt que de lire une seule ligne.
    let tokens: Vec<&str> = lines.flat_map(str::split_whitespace).take(4).collect();
    if tokens.len() < 4 {
        return None;
    }
    let value = |t: &str| match unit {
        Unit::Blocks(size) => t.parse::<u64>().ok()?.checked_mul(size),
        Unit::Human => parse_human_size(t),
    };
    Some(StorageInfo {
        total_bytes: value(tokens[1])?,
        used_bytes: value(tokens[2])?,
        free_bytes: value(tokens[3])?,
    })
}

/// `1K` → 1024, `512` → 512.
fn block_size(text: &str) -> Option<u64> {
    match text.strip_suffix(['K', 'k']) {
        Some(n) => n.parse::<u64>().ok()?.checked_mul(1024),
        None => text.parse().ok(),
    }
}

/// `12.5G` → octets (puissances de 1024). Un nombre sans suffixe est en octets.
fn parse_human_size(text: &str) -> Option<u64> {
    let (number, multiplier) = match text.chars().last()? {
        'K' | 'k' => (&text[..text.len() - 1], 1u64 << 10),
        'M' => (&text[..text.len() - 1], 1 << 20),
        'G' => (&text[..text.len() - 1], 1 << 30),
        'T' => (&text[..text.len() - 1], 1 << 40),
        _ => (text, 1),
    };
    let value: f64 = number.parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some((value * multiplier as f64).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(parse_human_size("0.0K"), Some(0));
        assert_eq!(parse_human_size("1.5M"), Some(1_572_864));
        assert_eq!(parse_human_size("2G"), Some(2_147_483_648));
        assert_eq!(parse_human_size("abc"), None);
    }

    #[test]
    fn missing_header_or_data() {
        assert_eq!(parse_df(""), None);
        assert_eq!(parse_df("df: /data: Permission denied\n"), None);
        assert_eq!(
            parse_df("Filesystem 1K-blocks Used Available Use% Mounted on\n"),
            None
        );
    }
}
