//! Ligne de commande PhotoRec générée pour plusieurs configurations.

use std::path::PathBuf;

use pccheck_recovery::{build_args, DiskId, FileFamily, RecoveryConfig, Source};

fn config(source: Source, families: Vec<FileFamily>, paranoid: bool) -> RecoveryConfig {
    RecoveryConfig {
        source,
        destination: PathBuf::from("dest").join("recup"),
        families,
        paranoid,
    }
}

fn recup_arg() -> String {
    PathBuf::from("dest")
        .join("recup")
        .join("recup_dir")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn whole_windows_disk_everything() {
    let c = config(
        Source::Disk {
            disk: DiskId::PhysicalDrive(1),
        },
        vec![FileFamily::Everything],
        true,
    );
    assert_eq!(
        build_args(&c),
        vec![
            "/log".to_string(),
            "/d".into(),
            recup_arg(),
            "/cmd".into(),
            r"\\.\PhysicalDrive1".into(),
            "partition_none,options,paranoid,fileopt,everything,enable,search".into(),
        ]
    );
}

#[test]
fn linux_partition_photos_without_paranoid() {
    let c = config(
        Source::Partition {
            device: "/dev/sdb1".into(),
            disk: DiskId::Block("sdb".into()),
        },
        vec![FileFamily::Photos],
        false,
    );
    let args = build_args(&c);
    assert_eq!(args[4], "/dev/sdb1");
    assert_eq!(
        args[5],
        "partition_none,options,paranoid_no,fileopt,everything,disable,\
         jpg,enable,png,enable,gif,enable,bmp,enable,tif,enable,mov,enable,psd,enable,\
         crw,enable,raf,enable,orf,enable,rw2,enable,mrw,enable,x3f,enable,search"
    );
}

#[test]
fn windows_volume_documents_and_archives_share_zip_once() {
    let c = config(
        Source::Partition {
            device: r"\\.\E:".into(),
            disk: DiskId::PhysicalDrive(2),
        },
        vec![FileFamily::Documents, FileFamily::Archives],
        true,
    );
    let args = build_args(&c);
    assert_eq!(args[4], r"\\.\E:");
    assert_eq!(
        args[5],
        "partition_none,options,paranoid,fileopt,everything,disable,\
         pdf,enable,doc,enable,zip,enable,txt,enable,tx?,enable,\
         7z,enable,rar,enable,gz,enable,bz2,enable,xz,enable,search"
    );
    assert_eq!(args[5].matches("zip,").count(), 1);
}

#[test]
fn everything_overrides_other_families() {
    let c = config(
        Source::Disk {
            disk: DiskId::Block("nvme0n1".into()),
        },
        vec![FileFamily::Videos, FileFamily::Everything],
        true,
    );
    let args = build_args(&c);
    assert_eq!(args[4], "/dev/nvme0n1");
    assert!(args[5].ends_with("fileopt,everything,enable,search"));
    assert!(!args[5].contains("mov"));
}

#[test]
fn command_order_follows_the_documentation() {
    // Ordre conseillé par le wiki : type de partition, options, fileopt, search en dernier.
    let c = config(
        Source::Disk {
            disk: DiskId::PhysicalDrive(0),
        },
        vec![FileFamily::Audio],
        true,
    );
    let cmds = build_args(&c).pop().unwrap_or_default();
    let pos = |w: &str| cmds.find(w).unwrap_or(usize::MAX);
    assert!(pos("partition_none") < pos("options"));
    assert!(pos("options") < pos("fileopt"));
    assert!(cmds.ends_with(",search"));
    assert!(
        !cmds.contains(' '),
        "la liste de commandes doit rester un seul argument"
    );
}

#[test]
fn config_round_trips_through_json() {
    let c = config(
        Source::Partition {
            device: "/dev/mmcblk0p1".into(),
            disk: DiskId::Block("mmcblk0".into()),
        },
        vec![FileFamily::Photos, FileFamily::Videos],
        false,
    );
    let json = serde_json::to_string(&c).unwrap();
    assert!(json.contains(r#""kind":"partition""#));
    assert!(json.contains(r#""families":["photos","videos"]"#));
    let back: RecoveryConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back, c);
}
