//! `owlry.util.*` — host helpers exposed to user Lua configs.
//!
//! Surface per docs/lua-api.md §5.1. All functions are stateless; they're
//! attached to the `owlry.util` sub-table built by [`build`] and consumed
//! from [`super::api::install`].
//!
//! No sandboxing in 2.1 (per spec §5.3): the user is running their own
//! config file on their own machine — same trust model as `~/.bashrc`.

use std::path::PathBuf;

use log::{debug, warn};
use mlua::{Lua, Table};

/// Construct the `owlry.util` table with every helper registered. Returned
/// to [`super::api::install`] which attaches it to the parent `owlry` table.
pub(crate) fn build(lua: &Lua) -> mlua::Result<Table> {
    let util = lua.create_table()?;

    util.set(
        "shell",
        lua.create_function(|_, cmd: String| Ok(shell_capture(&cmd)))?,
    )?;

    util.set(
        "shell_lines",
        lua.create_function(|_, cmd: String| {
            let out = shell_capture(&cmd);
            // Split on \n, drop a trailing empty element so a terminating
            // newline doesn't produce a phantom "" item.
            let mut lines: Vec<String> = out.split('\n').map(String::from).collect();
            if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
                lines.pop();
            }
            Ok(lines)
        })?,
    )?;

    util.set(
        "read_file",
        lua.create_function(|_, path: String| {
            // Per spec: "nil if missing". We extend "missing" to "any I/O
            // failure" (permission denied, etc.) so user configs don't have
            // to wrap every read in pcall.
            Ok(std::fs::read_to_string(&path).ok())
        })?,
    )?;

    util.set(
        "glob",
        lua.create_function(|_, pattern: String| {
            let expanded = expand_tilde(&pattern);
            match glob::glob(&expanded) {
                Ok(paths) => Ok(paths
                    .filter_map(|r| r.ok())
                    .map(|p: PathBuf| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()),
                Err(e) => Err(mlua::Error::RuntimeError(format!(
                    "owlry.util.glob: invalid pattern '{}': {}",
                    pattern, e
                ))),
            }
        })?,
    )?;

    util.set(
        "env",
        lua.create_function(|_, (name, default): (String, Option<String>)| {
            Ok(std::env::var(&name).ok().or(default))
        })?,
    )?;

    util.set(
        "hostname",
        lua.create_function(|_, ()| Ok(read_hostname()))?,
    )?;

    Ok(util)
}

/// Run a shell command via `sh -c`, capturing stdout. Trailing whitespace
/// is stripped — matches the common Lua-side convention (no surprise newlines).
/// Non-zero exit codes are tolerated; the (possibly empty) stdout is still
/// returned, and stderr is surfaced at warn level for debugging.
fn shell_capture(cmd: &str) -> String {
    let output = match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(o) => o,
        Err(e) => {
            warn!("owlry.util.shell: failed to spawn `sh -c {}`: {}", cmd, e);
            return String::new();
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            "owlry.util.shell: `{}` exited {} (stderr: {})",
            cmd,
            output.status,
            stderr.trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim_end().to_string()
}

/// Expand a leading `~/` to the user's home directory. Other forms
/// (`~user/`, embedded `~`, env-var refs) are left as-is — keep the
/// surface predictable.
fn expand_tilde(pattern: &str) -> String {
    if let Some(rest) = pattern.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().into_owned();
    }
    pattern.to_string()
}

