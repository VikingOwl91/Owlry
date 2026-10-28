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
        // Keep an explicitly empty argument, but drop a token made only of
        // field codes that expanded to nothing.
        if !expanded.is_empty() || !word.contains('%') {
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

/// Decode the string-level escapes used by desktop-entry values.
///
/// The desktop spec applies these escapes to the key value before the `Exec`
/// tokenizer sees it, so a literal backslash in an argument is written as
/// `\\\\` in the file and arrives here as `\\`. Unknown escapes are preserved.
pub fn unescape_desktop_value(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Whether an argv vector invokes a shell anywhere in the command.
///
/// A desktop entry such as `Exec=sh -c "prog %f"`, or a terminal wrapper
/// around one, would otherwise re-enable shell interpretation after the field
/// code has been expanded. Any shell token is enough to refuse the handler;
/// this intentionally also rejects harmless `sh script.sh` handlers, because
/// the file path is not safe to hand to an interpreter.
pub fn invokes_shell(argv: &[String]) -> bool {
    let Some(first) = argv.first() else {
        return false;
    };
    let name = basename(first);
    if is_shell_name(name) {
        return true;
    }

    // Wrappers that execute the rest of the vector. Follow them instead of
    // scanning every argument, which would reject a file literally named
    // `bash` passed to an editor.
    if matches!(name, "xargs" | "parallel") {
        return true;
    }
    if matches!(name, "busybox" | "toybox") {
        return argv.len() > 1 && invokes_shell(&argv[1..]);
    }
    if name == "env" {
        let mut i = 1;
        while i < argv.len() {
            let arg = &argv[i];
            if arg == "-S" || arg.starts_with("--split-string") {
                return true;
            }
            if arg == "-u" || arg == "--unset" {
                i += 2;
                continue;
            }
            if arg.starts_with('-') || arg.contains('=') {
                i += 1;
                continue;
            }
            return invokes_shell(&argv[i..]);
        }
        return false;
    }
    if matches!(name, "sudo" | "doas" | "pkexec") {
        let mut i = 1;
        while i < argv.len() {
            let arg = &argv[i];
            if arg == "--" {
                return i + 1 < argv.len() && invokes_shell(&argv[i + 1..]);
            }
            if arg.starts_with('-') {
                if matches!(
                    arg.as_str(),
                    "-u" | "-g" | "-h" | "-p" | "-C" | "-T" | "-r" | "-t" | "-U"
                ) {
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            return invokes_shell(&argv[i..]);
        }
        return false;
    }
    if is_terminal_name(name)
        && let Some(pos) = argv
            .iter()
            .position(|arg| arg == "-e" || arg == "--" || arg == "-x")
    {
        return pos + 1 < argv.len() && invokes_shell(&argv[pos + 1..]);
    }

    false
}

fn basename(program: &str) -> &str {
    std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
}

fn is_shell_name(name: &str) -> bool {
    matches!(
        name,
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish" | "csh" | "tcsh"
    )
}

fn is_terminal_name(name: &str) -> bool {
    matches!(
        name,
        "xterm"
            | "kitty"
            | "foot"
            | "alacritty"
            | "wezterm"
            | "konsole"
            | "gnome-terminal"
            | "kgx"
            | "terminator"
            | "tilix"
            | "urxvt"
            | "st"
            | "sakura"
            | "terminology"
    )
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
    fn desktop_values_decode_string_escapes_before_tokenizing() {
        assert_eq!(unescape_desktop_value(r"a\\\\b"), r"a\\b");
        assert_eq!(unescape_desktop_value(r"a\sb"), "a b");
        assert_eq!(unescape_desktop_value(r"a\qb"), r"a\qb");

        let exec = unescape_desktop_value(r#"app "a\\\\b" %f"#);
        let argv = desktop_exec_argv(&exec, "/tmp/x").unwrap();
        assert_eq!(argv, vec!["app", r"a\b", "/tmp/x"]);
    }

    #[test]
    fn desktop_exec_preserves_an_explicit_empty_argument() {
        let argv =
            desktop_exec_argv_with(r#"app "" %f"#, Some("/tmp/x"), None, None, None).unwrap();
        assert_eq!(argv, vec!["app", "", "/tmp/x"]);
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
    fn detects_shells_even_behind_wrappers_or_combined_flags() {
        for argv in [
            vec!["sh", "-c", "prog /tmp/x"],
            vec!["/bin/bash", "-lc", "prog /tmp/x"],
            vec!["env", "sh", "-c", "prog /tmp/x"],
            vec!["env", "-S", "sh -c 'prog /tmp/x'"],
            vec!["xterm", "-e", "sh", "-c", "prog /tmp/x"],
            vec!["xterm", "-e", "env", "sh", "-c", "prog /tmp/x"],
            vec!["sudo", "sh", "-c", "prog /tmp/x"],
            vec!["sudo", "-u", "root", "sh", "-c", "prog /tmp/x"],
            vec!["busybox", "sh", "-c", "prog /tmp/x"],
            vec!["sh", "script.sh"],
        ] {
            let argv: Vec<String> = argv.into_iter().map(str::to_string).collect();
            assert!(invokes_shell(&argv), "missed shell in {argv:?}");
        }
        assert!(!invokes_shell(&["nvim".into(), "-c".into(), "cmd".into()]));
        assert!(!invokes_shell(&["app".into(), "/tmp/x".into()]));
        assert!(
            !invokes_shell(&["nvim".into(), "/home/user/bash".into()]),
            "a file named bash is not a shell wrapper"
        );
        assert!(!invokes_shell(&[
            "xterm".into(),
            "-e".into(),
            "nvim".into(),
            "/home/user/bash".into()
        ]));
    }

    #[test]
    fn desktop_exec_rejects_unmatched_quotes() {
        assert!(desktop_exec_argv("app \"unterminated %f", "/tmp/x").is_err());
    }
}
