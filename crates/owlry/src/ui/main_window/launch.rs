use super::*;
use crate::providers::ItemSource;
use std::process::Command;

impl MainWindow {
    /// Handle item activation - returns true if window should close
    pub(super) fn handle_item_action(
        item: &LaunchItem,
        config: &Config,
        backend: &Rc<RefCell<SearchBackend>>,
    ) -> bool {
        match &item.activation {
            ItemActivation::PluginAction { command } => {
                // Provider-internal actions update state in place, so keep the
                // window open for the refreshed result set.
                if !backend.borrow_mut().execute_plugin_action(command) {
                    let message = format!("No provider handled action '{command}'");
                    log::warn!("{message}");
                    crate::notify::notify("Action failed", &message);
                }
                return false;
            }
            ItemActivation::Submenu { .. } => return false,
            ItemActivation::Launch => {}
        }

        Self::launch_item(item, config, backend);
        true
    }

    fn launch_item(item: &LaunchItem, config: &Config, backend: &Rc<RefCell<SearchBackend>>) {
        // dmenu mode: print selection to stdout instead of executing
        if matches!(item.provider, ProviderType::Dmenu) {
            println!("{}", item.name);
            return;
        }

        // Record this launch for frecency tracking (via backend)
        if config.providers.frecency {
            let provider_str = item.provider.to_string();
            backend.borrow_mut().record_launch(&item.id, &provider_str);
            #[cfg(feature = "dev-logging")]
            debug!("[UI] Recorded frecency launch for: {}", item.id);
        }

        info!("Launching: {} ({})", item.name, item.command);

        #[cfg(feature = "dev-logging")]
        debug!(
            "[UI] Launch details: terminal={}, provider={:?}, id={}",
            item.terminal, item.provider, item.id
        );

        // Reject script plugin commands that don't match the known-safe allowlist.
        // Script plugins (Lua/Rune user plugins) are untrusted code; only allow
        // patterns that can't escalate privileges or exfiltrate data.
        if item.source == ItemSource::ScriptPlugin {
            let cmd = &item.command;
            let allowed = cmd.is_empty()
                || cmd.starts_with("xdg-open ")
                || cmd.starts_with("wl-copy")
                || cmd.starts_with("wl-paste");
            if !allowed {
                let msg = format!(
                    "Blocked untrusted script plugin command from '{}': {}",
                    item.name, cmd
                );
                log::warn!("{}", msg);
                crate::notify::notify("Command blocked", &msg);
                return;
            }
        }

        // Reject items with no command — nothing to execute.
        if item.command.is_empty() && !matches!(item.provider, ProviderType::Application) {
            let msg = format!("Item '{}' has no command; cannot launch", item.name);
            log::warn!("{}", msg);
            crate::notify::notify("Launch failed", &msg);
            return;
        }

        // Check if this is a desktop application (has .desktop file as ID)
        let is_desktop_app =
            matches!(item.provider, ProviderType::Application) && item.id.ends_with(".desktop");

        // Desktop files should be launched via proper launchers that implement the
        // freedesktop Desktop Entry spec (D-Bus activation, field codes, env vars, etc.)
        // We delegate to: uwsm (if configured), gio launch, or gtk-launch as fallback.
        //
        // Non-desktop items (commands, plugins) use sh -c for shell execution.
        // File-search results resolve their default handler first so that
        // terminal-based handlers (nvim, vim, …) open in a real terminal
        // instead of being launched windowless via xdg-open, and so that
        // files with no associated application surface a notification.
        let result = if let Some(path) = Self::filesearch_open_path(item) {
            Self::launch_file_open(&path, config)
        } else if is_desktop_app {
            Self::launch_desktop_file(&item.id, config)
        } else {
            Self::launch_command(&item.command, item.terminal, config)
        };

        if let Err(e) = result {
            let msg = format!("Failed to launch '{}': {}", item.name, e);
            log::error!("{}", msg);
            crate::notify::notify("Launch failed", &msg);
        }
    }

