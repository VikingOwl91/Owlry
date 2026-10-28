//! Small command-line tokenizer shared by direct commands, terminal
//! configuration and desktop-entry `Exec` expansion.
//!
//! This is deliberately not a shell parser. It understands quoting and
//! backslash escapes so callers can turn a user-facing line into an argv
//! vector, and it can refuse shell metacharacters when the caller promises
//! that no shell will interpret the result.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnterminatedQuote,
    TrailingBackslash,
    ShellMetacharacter(char),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "command line is empty"),
            ParseError::UnterminatedQuote => write!(f, "unterminated quote"),
            ParseError::TrailingBackslash => write!(f, "trailing backslash"),
            ParseError::ShellMetacharacter(c) => {
                write!(f, "unquoted shell metacharacter '{c}'")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Split `input` into words, honoring single quotes, double quotes and
/// backslash escapes.
pub fn split_words(input: &str) -> Result<Vec<String>, ParseError> {
    split_words_inner(input, false)
}

/// Like [`split_words`], but rejects unquoted shell metacharacters. Use this
/// when the result will be spawned as argv and must never be handed to a
/// shell.
pub fn split_words_strict(input: &str) -> Result<Vec<String>, ParseError> {
    split_words_inner(input, true)
}

fn split_words_inner(input: &str, strict: bool) -> Result<Vec<String>, ParseError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => current.push(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c) => current.push(c),
                            None => return Err(ParseError::TrailingBackslash),
                        },
                        Some(c) => current.push(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.next() {
                    Some(c) => current.push(c),
                    None => return Err(ParseError::TrailingBackslash),
                }
            }
            c if strict
                && matches!(
                    c,
                    ';' | '&' | '|' | '$' | '`' | '<' | '>' | '(' | ')' | '{' | '}'
                ) =>
            {
                return Err(ParseError::ShellMetacharacter(c));
            }
            c => {
                in_word = true;
                current.push(c);
            }
        }
    }

    if in_word {
        words.push(current);
    }
    if words.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(words)
}

/// Expand a desktop-entry `Exec` line against one file path, returning argv.
///
/// This is the common case for file handlers. See
/// [`desktop_exec_argv_with`] for the other field codes.
pub fn desktop_exec_argv(exec: &str, path: &str) -> Result<Vec<String>, ParseError> {
    desktop_exec_argv_with(exec, Some(path), None, None, None)
}

/// Expand a desktop-entry `Exec` line into argv.
///
/// `%f`, `%F`, `%u` and `%U` become the file path; `%c` the translated name;
/// `%i` the icon name; `%k` the desktop file path; `%%` a literal `%`.
/// Unknown codes are dropped. If a file path was supplied and the line
/// contains no file field code, the path is appended so the handler still
/// receives it. No shell is involved.
pub fn desktop_exec_argv_with(
    exec: &str,
    path: Option<&str>,
    name: Option<&str>,
    icon: Option<&str>,
    desktop_path: Option<&str>,
) -> Result<Vec<String>, ParseError> {
    let words = split_words(exec)?;
    let mut argv = Vec::new();
    let mut substituted = false;

    for word in words {
        // The spec expands `%i` to two arguments, `--icon` and the icon name.
        if word == "%i" {
            if let Some(icon) = icon {
                argv.push("--icon".to_string());
                argv.push(icon.to_string());
            }
            continue;
        }

        let mut expanded = String::new();
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                expanded.push(c);
                continue;
            }
            match chars.next() {
                Some('f' | 'F' | 'u' | 'U') => {
                    if let Some(path) = path {
                        expanded.push_str(path);
                        substituted = true;
                    }
                }
                Some('c') => {
                    if let Some(name) = name {
                        expanded.push_str(name);
                    }
                }
                Some('i') => {
                    if let Some(icon) = icon {
                        expanded.push_str(icon);
                    }
                }
                Some('k') => {
                    if let Some(desktop_path) = desktop_path {
                        expanded.push_str(desktop_path);
                    }
                }
                Some('%') => expanded.push('%'),
                Some(_) => {}
                None => {}
            }
        }
        if !expanded.is_empty() {
            argv.push(expanded);
        }
    }

    if let Some(path) = path
        && !substituted
    {
        argv.push(path.to_string());
    }
    if argv.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(argv)
}

