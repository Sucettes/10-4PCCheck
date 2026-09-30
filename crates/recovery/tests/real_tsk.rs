//! Essai avec la vraie Sleuth Kit sur une image FAT16 (aucun disque physique, aucun droit admin).
//! Ignoré par défaut : il faut les binaires et l'image de test.
//!
//! ```text
//! python tools/dev/make-fat-image.py %TEMP%\pccheck-tsk-test\fat.img
//! set PCCHECK_TSK=...\tools\windows\sleuthkit\tsk_recover.exe
//! set PCCHECK_TEST_FAT=%TEMP%\pccheck-tsk-test\fat.img
//! cargo test -p pccheck-recovery --test real_tsk -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pccheck_recovery::tsk::{extract_files, SelectedFile};
use pccheck_recovery::{list_deleted, locate_tsk, TskJob};

#[test]
#[ignore = "exige The Sleuth Kit et une image de test (voir l'en-tête)"]
fn tsk_lists_and_recovers_deleted_files_with_names() {
    let image = PathBuf::from(std::env::var_os("PCCHECK_TEST_FAT").expect("PCCHECK_TEST_FAT"));
    let tsk = locate_tsk(&[]).expect("PCCHECK_TSK");

    let list = list_deleted(&tsk, &image.display().to_string()).expect("fls");
    let names: Vec<&str> = list.files.iter().map(|f| f.path.as_str()).collect();
    println!("{names:?}");
    assert!(names.contains(&"_HOTO.PNG") && names.contains(&"_APPORT.PDF"));
    assert!(
        !names.contains(&"GARDE.TXT"),
        "seulement les fichiers supprimés"
    );

    let dest = image.with_file_name(format!("recup-{}", std::process::id()));
    let mut job = TskJob::spawn(
        &tsk.join(if cfg!(windows) {
            "tsk_recover.exe"
        } else {
            "tsk_recover"
        }),
        &[image.display().to_string()],
        &dest,
    )
    .expect("tsk_recover");
    let deadline = Instant::now() + Duration::from_secs(60);
    let p = loop {
        let p = job.progress();
        if !p.running || Instant::now() > deadline {
            break p;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    // La sortie standard est lue par un fil : on lui laisse le temps de finir.
    std::thread::sleep(Duration::from_millis(300));
    let p2 = job.progress();
    println!("{p2:?}");
    assert_eq!(p.files_found, 2);
    assert_eq!(p2.reported, Some(2));
    assert_eq!(
        std::fs::metadata(dest.join("_HOTO.PNG")).unwrap().len(),
        8455
    );
    let _ = std::fs::remove_dir_all(&dest);

    // Récupération ciblée (icat) : un seul fichier, et un chemin piégé qui doit rester dans dest.
    let picked = image.with_file_name(format!("choix-{}", std::process::id()));
    std::fs::create_dir_all(&picked).unwrap();
    let files = [
        SelectedFile {
            inode: "4".into(),
            path: "Photos/_HOTO.PNG".into(),
        },
        SelectedFile {
            inode: "5".into(),
            path: "../../évasion/_APPORT.PDF".into(),
        },
    ];
    let r = extract_files(
        &tsk.join(if cfg!(windows) { "icat.exe" } else { "icat" }),
        &image.display().to_string(),
        &files,
        &picked,
    );
    println!("{r:?}");
    assert_eq!(r.recovered, 2);
    assert_eq!(
        std::fs::metadata(picked.join("Photos").join("_HOTO.PNG"))
            .unwrap()
            .len(),
        8455
    );
    assert!(picked.join("évasion").join("_APPORT.PDF").is_file());
    let _ = std::fs::remove_dir_all(&picked);
}