    /// Launch a .desktop file.
    ///
    /// When `use_uwsm` is enabled in config, launches via `uwsm app -- <file>`
    /// which starts the app in a proper systemd user session.
    ///
    /// Otherwise, uses `gio launch` which is always available (part of glib2/GTK4)
    /// and handles D-Bus activation, field codes, Terminal flag, etc.
    fn launch_desktop_file(
        desktop_path: &str,
        config: &Config,
    ) -> std::io::Result<std::process::Child> {
        use std::path::Path;

        // Check if desktop file exists
        if !Path::new(desktop_path).exists() {
            let msg = format!("Desktop file not found: {}", desktop_path);
            log::error!("{}", msg);
            crate::notify::notify("Launch failed", &msg);
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, msg));
        }

        if config.general.use_uwsm {
            // Check if uwsm is available
            let uwsm_available = Command::new("which")
                .arg("uwsm")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if !uwsm_available {
                let msg = "uwsm is enabled in config but not installed";
                log::error!("{}", msg);
                crate::notify::notify("Launch failed", msg);
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, msg));
            }

            info!("Launching via uwsm: {}", desktop_path);
            Command::new("uwsm")
                .args(["app", "--", desktop_path])
                .spawn()
        } else {
            info!("Launching via gio: {}", desktop_path);
            Command::new("gio").args(["launch", desktop_path]).spawn()
        }
    }

    /// Launch a shell command (for non-desktop items like PATH commands, plugins, etc.)
    pub(super) fn launch_command(
        command: &str,
        terminal: bool,
        config: &Config,
    ) -> std::io::Result<std::process::Child> {
        let cmd = if terminal {
            let terminal_cmd = config
                .general
                .terminal_command
                .as_deref()
                .unwrap_or("xterm");
            format!("{} -e {}", terminal_cmd, command)
        } else {
            command.to_string()
        };

        // Shell/system commands run directly without uwsm wrapper
        // (they're typically short-lived or system utilities)
        let is_system_command = cmd.starts_with("playerctl ")
            || cmd.starts_with("dbus-send ")
            || cmd.starts_with("systemctl ")
            || cmd.starts_with("journalctl ")
            || cmd.contains(" | ")
            || cmd.contains(" && ")
            || cmd.contains(" || ")
            || cmd.contains(" > ")
            || cmd.contains(" < ");

        // Use uwsm for regular commands if enabled (and not a system command)
        if config.general.use_uwsm && !is_system_command {
            info!("Launching command via uwsm: {}", cmd);
            Command::new("uwsm")
                .args(["app", "--", "sh", "-c", &cmd])
                .spawn()
        } else {
            Command::new("sh").arg("-c").arg(&cmd).spawn()
        }
    }

    /// If `item` is a file-search result, return the absolute path it points
    /// at (decoded from its `file:<path>` id). Returns `None` for every other
    /// provider, so callers fall through to the generic launch path.
    pub(super) fn filesearch_open_path(item: &LaunchItem) -> Option<String> {
        match &item.provider {
            ProviderType::Plugin(id) if id == "filesearch" => {
                item.id.strip_prefix("file:").map(str::to_string)
            }
            _ => None,
        }
    }

    /// Open a file via its default desktop handler.
    ///
    /// Plain `xdg-open` ignores the handler's `Terminal=true` flag, so a file
    /// whose default app is a terminal program (nvim, vim, less, …) launches
    /// windowless and appears to do nothing. When the resolved handler wants a
    /// terminal we reconstruct its `Exec` line and run it through the
    /// configured terminal (reusing [`Self::launch_command`]'s terminal
    /// handling, including its `-e` convention).
    ///
    /// When the file's type has no associated application, the user is
    /// notified instead of silently getting nothing. GUI handlers and
    /// indeterminate resolutions fall back to plain `xdg-open`, preserving the
    /// prior behavior (Zed, browsers, image viewers, …).
    fn launch_file_open(path: &str, config: &Config) -> std::io::Result<std::process::Child> {
        match Self::resolve_default_handler(path) {
            HandlerResolution::Found(handler) if handler.terminal => {
                let exec = expand_exec_for_file(&handler.exec, path);
                info!(
                    "File handler is terminal-based; opening in terminal: {}",
                    exec
                );
                Self::launch_command(&exec, true, config)
            }
            HandlerResolution::NoAssociation => {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string());
                let msg = format!("No application is associated with '{}'", name);
                info!("{}", msg);
                crate::notify::notify("Cannot open file", &msg);
                // Best-effort: hand it to xdg-open anyway in case the desktop
                // environment offers its own "open with…" fallback.
                Self::launch_command(&format!("xdg-open {}", shell_quote(path)), false, config)
            }
            // GUI handler or indeterminate resolution: plain xdg-open.
            _ => Self::launch_command(&format!("xdg-open {}", shell_quote(path)), false, config),
        }
    }

    /// Enter the "Open with…" application picker for `file_path`.
    ///
    /// Reuses the submenu UI: the full installed-app list is shown (searchable
    /// by typing) and an `OpenWithContext` is recorded so row selection opens
    /// or associates the file instead of launching the app standalone.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn enter_open_with_picker(
        backend: &Rc<RefCell<SearchBackend>>,
        config: &Config,
        submenu_state: &Rc<RefCell<SubmenuState>>,
        results_list: &ListBox,
        current_results: &Rc<RefCell<Vec<LaunchItem>>>,
        mode_label: &Label,
        hints_label: &Label,
        search_entry: &Entry,
        file_path: &str,
    ) {
        let mime = Self::xdg_mime_query(&["query", "filetype", file_path]).unwrap_or_default();
        let apps = Self::fetch_applications(backend, config);
        if apps.is_empty() {
            crate::notify::notify("Open with", "No applications available to open this file");
            return;
        }

        let filename = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());

        {
            let mut state = submenu_state.borrow_mut();
            state.active = true;
            state.service_name = String::new();
            state.display_name = format!("Open “{}” with…", filename);
            state.items = apps.clone();
            state.saved_search = search_entry.text().to_string();
            state.open_with = Some(OpenWithContext {
                path: file_path.to_string(),
                mime,
            });
        }

        mode_label.set_label("← Open with…");
        hints_label.set_label("Enter: open once  Shift+Enter: set as default  Esc: back");
        search_entry.set_text("");
        search_entry.set_placeholder_text(Some("Filter applications..."));

        results_list.remove_all();
        for item in &apps {
            let row = ResultRow::new(item, "");
            results_list.append(&row);
        }
        if let Some(first_row) = results_list.row_at_index(0) {
            results_list.select_row(Some(&first_row));
        }
        *current_results.borrow_mut() = apps;
    }

    /// Fetch the full installed-application list (Application provider only).
    fn fetch_applications(
        backend: &Rc<RefCell<SearchBackend>>,
        config: &Config,
    ) -> Vec<LaunchItem> {
        let mut filter = ProviderFilter::all();
        filter.set_prefix(Some(ProviderType::Application));
        backend.borrow_mut().search("", 1000, &filter, config)
    }

    /// Open the picker's target file with the chosen application. When
    /// `persist` is set, also record it as the new default handler for the
    /// file's MIME type (writes `~/.config/mimeapps.list` via `xdg-mime`).
    /// Returns true once a launch was attempted (the caller can then close).
    pub(super) fn pick_application(
        app: &LaunchItem,
        ctx: &OpenWithContext,
        persist: bool,
        config: &Config,
    ) -> bool {
        if persist && let Some(desktop_id) = Self::desktop_id_basename(&app.id) {
            if Self::set_default_application(&desktop_id, &ctx.mime) {
                info!("Set default application for {}: {}", ctx.mime, desktop_id);
            } else {
                crate::notify::notify(
                    "Open with",
                    &format!("Could not set {} as default for {}", app.name, ctx.mime),
                );
            }
        }

        if let Err(e) = Self::launch_file_with_app(&app.id, &ctx.path, config) {
            crate::notify::notify(
                "Open failed",
                &format!("Failed to open with {}: {}", app.name, e),
            );
        }
        true
    }

    /// Open `file_path` with a specific application `.desktop` file. Reuses the
    /// handler parsing so a `Terminal=true` chosen app is wrapped in the
    /// configured terminal, just like the default-handler path.
    fn launch_file_with_app(
        app_desktop_path: &str,
        file_path: &str,
        config: &Config,
    ) -> std::io::Result<std::process::Child> {
        if let Ok(contents) = std::fs::read_to_string(app_desktop_path)
            && let Some(handler) = parse_desktop_handler(&contents)
        {
            let exec = expand_exec_for_file(&handler.exec, file_path);
            info!("Opening '{}' with chosen app: {}", file_path, exec);
            return Self::launch_command(&exec, handler.terminal, config);
        }
        // Fallback: let gio launch the .desktop with the file as an argument.
        info!(
            "Opening '{}' via gio launch {}",
            file_path, app_desktop_path
        );
        Command::new("gio")
            .args(["launch", app_desktop_path, file_path])
            .spawn()
    }

    /// Persist `desktop_id` as the default application for `mime` via
    /// `xdg-mime default` (which writes `mimeapps.list` in the spec format).
    fn set_default_application(desktop_id: &str, mime: &str) -> bool {
        if mime.is_empty() {
            return false;
        }
        match Command::new("xdg-mime")
            .args(["default", desktop_id, mime])
            .status()
        {
            Ok(status) => status.success(),
            Err(e) => {
                log::warn!("xdg-mime default failed: {}", e);
                false
            }
        }
    }

    /// Extract the desktop-file id (e.g. `org.gnome.gedit.desktop`) from a full
    /// application `.desktop` path. Returns `None` for paths that don't name a
    /// `.desktop` file.
    fn desktop_id_basename(desktop_path: &str) -> Option<String> {
        let name = std::path::Path::new(desktop_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())?;
        if name.ends_with(".desktop") {
            Some(name)
        } else {
            None
        }
    }

    /// Resolve the default desktop handler for `path` by MIME type.
    ///
    /// Distinguishes "no application is associated" (worth informing the user)
    /// from "resolution tooling unavailable" (stay silent, fall back to
    /// `xdg-open`).
    pub(super) fn resolve_default_handler(path: &str) -> HandlerResolution {
        // MIME type — `xdg-mime` missing/erroring leaves us indeterminate.
        let mime = match Self::xdg_mime_query(&["query", "filetype", path]) {
            Some(m) => m,
            None => return HandlerResolution::Indeterminate,
        };
        if mime.is_empty() {
            return HandlerResolution::NoAssociation;
        }

        // Default application for that type — empty means nothing is associated.
        let desktop_id = match Self::xdg_mime_query(&["query", "default", &mime]) {
            Some(d) => d,
            None => return HandlerResolution::Indeterminate,
        };
        if desktop_id.is_empty() {
            return HandlerResolution::NoAssociation;
        }

        // Locate and parse the named .desktop file.
        let desktop_path = match Self::locate_desktop_file(&desktop_id) {
            Some(p) => p,
            None => return HandlerResolution::Indeterminate,
        };
        match std::fs::read_to_string(&desktop_path)
            .ok()
            .and_then(|c| parse_desktop_handler(&c))
        {
            Some(handler) => HandlerResolution::Found(handler),
            None => HandlerResolution::Indeterminate,
        }
    }

    /// Run `xdg-mime` with `args`, returning trimmed stdout on success.
    fn xdg_mime_query(args: &[&str]) -> Option<String> {
        let out = Command::new("xdg-mime").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Locate a `.desktop` file by id across the XDG application directories.
    fn locate_desktop_file(desktop_id: &str) -> Option<std::path::PathBuf> {
        use std::path::PathBuf;

        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(data_home).join("applications"));
        } else if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".local/share/applications"));
        }
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
            dirs.push(PathBuf::from(d).join("applications"));
        }

        // Direct match first; then the freedesktop nested-id convention where
        // `subdir-name.desktop` lives at `subdir/name.desktop`.
        for dir in &dirs {
            let direct = dir.join(desktop_id);
            if direct.is_file() {
                return Some(direct);
            }
        }
        if desktop_id.contains('-') {
            let nested = desktop_id.replace('-', "/");
            for dir in &dirs {
                let p = dir.join(&nested);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        None
    }
}

