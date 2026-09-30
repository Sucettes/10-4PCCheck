//! Écrit le rapport d'exemple (JSON, HTML, PDF) pour en regarder le rendu.
//!
//! `cargo run -p pccheck-report --example sample -- <dossier>`
//! (par défaut : `<dossier temporaire>/pccheck-report-exemple`).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("pccheck-report-exemple"));
    let report = pccheck_report::sample_report();
    let start = Instant::now();
    match pccheck_report::save(&report, &dir) {
        Ok(saved) => {
            println!("Rendu en {} ms", start.elapsed().as_millis());
            println!("{}", saved.json.display());
            println!("{}", saved.html.display());
            println!("{}", saved.pdf.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Erreur : {e}");
            ExitCode::FAILURE
        }
    }
}