/// Whether an argv vector explicitly invokes a shell's command-string mode.
///
/// A desktop entry such as `Exec=sh -c "prog %f"` would otherwise re-enable
/// shell interpretation after the field code has been expanded. Callers use
/// this to refuse that shape when the expansion included an untrusted path.
pub fn invokes_shell(argv: &[String]) -> bool {
    let Some(program) = argv.first() else {
        return false;
    };
    let name = std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    matches!(
        name,
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish" | "csh" | "tcsh"
    ) && argv[1..]
        .iter()
        .any(|arg| arg == "-c" || arg == "--command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plain_words_and_quotes() {
        assert_eq!(
            split_words(r#"nvim "/tmp/it's a file""#).unwrap(),
            vec!["nvim", "/tmp/it's a file"]
        );
        assert_eq!(
            split_words(r#"app "arg with space" plain"#).unwrap(),
            vec!["app", "arg with space", "plain"]
        );
    }

    #[test]
    fn strict_mode_rejects_unquoted_metacharacters() {
        for line in [
            "printf hi; touch /tmp/pwned",
            "printf hi && touch /tmp/pwned",
            "printf hi | sh",
            "printf $(id)",
            "printf `id`",
            "printf hi > /tmp/out",
        ] {
            assert!(split_words_strict(line).is_err(), "accepted {line:?}");
        }
    }

    #[test]
    fn strict_mode_allows_metacharacters_inside_quotes() {
        assert_eq!(
            split_words_strict("printf 'hi; not a command'").unwrap(),
            vec!["printf", "hi; not a command"]
        );
        assert_eq!(
            split_words_strict(r#"printf "a && b""#).unwrap(),
            vec!["printf", "a && b"]
        );
    }

    #[test]
    fn desktop_exec_expands_file_field_codes() {
        assert_eq!(
            desktop_exec_argv("nvim %F", "/tmp/it's a file").unwrap(),
            vec!["nvim", "/tmp/it's a file"]
        );
        assert_eq!(
            desktop_exec_argv("app %u", "/tmp/x").unwrap(),
            vec!["app", "/tmp/x"]
        );
        assert_eq!(
            desktop_exec_argv("app %i %c %F 100%%", "/tmp/x").unwrap(),
            vec!["app", "/tmp/x", "100%"]
        );
        assert_eq!(
            desktop_exec_argv("nvim", "/tmp/x").unwrap(),
            vec!["nvim", "/tmp/x"]
        );
    }

    #[test]
    fn desktop_exec_expands_name_icon_and_desktop_path_codes() {
        let argv = desktop_exec_argv_with(
            "app --title %c %i --file %k %f",
            Some("/tmp/x"),
            Some("Example"),
            Some("example-icon"),
            Some("/usr/share/applications/example.desktop"),
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                "app",
                "--title",
                "Example",
                "--icon",
                "example-icon",
                "--file",
                "/usr/share/applications/example.desktop",
                "/tmp/x",
            ]
        );
    }

    #[test]
    fn desktop_exec_without_a_path_does_not_append_one() {
        let argv = desktop_exec_argv_with(
            "app %c",
            None,
            Some("Example"),
            None,
            Some("/usr/share/applications/example.desktop"),
        )
        .unwrap();
        assert_eq!(argv, vec!["app", "Example"]);
    }

    #[test]
    fn desktop_exec_treats_shell_syntax_as_literal_arguments() {
        let argv = desktop_exec_argv("app; touch /tmp/pwned %f", "/tmp/file").unwrap();
        assert_eq!(argv[0], "app;");
        assert!(argv.iter().any(|a| a == "/tmp/pwned"));
        assert_eq!(argv.last().unwrap(), "/tmp/file");
    }

    #[test]
    fn detects_explicit_shell_command_mode() {
        assert!(invokes_shell(&[
            "sh".into(),
            "-c".into(),
            "prog /tmp/x".into()
        ]));
        assert!(invokes_shell(&[
            "/bin/bash".into(),
            "-c".into(),
            "prog".into()
        ]));
        assert!(!invokes_shell(&["sh".into(), "script.sh".into()]));
        assert!(!invokes_shell(&["nvim".into(), "-c".into(), "cmd".into()]));
    }

    #[test]
    fn desktop_exec_rejects_unmatched_quotes() {
        assert!(desktop_exec_argv("app \"unterminated %f", "/tmp/x").is_err());
    }
}