/// A file's default desktop handler: its `Exec` line (field codes intact) and
/// whether it requests a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopHandler {
    exec: String,
    terminal: bool,
}

/// Outcome of resolving a file's default application.
pub(super) enum HandlerResolution {
    /// A default handler was found and parsed.
    Found(DesktopHandler),
    /// The type has no associated default application (or no MIME type) — the
    /// user should be told rather than getting a silent no-op.
    NoAssociation,
    /// Resolution couldn't run to completion (tooling absent, handler file
    /// missing, …); stay silent and fall back to `xdg-open`.
    Indeterminate,
}

/// Shell-quote `s` for inclusion in a single-quoted `sh -c` argument.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parse the `[Desktop Entry]` group of a `.desktop` file for `Exec=` and
/// `Terminal=`. Only the first `Exec=` in the group is used, and only that
/// group is consulted (action groups are ignored). Returns `None` when there
/// is no `Exec` line — nothing to launch.
fn parse_desktop_handler(contents: &str) -> Option<DesktopHandler> {
    let mut in_entry = false;
    let mut exec: Option<String> = None;
    let mut terminal = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(value.trim().to_string());
            }
        } else if let Some(value) = line.strip_prefix("Terminal=") {
            terminal = value.trim().eq_ignore_ascii_case("true");
        }
    }

    exec.map(|exec| DesktopHandler { exec, terminal })
}

