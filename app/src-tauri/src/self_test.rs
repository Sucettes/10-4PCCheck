//! Mode d'autotest : `pccheck --self-test <fichier>`.
//!
//! L'interface s'affiche normalement, puis envoie ce qu'elle a rendu. Le résultat est écrit
//! dans le fichier et l'application quitte. Sert à vérifier l'app sans écran (CI, conteneurs).

use std::path::PathBuf;
use std::time::Duration;

/// Délai au-delà duquel l'autotest est déclaré en échec (interface jamais rendue).
const DEADLINE: Duration = Duration::from_secs(90);

/// Code de sortie quand l'interface n'a rien envoyé à temps.
pub const EXIT_TIMEOUT: i32 = 3;

/// Lit `--self-test <fichier>` dans les arguments. Erreur si le chemin manque.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Option<PathBuf>, String> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--self-test" {
            return match args.next() {
                Some(path) if !path.starts_with("--") => Ok(Some(PathBuf::from(path))),
                _ => Err("--self-test attend un chemin de fichier".into()),
            };
        }
    }
    Ok(None)
}

/// Écrit l'échec et quitte si l'interface ne répond pas avant le délai.
pub fn arm_deadline(output: PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(DEADLINE);
        let body = format!(
            r#"{{"ok":false,"reason":"interface non rendue après {} s"}}"#,
            DEADLINE.as_secs()
        );
        if let Err(e) = std::fs::write(&output, body) {
            eprintln!("autotest : écriture de {output:?} impossible : {e}");
        }
        std::process::exit(EXIT_TIMEOUT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn absent_flag_means_normal_mode() {
        assert_eq!(parse_args(args(&["pccheck"])).unwrap(), None);
    }

    #[test]
    fn flag_with_path_enables_self_test() {
        assert_eq!(
            parse_args(args(&["pccheck", "--self-test", "out.json"])).unwrap(),
            Some(PathBuf::from("out.json"))
        );
    }

    #[test]
    fn flag_without_path_is_an_error() {
        assert!(parse_args(args(&["pccheck", "--self-test"])).is_err());
        assert!(parse_args(args(&["pccheck", "--self-test", "--autre"])).is_err());
    }
}
