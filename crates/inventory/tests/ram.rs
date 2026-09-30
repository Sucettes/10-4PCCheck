//! Test RAM partiel sur de petites tailles (64 Mio au plus).

use std::sync::atomic::AtomicBool;

use pccheck_inventory::ram::{BLOCK_BYTES, PATTERNS};
use pccheck_inventory::{available_memory_bytes, run_ram_test, RamPhase};

#[test]
fn available_memory_is_plausible() {
    if cfg!(any(windows, target_os = "linux")) {
        let avail = available_memory_bytes().expect("mémoire disponible lisible");
        assert!(avail > 16 << 20, "au moins 16 Mio libres");
    }
}

#[test]
fn sixty_four_mebibytes_pass_every_pattern() {
    let cancel = AtomicBool::new(false);
    let mut progress = Vec::new();
    let r = run_ram_test(BLOCK_BYTES, &cancel, |p| progress.push(p));
    assert!(!r.cancelled);
    assert!(!r.allocation_limited);
    assert_eq!(r.tested_bytes, 64 << 20);
    assert_eq!(r.passes as usize, PATTERNS.len());
    assert_eq!(r.errors, 0);
    assert_eq!(r.first_error_offset, None);
    // 1 allocation + (écriture + vérification) par motif, un bloc chacun.
    assert_eq!(progress.len(), 1 + 2 * PATTERNS.len());
    assert_eq!(progress[0].phase, RamPhase::Allocating);
    let last = progress.last().unwrap();
    assert_eq!(last.phase, RamPhase::Verifying);
    assert_eq!(last.bytes_done, last.bytes_total);
    serde_json::to_string(&r).expect("sérialisable");
}

#[test]
fn odd_size_is_rounded_down_to_whole_words() {
    let cancel = AtomicBool::new(false);
    let r = run_ram_test(1_003, &cancel, |_| {});
    assert_eq!(r.tested_bytes, 1_000);
    assert_eq!(r.errors, 0);
    assert_eq!(r.passes as usize, PATTERNS.len());
}

#[test]
fn zero_bytes_and_cancel() {
    let cancel = AtomicBool::new(false);
    let r = run_ram_test(0, &cancel, |_| {});
    assert_eq!((r.tested_bytes, r.passes), (0, 0));

    let cancelled = AtomicBool::new(true);
    let r = run_ram_test(BLOCK_BYTES, &cancelled, |_| {});
    assert!(r.cancelled);
    assert_eq!(r.passes, 0);
}