/// Expand a desktop `Exec` line's field codes against a single `path`.
///
/// `%f %F %u %U` are replaced with the shell-quoted path; `%%` becomes a
/// literal `%`; every other code (`%i %c %k`, …) is dropped per the
/// freedesktop spec. If the line carries no file field code, the path is
/// appended so the file is still passed to the handler.
fn expand_exec_for_file(exec: &str, path: &str) -> String {
    let quoted = shell_quote(path);
    let mut out = String::new();
    let mut substituted = false;
    let mut chars = exec.chars();

    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('f') | Some('F') | Some('u') | Some('U') => {
                out.push_str(&quoted);
                substituted = true;
            }
            Some('%') => out.push('%'),
            _ => {} // drop %i %c %k and unknown/trailing codes
        }
    }

    let mut out = out.trim().to_string();
    if !substituted {
        out.push(' ');
        out.push_str(&quoted);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(provider: ProviderType, id: &str) -> LaunchItem {
        LaunchItem {
            id: id.to_string(),
            name: "x".into(),
            description: None,
            icon: None,
            provider,
            command: "xdg-open '/tmp/x'".into(),
            terminal: false,
            activation: ItemActivation::Launch,
            tags: Vec::new(),
            source: ItemSource::Core,
        }
    }

    #[test]
    fn filesearch_open_path_extracts_path_from_id() {
        let it = item(
            ProviderType::Plugin("filesearch".into()),
            "file:/home/u/a.rs",
        );
        assert_eq!(
            MainWindow::filesearch_open_path(&it).as_deref(),
            Some("/home/u/a.rs")
        );
    }

    #[test]
    fn filesearch_open_path_none_for_other_providers() {
        let app = item(ProviderType::Application, "file:/home/u/a.rs");
        assert!(MainWindow::filesearch_open_path(&app).is_none());
        let calc = item(ProviderType::Plugin("calc".into()), "file:/home/u/a.rs");
        assert!(MainWindow::filesearch_open_path(&calc).is_none());
    }

    #[test]
    fn parse_handler_reads_command_and_terminal_true() {
        let contents = "[Desktop Entry]\nName=Neovim\nExec=nvim %F\nTerminal=true\n";
        let h = parse_desktop_handler(contents).expect("has command");
        assert_eq!(h.exec, "nvim %F");
        assert!(h.terminal);
    }

    #[test]
    fn parse_handler_absent_terminal_is_false() {
        // Regression guard: GUI handlers (no Terminal= line) must NOT be
        // treated as terminal apps, so they keep using xdg-open.
        let contents = "[Desktop Entry]\nName=Zed\nExec=zeditor %U\n";
        let h = parse_desktop_handler(contents).expect("has command");
        assert!(!h.terminal);
    }

    #[test]
    fn parse_handler_terminal_value_is_case_insensitive() {
        let contents = "[Desktop Entry]\nExec=nvim %F\nTerminal=True\n";
        assert!(parse_desktop_handler(contents).unwrap().terminal);
    }

    #[test]
    fn parse_handler_ignores_action_groups() {
        // Terminal=true in an action group must not leak into the main entry.
        let contents = "[Desktop Entry]\nExec=zeditor %U\nTerminal=false\n\n\
                        [Desktop Action new]\nExec=zeditor --new\nTerminal=true\n";
        let h = parse_desktop_handler(contents).expect("has command");
        assert_eq!(h.exec, "zeditor %U");
        assert!(!h.terminal);
    }

    #[test]
    fn parse_handler_none_without_command_line() {
        let contents = "[Desktop Entry]\nName=Broken\nTerminal=true\n";
        assert!(parse_desktop_handler(contents).is_none());
    }

    #[test]
    fn expand_substitutes_file_field_codes() {
        assert_eq!(
            expand_exec_for_file("nvim %F", "/home/u/a.rs"),
            "nvim '/home/u/a.rs'"
        );
        assert_eq!(expand_exec_for_file("vim %f", "/tmp/x"), "vim '/tmp/x'");
        assert_eq!(expand_exec_for_file("app %u", "/tmp/x"), "app '/tmp/x'");
        assert_eq!(expand_exec_for_file("app %U", "/tmp/x"), "app '/tmp/x'");
    }

    #[test]
    fn expand_drops_unknown_codes_and_keeps_literal_percent() {
        assert_eq!(
            expand_exec_for_file("app %i %c %F 100%%", "/tmp/x"),
            "app   '/tmp/x' 100%"
        );
    }

    #[test]
    fn expand_appends_path_when_no_field_code() {
        assert_eq!(expand_exec_for_file("nvim", "/tmp/x"), "nvim '/tmp/x'");
    }

    #[test]
    fn expand_quotes_embedded_single_quotes() {
        assert_eq!(
            expand_exec_for_file("nvim %F", "/tmp/it's a file"),
            "nvim '/tmp/it'\\''s a file'"
        );
    }

    #[test]
    fn desktop_id_basename_extracts_id_from_path() {
        assert_eq!(
            MainWindow::desktop_id_basename("/usr/share/applications/org.gnome.gedit.desktop")
                .as_deref(),
            Some("org.gnome.gedit.desktop")
        );
        assert_eq!(
            MainWindow::desktop_id_basename(
                "/home/u/.local/share/applications/dev.zed.Zed.desktop"
            )
            .as_deref(),
            Some("dev.zed.Zed.desktop")
        );
    }

    #[test]
    fn desktop_id_basename_none_for_non_desktop_paths() {
        assert!(MainWindow::desktop_id_basename("/usr/bin/nvim").is_none());
        assert!(MainWindow::desktop_id_basename("").is_none());
    }
}
