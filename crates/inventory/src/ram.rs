//! Test RAM **partiel**, depuis le système d'exploitation.
//!
//! Limite (plan §3, « Test de la RAM ») : on ne teste que la mémoire que l'OS accepte de nous
//! donner, pas celle qu'occupent le noyau, les pilotes et les autres programmes. Les adresses
//! sont virtuelles : une erreur ne désigne pas une barrette. Un test complet exige de démarrer
//! sur MemTest86+. Ce test repère une barrette franchement défectueuse, pas une erreur rare.
//!
//! Déroulement : allocation par blocs de 64 Mio jusqu'à `max_bytes`, puis pour chaque motif,
//! écriture de **tous** les blocs avant de tous les relire. Le volume dépasse largement les
//! caches du processeur, donc la relecture vient bien de la RAM. Écritures et lectures sont
//! volatiles : le compilateur ne peut ni les supprimer ni déduire la valeur relue.

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Taille d'un bloc alloué.
pub const BLOCK_BYTES: u64 = 64 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RamPattern {
    /// 0x5555… puis 0xAAAA… : bits voisins opposés, puis inversés.
    Alternating55,
    AlternatingAa,
    /// Un seul bit à 1 qui se décale d'un mot à l'autre (lignes de données collées à 0).
    WalkingOnes,
    /// Un seul bit à 0 (lignes collées à 1).
    WalkingZeros,
    /// Chaque mot contient sa propre adresse (erreurs d'adressage : deux adresses, une cellule).
    OwnAddress,
}

