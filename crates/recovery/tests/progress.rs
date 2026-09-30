//! Comptage des fichiers trouvés sur un dossier temporaire rempli de faux fichiers PhotoRec,
//! et analyse d'un `photorec.log` reconstruit.

use std::fs;
use std::path::{Path, PathBuf};

use pccheck_recovery::{count_found, list_found, parse_log};

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pccheck-recovery-test-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, size: usize) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(name), vec![0u8; size]).unwrap();
}

/// recup_dir.1 : d'une exécution précédente ; recup_dir.2 et .3 : exécution suivie.
fn fake_destination(name: &str) -> PathBuf {
    let dest = tempdir(name);
    write(&dest.join("recup_dir.1"), "f0000100.jpg", 1000);
    write(&dest.join("recup_dir.1"), "report.xml", 50);
    write(&dest.join("recup_dir.2"), "f0002000.jpg", 300);
    write(&dest.join("recup_dir.2"), "f0002100_Lettre.doc", 200);
    write(&dest.join("recup_dir.2"), "f0001500.PDF", 100);
    write(&dest.join("recup_dir.3"), "f0000050.mp4", 4000);
    write(&dest.join("recup_dir.3"), "f0000900.jpg", 10);
    // Bruit : ni dossier recup_dir, ni fichier au bon endroit.
    write(&dest.join("autre"), "f0009999.jpg", 1);
    write(&dest, "photorec.log", 10);
    fs::create_dir_all(dest.join("recup_dir.x")).unwrap();
    dest
}

#[test]
fn counts_only_this_run_and_skips_report() {
    let dest = fake_destination("count");
    let s = count_found(&dest, 2).unwrap();
    assert_eq!(s.files_found, 5);
    assert_eq!(s.bytes_found, 300 + 200 + 100 + 4000 + 10);
    assert_eq!(s.by_extension.get("jpg"), Some(&2));
    assert_eq!(
        s.by_extension.get("pdf"),
        Some(&1),
        "extension en minuscules"
    );
    assert_eq!(s.by_extension.get("doc"), Some(&1));
    assert_eq!(s.by_extension.get("mp4"), Some(&1));
    // Plus récent : dernier dossier, secteur le plus haut.
    let names: Vec<&str> = s.last_files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "f0000900.jpg",
            "f0000050.mp4",
            "f0002100_Lettre.doc",
            "f0002000.jpg",
            "f0001500.PDF"
        ]
    );

    let all = count_found(&dest, 1).unwrap();
    assert_eq!(all.files_found, 6, "report.xml exclu");
}

#[test]
fn last_files_are_capped_at_ten() {
    let dest = tempdir("cap");
    for i in 0..25 {
        write(&dest.join("recup_dir.1"), &format!("f{:07}.jpg", i * 8), 1);
    }
    let s = count_found(&dest, 1).unwrap();
    assert_eq!(s.files_found, 25);
    assert_eq!(s.last_files.len(), 10);
    assert_eq!(s.last_files[0].name, "f0000192.jpg");
}

#[test]
fn missing_destination_counts_nothing() {
    let dest = std::env::temp_dir().join("pccheck-recovery-inexistant-xyz");
    let s = count_found(&dest, 1).unwrap();
    assert_eq!(s.files_found, 0);
    assert!(list_found(&dest, 5).unwrap().is_empty());
}

#[test]
fn list_found_is_newest_first_and_limited() {
    let dest = fake_destination("list");
    let files = list_found(&dest, 3).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["f0000900.jpg", "f0000050.mp4", "f0002100_Lettre.doc"]
    );
    assert_eq!(files[1].extension, "mp4");
    assert_eq!(files[1].size, 4000);
    assert!(files[1]
        .path
        .ends_with(Path::new("recup_dir.3").join("f0000050.mp4")));
    assert_eq!(list_found(&dest, 100).unwrap().len(), 6);
}

/// Journal reconstruit d'après les `log_info` des sources de PhotoRec 7.x : une exécution
/// précédente, puis l'exécution suivie (fichiers anonymisés, aucune donnée réelle).
const LOG: &str = "\
\n\nMon Sep 28 10:00:00 2026\n\
Command line: PhotoRec /log /d /mnt/cle/ancien/recup_dir /cmd /dev/sdz partition_none,search\n\
Pass 1 (blocksize=4096) STATUS_EXT2_ON\n\
Total: 99 files found\n\
PhotoRec exited normally.\n\
\n\nTue Sep 29 14:00:00 2026\n\
Command line: PhotoRec /log /d /mnt/cle/recup/recup_dir /cmd /dev/sdb partition_none,options,paranoid,fileopt,everything,enable,search\n\
\n\
PhotoRec 7.2, Data Recovery Utility, February 2024\n\
OS: Linux, kernel 6.8.0 (#1 SMP) x86_64\n\
\n\
Analyse\n\
Pass 0 (blocksize=512) STATUS_FIND_OFFSET\n\
/mnt/cle/recup/recup_dir.1/f0002048.jpg\t 2048-2111\n\
Elapsed time 0h00m05s\n\
Pass 1 (blocksize=4096) STATUS_EXT2_ON\n\
/mnt/cle/recup/recup_dir.1/f0004096.jpg\t 4096-4351\n\
/mnt/cle/recup/recup_dir.1/f0008192.pdf\t 8192-8199\n\
Elapsed time 0h02m03s\n\
Pass 1 +3 files\n\
jpg: 2/2 recovered\n\
pdf: 1/1 recovered\n\
Total: 3 files found\n\
\n\
Cannot write to file /mnt/cle/recup/recup_dir.1/f0010000.mov: No space left on device\n\
PhotoRec exited normally.\n";

#[test]
fn log_of_last_run_is_parsed() {
    let s = parse_log(LOG);
    assert_eq!(s.pass, Some(1));
    assert_eq!(s.elapsed_s, Some(123));
    assert_eq!(s.total_found, Some(3));
    assert_eq!(s.recovered_by_format.get("jpg"), Some(&(2, 2)));
    assert_eq!(s.recovered_by_format.get("pdf"), Some(&(1, 1)));
    assert!(s.finished_normally);
    assert_eq!(s.errors.len(), 1);
    assert!(s.errors[0].starts_with("Cannot write to file"));
}

#[test]
fn interrupted_log_has_no_normal_end() {
    let text = "Command line: PhotoRec /log /cmd /dev/sdb search\n\
                Pass 1 (blocksize=4096) STATUS_EXT2_ON\n\
                SIGTERM detected! PhotoRec has been killed.\n";
    let s = parse_log(text);
    assert!(!s.finished_normally);
    assert_eq!(s.total_found, None);
    assert_eq!(
        s.errors,
        vec!["SIGTERM detected! PhotoRec has been killed."]
    );
}

#[test]
fn syntax_error_is_reported() {
    let text = "Command line: PhotoRec /cmd /dev/sdb fileopt,jpeg,enable,search\n\
                Syntax error in command line: jpeg,enable,search\n\
                PhotoRec exited normally.\n";
    let s = parse_log(text);
    assert_eq!(s.errors.len(), 1);
    assert!(s.errors[0].contains("Syntax error"));
}
