//! Ligne de commande Windows. `CreateProcessW` reçoit une seule chaîne : chaque programme la
//! redécoupe lui-même. Règles du runtime C de Microsoft :
//! https://learn.microsoft.com/cpp/c-language/parsing-c-command-line-arguments
//! Le PhotoRec livré est compilé avec Cygwin (`cygwin1.dll`), qui redécoupe autrement : `'` y
//! est aussi un guillemet, et `*?[{` déclenchent l'expansion de noms de fichiers. Entre
//! guillemets doubles, les deux découpages s'accordent : on y met tout argument qui contient
//! l'un de ces caractères.
//! Pure et compilée partout pour être testée sous Linux aussi.

/// Programme entre guillemets (argv[0] n'accepte pas d'échappement, et un chemin Windows ne
/// contient jamais de `"`), puis chaque argument échappé.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_command_line(program: &str, args: &[String]) -> String {
    let mut line = format!("\"{program}\"");
    for arg in args {
        line.push(' ');
        push_quoted(&mut line, arg);
    }
    line
}

/// Règles MSVCRT : entre guillemets si l'argument est vide ou contient un blanc, `"`, ou un
/// caractère spécial pour Cygwin (voir l'en-tête) ; dans
/// ce cas, `n` barres obliques inverses suivies de `"` deviennent `2n+1` barres puis `"`, et
/// `n` barres en fin d'argument deviennent `2n` (sinon elles échapperaient le `"` fermant).
/// Ailleurs, les barres sont littérales : `C:\dossier` reste tel quel.
#[cfg_attr(not(windows), allow(dead_code))]
fn push_quoted(line: &mut String, arg: &str) {
    let needs_quotes = arg.is_empty()
        || arg.contains([
            ' ', '\t', '\n', '\u{b}', '"', '\'', '*', '?', '[', ']', '{', '}',
        ]);
    if !needs_quotes {
        line.push_str(arg);
        return;
    }
    line.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                line.push_str(&"\\".repeat(backslashes * 2 + 1));
                line.push('"');
                backslashes = 0;
            }
            _ => {
                line.push_str(&"\\".repeat(backslashes));
                line.push(c);
                backslashes = 0;
            }
        }
    }
    line.push_str(&"\\".repeat(backslashes * 2));
    line.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_arguments_are_not_quoted() {
        let args: Vec<String> = ["/log", r"\\.\PhysicalDrive1", "partition_none,search"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            windows_command_line(r"C:\Clé USB\photorec_win.exe", &args),
            r#""C:\Clé USB\photorec_win.exe" /log \\.\PhysicalDrive1 partition_none,search"#
        );
    }

    /// Partie « argument » d'une ligne à un seul argument.
    fn quoted(arg: &str) -> String {
        let full = windows_command_line("p", &[arg.to_string()]);
        full.strip_prefix("\"p\" ").unwrap_or_default().to_string()
    }

    #[test]
    fn spaces_quotes_and_trailing_backslashes() {
        assert_eq!(
            quoted(r"E:\mes recup\recup_dir"),
            r#""E:\mes recup\recup_dir""#
        );
        assert_eq!(quoted(r"E:\a b\"), r#""E:\a b\\""#);
        assert_eq!(quoted(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quoted(r#"a\"b"#), r#""a\\\"b""#);
        assert_eq!(quoted(""), r#""""#);
        assert_eq!(quoted(r"C:\sans_espace\"), r"C:\sans_espace\");
    }

    #[test]
    fn cygwin_special_characters_are_quoted() {
        let args: Vec<String> = [r"D:\L'ete\recup_dir", "fileopt,tx?,enable,search"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            windows_command_line("p.exe", &args),
            r#""p.exe" "D:\L'ete\recup_dir" "fileopt,tx?,enable,search""#
        );
    }
}