/// Motifs dans l'ordre d'exécution. Une passe = un motif écrit puis vérifié partout.
pub const PATTERNS: [RamPattern; 5] = [
    RamPattern::Alternating55,
    RamPattern::AlternatingAa,
    RamPattern::WalkingOnes,
    RamPattern::WalkingZeros,
    RamPattern::OwnAddress,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RamPhase {
    Allocating,
    Writing,
    Verifying,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RamProgress {
    pub phase: RamPhase,
    /// Motif en cours, `None` pendant l'allocation.
    pub pattern: Option<RamPattern>,
    /// Passe en cours, de 1 à `total_passes` (0 pendant l'allocation).
    pub pass: u32,
    pub total_passes: u32,
    /// Octets traités dans la phase en cours, sur `bytes_total`.
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RamTestResult {
    pub requested_bytes: u64,
    /// Mémoire réellement allouée et testée.
    pub tested_bytes: u64,
    /// L'OS a refusé une allocation avant `requested_bytes` : test fait sur ce qui a été obtenu.
    pub allocation_limited: bool,
    /// Passes complètes (motif écrit et vérifié sur toute la mémoire testée).
    pub passes: u32,
    /// Mots de 64 bits relus différents de ce qui avait été écrit.
    pub errors: u64,
    /// Position de la première erreur dans la zone testée, en octets (adresse virtuelle
    /// relative, pas une adresse physique).
    pub first_error_offset: Option<u64>,
    pub duration_ms: u64,
    pub cancelled: bool,
}

/// Mémoire physique disponible selon l'OS, en octets. `None` si illisible.
pub fn available_memory_bytes() -> Option<u64> {
    crate::platform::available_memory_bytes()
}

/// Teste jusqu'à `max_bytes` de mémoire libre (l'appelant passe environ 50 % de
/// `available_memory_bytes()` pour ne pas faire swapper la machine). Bloquant ; `cancel` est
/// vérifié entre deux blocs. `on_progress` est appelé après chaque bloc.
pub fn run_ram_test(
    max_bytes: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(RamProgress),
) -> RamTestResult {
    let start = Instant::now();
    let mut result = RamTestResult {
        requested_bytes: max_bytes,
        tested_bytes: 0,
        allocation_limited: false,
        passes: 0,
        errors: 0,
        first_error_offset: None,
        duration_ms: 0,
        cancelled: false,
    };
    let total_passes = u32::try_from(PATTERNS.len()).unwrap_or(u32::MAX);
    let is_cancelled = || cancel.load(Ordering::Relaxed);

    let mut blocks: Vec<Vec<u64>> = Vec::new();
    while result.tested_bytes < max_bytes {
        if is_cancelled() {
            result.cancelled = true;
            break;
        }
        let size = (max_bytes - result.tested_bytes).min(BLOCK_BYTES);
        let Ok(words) = usize::try_from(size / 8) else {
            break;
        };
        if words == 0 {
            break;
        }
        match allocate_block(words) {
            Some(block) => {
                result.tested_bytes += bytes_of(block.len());
                blocks.push(block);
            }
            None => {
                result.allocation_limited = true;
                break;
            }
        }
        on_progress(RamProgress {
            phase: RamPhase::Allocating,
            pattern: None,
            pass: 0,
            total_passes,
            bytes_done: result.tested_bytes,
            bytes_total: max_bytes,
            errors: 0,
        });
    }

    'passes: for (i, &pattern) in PATTERNS.iter().enumerate() {
        let pass = u32::try_from(i + 1).unwrap_or(u32::MAX);
        for phase in [RamPhase::Writing, RamPhase::Verifying] {
            let mut done = 0u64;
            for block in &mut blocks {
                if result.cancelled || is_cancelled() {
                    result.cancelled = true;
                    break 'passes;
                }
                if phase == RamPhase::Writing {
                    write_block(block, pattern);
                } else {
                    let (bad, first) = verify_block(block, pattern);
                    result.errors += bad;
                    if result.first_error_offset.is_none() {
                        result.first_error_offset = first.map(|w| done + bytes_of(w));
                    }
                }
                done += bytes_of(block.len());
                on_progress(RamProgress {
                    phase,
                    pattern: Some(pattern),
                    pass,
                    total_passes,
                    bytes_done: done,
                    bytes_total: result.tested_bytes,
                    errors: result.errors,
                });
            }
        }
        if !blocks.is_empty() {
            result.passes = pass;
        }
    }

    drop(blocks);
    result.duration_ms = millis(start.elapsed());
    result
}

/// Alloue sans faire planter le programme si l'OS refuse (`try_reserve_exact` au lieu d'un
/// `Vec` qui avorterait), puis remplit de zéros pour que l'OS fournisse vraiment les pages.
fn allocate_block(words: usize) -> Option<Vec<u64>> {
    let mut v = Vec::new();
    v.try_reserve_exact(words).ok()?;
    v.resize(words, 0);
    Some(v)
}

fn pattern_word(pattern: RamPattern, index: usize, address: usize) -> u64 {
    let bit = 1u64 << (index % 64);
    match pattern {
        RamPattern::Alternating55 => 0x5555_5555_5555_5555,
        RamPattern::AlternatingAa => 0xAAAA_AAAA_AAAA_AAAA,
        RamPattern::WalkingOnes => bit,
        RamPattern::WalkingZeros => !bit,
        RamPattern::OwnAddress => address as u64,
    }
}

fn write_block(block: &mut [u64], pattern: RamPattern) {
    for (i, word) in block.iter_mut().enumerate() {
        let value = pattern_word(pattern, i, ptr::from_mut(word) as usize);
        // SAFETY : `word` est une référence exclusive, valide et alignée sur un u64.
        unsafe { ptr::write_volatile(word, value) };
    }
}

/// Nombre de mots faux et index du premier.
fn verify_block(block: &[u64], pattern: RamPattern) -> (u64, Option<usize>) {
    let mut bad = 0u64;
    let mut first = None;
    for (i, word) in block.iter().enumerate() {
        // SAFETY : `word` est une référence partagée, valide et alignée sur un u64.
        let got = unsafe { ptr::read_volatile(word) };
        if got != pattern_word(pattern, i, ptr::from_ref(word) as usize) {
            bad += 1;
            first.get_or_insert(i);
        }
    }
    (bad, first)
}

fn bytes_of(words: usize) -> u64 {
    (words as u64).saturating_mul(8)
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_detects_a_corrupted_word() {
        let mut block = vec![0u64; 1024];
        write_block(&mut block, RamPattern::WalkingOnes);
        assert_eq!(verify_block(&block, RamPattern::WalkingOnes), (0, None));
        block[70] ^= 1 << 3;
        block[900] = 0;
        assert_eq!(verify_block(&block, RamPattern::WalkingOnes), (2, Some(70)));
    }

    #[test]
    fn own_address_pattern_differs_per_word() {
        let mut block = vec![0u64; 4];
        write_block(&mut block, RamPattern::OwnAddress);
        assert_eq!(block[1] - block[0], 8);
        assert_eq!(verify_block(&block, RamPattern::OwnAddress), (0, None));
    }

    #[test]
    fn walking_patterns_are_complements() {
        for i in [0, 1, 63, 64, 1000] {
            assert_eq!(
                pattern_word(RamPattern::WalkingOnes, i, 0),
                !pattern_word(RamPattern::WalkingZeros, i, 0)
            );
        }
    }
}
