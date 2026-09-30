//! Essai avec le vrai PhotoRec sur une image disque (aucun disque physique, aucun droit admin).
//! Ignoré par défaut : il faut le binaire et une image de test.
//!
//! ```text
//! python tools/dev/make-photorec-image.py %TEMP%\pccheck-photorec-test\image.dd
//! set PCCHECK_PHOTOREC=...\tools\windows\testdisk\photorec_win.exe
//! set PCCHECK_TEST_IMAGE=%TEMP%\pccheck-photorec-test\image.dd
//! cargo test -p pccheck-recovery --test real_photorec -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pccheck_recovery::{DiskId, FileFamily, RecoveryConfig, RecoveryJob, Source};

#[test]
#[ignore = "exige photorec et une image de test (voir l'en-tête)"]
fn photorec_finds_files_in_image() {
    let photorec = PathBuf::from(std::env::var_os("PCCHECK_PHOTOREC").expect("PCCHECK_PHOTOREC"));
    let image = PathBuf::from(std::env::var_os("PCCHECK_TEST_IMAGE").expect("PCCHECK_TEST_IMAGE"));
    let dest = image.with_file_name(format!("recup-{}", std::process::id()));
    let config = RecoveryConfig {
        // Une image n'est sur aucun disque physique : un numéro inexistant passe la règle
        // « destination sur un autre disque ».
        source: Source::Partition {
            device: image.display().to_string(),
            disk: DiskId::PhysicalDrive(999),
        },
        destination: dest.clone(),
        families: vec![FileFamily::Everything],
        paranoid: false,
    };
    let job = RecoveryJob::start(config, &photorec).expect("lancement de PhotoRec");
    println!("arguments : {:?}", job.args());
    let deadline = Instant::now() + Duration::from_secs(120);
    let last = loop {
        let p = job.progress();
        if !p.running || Instant::now() > deadline {
            break p;
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    println!("{last:#?}");
    assert!(!last.running, "PhotoRec ne s'est pas terminé en 2 min");
    assert!(last.files_found >= 3, "au moins le PNG, le PDF et le ZIP");
    for ext in ["png", "pdf", "zip"] {
        assert!(
            last.by_extension.contains_key(ext),
            "{ext} absent : {:?}",
            last.by_extension
        );
    }
    let _ = std::fs::remove_dir_all(&dest);
}
