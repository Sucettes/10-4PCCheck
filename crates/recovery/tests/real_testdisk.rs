//! Essai du vrai TestDisk dans un pseudo-terminal, sur une image (aucun disque physique).
//! Ignoré par défaut : il faut le binaire et l'image FAT de test.
//!
//! ```text
//! python tools/dev/make-fat-image.py %TEMP%\pccheck-tsk-test\fat.img
//! set PCCHECK_TESTDISK=...\tools\windows\testdisk\testdisk_win.exe
//! set PCCHECK_TEST_FAT=%TEMP%\pccheck-tsk-test\fat.img
//! cargo test -p pccheck-recovery --test real_testdisk -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use pccheck_recovery::{PtySession, Utf8Stream};

/// Lit l'écran jusqu'à voir `needle` ou jusqu'au délai ; rend tout le texte reçu.
fn read_until(rx: &mpsc::Receiver<String>, needle: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut screen = String::new();
    while Instant::now() < deadline && !screen.contains(needle) {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            screen.push_str(&chunk);
        }
    }
    screen
}

#[test]
#[ignore = "exige TestDisk et une image de test (voir l'en-tête)"]
fn testdisk_runs_in_pseudo_terminal() {
    let exe = PathBuf::from(std::env::var_os("PCCHECK_TESTDISK").expect("PCCHECK_TESTDISK"));
    let image = PathBuf::from(std::env::var_os("PCCHECK_TEST_FAT").expect("PCCHECK_TEST_FAT"));
    let cwd = std::env::temp_dir();
    // Le manifeste de TestDisk exige l'administrateur ; sur une image, les droits normaux suffisent.
    let (mut session, mut reader) = PtySession::spawn(
        &exe,
        &[image.display().to_string()],
        &cwd,
        100,
        30,
        &[("__COMPAT_LAYER", "RunAsInvoker")],
    )
    .expect("lancement de TestDisk");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut utf8 = Utf8Stream::default();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(utf8.push(&buf[..n])).is_err() {
                break;
            }
        }
    });

    // ConPTY demande d'abord la position du curseur (ESC[6n) et attend la réponse avant d'afficher
    // quoi que ce soit : un vrai terminal (xterm.js dans l'app) répond seul, ici on répond à la main.
    let hello = read_until(&rx, "\x1b[6n", Duration::from_secs(10));
    if hello.contains("\x1b[6n") {
        session.write(b"\x1b[1;1R").unwrap();
    }
    // Premier écran de TestDisk : création du journal ([ Create ] / [ Append ] / [ No Log ]).
    let first = read_until(&rx, "Log", Duration::from_secs(15));
    println!("--- écran 1 ({} octets) : {first:?}", first.len());
    assert!(first.contains("TestDisk"), "bannière TestDisk absente");

    // Flèche droite ×2 (« No Log ») puis Entrée : écran du choix du support.
    session.write(b"\x1b[C\x1b[C\r").unwrap();
    let second = read_until(&rx, "Proceed", Duration::from_secs(10));
    println!("--- écran 2 ---\n{second}");
    assert!(
        second.contains("Proceed") || second.contains("Disk"),
        "choix du support absent"
    );

    // Sortie brute enregistrée à la demande (rejouée dans xterm.js pour vérifier le rendu).
    if let Some(path) = std::env::var_os("PCCHECK_CAPTURE") {
        std::fs::write(path, format!("{first}{second}")).unwrap();
    }

    // Quitter : q répété jusqu'à la fin du programme.
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.try_wait().is_none() && Instant::now() < deadline {
        session.write(b"q").unwrap();
        std::thread::sleep(Duration::from_millis(300));
    }
    let code = session.try_wait();
    println!("code de sortie : {code:?}");
    if code.is_none() {
        session.kill();
    }
}