/// Resolve the current hostname via `gethostname(2)`. `libc` is already a
/// hard dep elsewhere, so no extra crate needed. Returns empty string on
/// the unlikely failure path (signature guarantees infallibility on Linux).
fn read_hostname() -> String {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut std::os::raw::c_char, buf.len()) };
    if rc != 0 {
        debug!("owlry.util.hostname: gethostname returned {}", rc);
        return String::new();
    }
    // Strings from gethostname are NUL-terminated within the buffer.
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn util_table(lua: &Lua) -> Table {
        build(lua).expect("build util table")
    }

    fn install_owlry_util(lua: &Lua) {
        let owlry = lua.create_table().expect("table");
        owlry.set("util", util_table(lua)).expect("set util");
        lua.globals().set("owlry", owlry).expect("global");
    }

    #[test]
    fn shell_captures_stdout_trimmed() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: String = lua
            .load(r#"return owlry.util.shell("echo hi")"#)
            .eval()
            .expect("shell");
        assert_eq!(out, "hi");
    }

    #[test]
    fn shell_preserves_interior_newlines() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: String = lua
            .load(r#"return owlry.util.shell("printf 'a\nb\nc'")"#)
            .eval()
            .expect("shell");
        assert_eq!(out, "a\nb\nc");
    }

    #[test]
    fn shell_lines_splits_into_table_without_trailing_empty() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Vec<String> = lua
            .load(r#"return owlry.util.shell_lines("printf 'a\nb\nc\n'")"#)
            .eval()
            .expect("shell_lines");
        assert_eq!(out, vec!["a", "b", "c"]);
    }

    #[test]
    fn shell_lines_empty_command_yields_empty_table() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Vec<String> = lua
            .load(r#"return owlry.util.shell_lines("true")"#)
            .eval()
            .expect("shell_lines");
        assert_eq!(out, Vec::<String>::new());
    }

    #[test]
    fn read_file_returns_contents_when_present() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hello.txt");
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(b"hello from owlry").expect("write");

        let lua = Lua::new();
        install_owlry_util(&lua);
        let script = format!(
            r#"return owlry.util.read_file("{}")"#,
            path.to_string_lossy()
        );
        let out: Option<String> = lua.load(&script).eval().expect("read_file");
        assert_eq!(out.as_deref(), Some("hello from owlry"));
    }

    #[test]
    fn read_file_returns_nil_when_missing() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Option<String> = lua
            .load(r#"return owlry.util.read_file("/definitely/does/not/exist")"#)
            .eval()
            .expect("read_file");
        assert!(out.is_none());
    }

    #[test]
    fn glob_lists_matching_paths() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["a.txt", "b.txt", "c.md"] {
            let mut f = std::fs::File::create(dir.path().join(name)).expect("create");
            f.write_all(b"x").expect("write");
        }
        let pattern = format!("{}/*.txt", dir.path().to_string_lossy());

        let lua = Lua::new();
        install_owlry_util(&lua);
        let script = format!(r#"return owlry.util.glob("{}")"#, pattern);
        let out: Vec<String> = lua.load(&script).eval().expect("glob");
        assert_eq!(out.len(), 2, "two .txt files expected, got: {:?}", out);
        assert!(out.iter().any(|p| p.ends_with("a.txt")));
        assert!(out.iter().any(|p| p.ends_with("b.txt")));
    }

    #[test]
    fn glob_returns_empty_for_no_matches() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Vec<String> = lua
            .load(r#"return owlry.util.glob("/tmp/owlry_definitely_no_match_xyz/*")"#)
            .eval()
            .expect("glob");
        assert!(out.is_empty());
    }

    #[test]
    fn glob_errors_on_invalid_pattern() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        // Range patterns with no `]` are invalid in the glob crate.
        let err = lua
            .load(r#"return owlry.util.glob("[unterminated")"#)
            .eval::<Vec<String>>()
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("owlry.util.glob"), "got: {}", msg);
    }

    #[test]
    fn env_returns_value_when_set() {
        unsafe {
            std::env::set_var("OWLRY_TEST_ENV_VAR", "present");
        }
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Option<String> = lua
            .load(r#"return owlry.util.env("OWLRY_TEST_ENV_VAR")"#)
            .eval()
            .expect("env");
        assert_eq!(out.as_deref(), Some("present"));
        unsafe {
            std::env::remove_var("OWLRY_TEST_ENV_VAR");
        }
    }

    #[test]
    fn env_returns_nil_when_missing_no_default() {
        // Pick a name unlikely to collide; also pre-clear for safety.
        unsafe {
            std::env::remove_var("OWLRY_TEST_MISSING_XYZ");
        }
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Option<String> = lua
            .load(r#"return owlry.util.env("OWLRY_TEST_MISSING_XYZ")"#)
            .eval()
            .expect("env");
        assert!(out.is_none());
    }

    #[test]
    fn env_returns_default_when_missing() {
        unsafe {
            std::env::remove_var("OWLRY_TEST_MISSING_FOR_DEFAULT");
        }
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: Option<String> = lua
            .load(r#"return owlry.util.env("OWLRY_TEST_MISSING_FOR_DEFAULT", "fallback")"#)
            .eval()
            .expect("env");
        assert_eq!(out.as_deref(), Some("fallback"));
    }

    #[test]
    fn hostname_returns_non_empty_string() {
        let lua = Lua::new();
        install_owlry_util(&lua);
        let out: String = lua
            .load(r#"return owlry.util.hostname()"#)
            .eval()
            .expect("hostname");
        assert!(!out.is_empty(), "hostname must be non-empty");
        // Sanity: no embedded NUL bytes.
        assert!(!out.contains('\0'));
    }

    #[test]
    fn expand_tilde_resolves_home_prefix() {
        if let Some(home) = dirs::home_dir() {
            let result = expand_tilde("~/foo/bar");
            assert!(result.starts_with(home.to_string_lossy().as_ref()));
            assert!(result.ends_with("/foo/bar"));
        }
    }

    #[test]
    fn expand_tilde_leaves_non_home_paths_untouched() {
        assert_eq!(expand_tilde("/etc/passwd"), "/etc/passwd");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
        assert_eq!(expand_tilde("~user/foo"), "~user/foo"); // ~user/ not supported
    }
}
