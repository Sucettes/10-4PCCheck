//! Refus de lancement : jamais de dossier créé quand la destination est sur la source.

use std::path::{Path, PathBuf};

use pccheck_recovery::{
    locate_destination, DestinationLocation, DiskId, FileFamily, RecoveryConfig, RecoveryError,
    RecoveryJob, Source,
};

fn dest(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "pccheck-recovery-job-{}-{name}",
        std::process::id()
    ))
}

#[test]
fn destination_on_source_disk_is_refused_before_any_write() {
    let d = dest("meme-disque");
    // La source est le disque du dossier temporaire lui-même.
    let location = locate_destination(&d).unwrap();
    let source_disk = match &location {
        DestinationLocation::Disks { disks } => disks[0].clone(),
        // Dossier temporaire en mémoire (tmpfs) : la règle refuse aussi, pour une autre raison.
        _ => DiskId::Block("inconnu".into()),
    };
    let config = RecoveryConfig {
        source: Source::Disk { disk: source_disk },
        destination: d.clone(),
        families: vec![FileFamily::Photos],
        paranoid: true,
    };
    let err = RecoveryJob::start(config, Path::new("photorec-inexistant"))
        .err()
        .expect("le lancement aurait dû être refusé");
    match location {
        DestinationLocation::Disks { .. } => {
            assert!(matches!(err, RecoveryError::SameDisk { .. }), "{err:?}")
        }
        _ => assert!(
            matches!(err, RecoveryError::DestinationDiskUnknown { .. }),
            "{err:?}"
        ),
    }
    assert!(!d.exists(), "aucun dossier ne doit être créé sur la source");
    // Message lisible pour l'interface, sérialisé avec son code.
    let json = serde_json::to_value(&err).unwrap();
    assert!(json["code"].is_string());
}

#[test]
fn empty_family_list_is_refused() {
    let config = RecoveryConfig {
        source: Source::Disk {
            disk: DiskId::PhysicalDrive(99),
        },
        destination: dest("vide"),
        families: vec![],
        paranoid: true,
    };
    assert_eq!(
        RecoveryJob::start(config, Path::new("photorec-inexistant")).err(),
        Some(RecoveryError::NoFileFamily)
    );
}
