//! File search provider.
//!
//! Dynamic provider that searches for files using `fd` (preferred) or
//! `locate`. Because each query shells out and walks the filesystem, it is
//! **opt-in only** — it never contributes to bare auto-mode queries (see
//! [`DynamicProvider::runs_in_auto_mode`]). Triggered by:
//! - `:file name` / `:files name` / `:find name` (prefix)
//! - `/ name` / `/name` (slash alias, routed to the `:file` prefix by
//!   [`crate::filter::ProviderFilter::parse_query`])
//!
//! External dependencies:
//! - `fd` (preferred) or `locate`

use super::{DynamicProvider, ItemSource, LaunchItem, ProviderType};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TYPE_ID: &str = "filesearch";
const MAX_RESULTS: usize = 20;
/// Cap recursion depth so a single query can't walk an unbounded tree.
const MAX_DEPTH: usize = 8;
/// Hard wall-clock cap per query — defends against slow/network mounts.
const QUERY_TIMEOUT: Duration = Duration::from_millis(750);
/// Noise directories pruned from the walk (`fd` already skips hidden and
/// `.gitignore`d paths by default; these cover the non-hidden offenders).
const EXCLUDES: &[&str] = &[".git", "node_modules", "target", ".cache"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchTool {
    Fd,
    Locate,
    None,
}

/// Dynamic file search provider — shells out to `fd` or `locate` on explicit
/// `:file` / `/` queries (never in auto mode).
pub(crate) struct FileSearchProvider {
    search_tool: SearchTool,
    home: String,
    /// Resolved, existing search roots. Empty means "search `$HOME`".
    roots: Vec<String>,
}

impl Default for FileSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSearchProvider {
    /// Construct with the default root (`$HOME`).
    pub fn new() -> Self {
        Self::with_roots(Vec::new())
    }

    /// Construct with explicit search roots (from `providers.filesearch_roots`).
    /// Entries may use a leading `~/`; non-directory entries are dropped. An
    /// empty resolved set falls back to `$HOME`.
    pub fn with_roots(roots: Vec<String>) -> Self {
        let search_tool = Self::detect_search_tool();
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        let roots = Self::resolve_roots(roots, &home);

        Self {
            search_tool,
            home,
            roots,
        }
    }

    /// Expand `~`/`~/` against `home` and keep only entries that are existing
    /// directories.
    fn resolve_roots(roots: Vec<String>, home: &str) -> Vec<String> {
        roots
            .into_iter()
            .map(|r| Self::expand_tilde(r.trim(), home))
            .filter(|p| !p.is_empty() && Path::new(p).is_dir())
            .collect()
    }

    fn expand_tilde(path: &str, home: &str) -> String {
        if path == "~" {
            home.to_string()
        } else if let Some(rest) = path.strip_prefix("~/") {
            format!("{}/{}", home, rest)
        } else {
            path.to_string()
        }
    }

    /// Assemble the `fd` argument vector with bounds and roots. Pure so the
    /// bounding can be unit-tested without spawning `fd`.
    fn fd_args(pattern: &str, roots: &[String]) -> Vec<String> {
        let mut args = vec![
            "--max-results".to_string(),
            MAX_RESULTS.to_string(),
            "--max-depth".to_string(),
            MAX_DEPTH.to_string(),
            "--type".to_string(),
            "f".to_string(),
            "--type".to_string(),
            "d".to_string(),
        ];
        for ex in EXCLUDES {
            args.push("--exclude".to_string());
            args.push((*ex).to_string());
        }
        args.push(pattern.to_string());
        args.extend(roots.iter().cloned());
        args
    }

    /// Run `cmd`, draining stdout on a thread (so a full pipe can't deadlock
    /// the child) and killing+reaping it past `timeout`. Returns `None` on
    /// spawn failure or timeout.
    fn run_bounded(mut cmd: Command, timeout: Duration) -> Option<String> {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut stdout = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stdout.read_to_string(&mut buf);
            buf
        });

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => return reader.join().ok(),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = reader.join();
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return None;
                }
            }
        }
    }

    fn detect_search_tool() -> SearchTool {
        // Prefer fd (faster, respects .gitignore).
        if Self::command_exists("fd") {
            return SearchTool::Fd;
        }
        // Fall back to locate (requires updatedb).
        if Self::command_exists("locate") {
            return SearchTool::Locate;
        }
        SearchTool::None
    }

    fn command_exists(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Extract the search term from the query.
    ///
    /// The trigger markers (`:file`, `/`) are stripped upstream by
    /// [`crate::filter::ProviderFilter::parse_query`], so the term arrives
    /// clean — this just trims and rejects an empty term.
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn evaluate(&self, query: &str) -> Vec<LaunchItem> {
        let search_term = match Self::extract_search_term(query) {
            Some(t) if !t.is_empty() => t,
            _ => return Vec::new(),
        };

        self.search_files(search_term)
    }

    fn search_files(&self, pattern: &str) -> Vec<LaunchItem> {
        match self.search_tool {
            SearchTool::Fd => self.search_with_fd(pattern),
            SearchTool::Locate => self.search_with_locate(pattern),
            SearchTool::None => Vec::new(),
        }
    }

    fn search_with_fd(&self, pattern: &str) -> Vec<LaunchItem> {
        let mut cmd = Command::new("fd");
        cmd.args(Self::fd_args(pattern, &self.roots));
        // With no explicit roots, search from $HOME; otherwise the roots are
        // passed as positional paths and the cwd is irrelevant.
        if self.roots.is_empty() {
            cmd.current_dir(&self.home);
        }

        match Self::run_bounded(cmd, QUERY_TIMEOUT) {
            Some(out) => self.parse_file_results(&out),
            None => Vec::new(),
        }
    }

    fn search_with_locate(&self, pattern: &str) -> Vec<LaunchItem> {
        // `locate` queries a prebuilt index, so roots/depth don't apply; it
        // still gets the wall-clock cap.
        let mut cmd = Command::new("locate");
        cmd.args([
            "--limit",
            &MAX_RESULTS.to_string(),
            "--ignore-case",
            pattern,
        ]);

        match Self::run_bounded(cmd, QUERY_TIMEOUT) {
            Some(out) => self.parse_file_results(&out),
            None => Vec::new(),
        }
    }

    fn parse_file_results(&self, output: &str) -> Vec<LaunchItem> {
        output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|path| {
                let path = path.trim();
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("{}/{}", self.home, path)
                };

                let filename = Path::new(&full_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| full_path.clone());

                let is_dir = Path::new(&full_path).is_dir();
                let icon = if is_dir { "folder" } else { "text-x-generic" };

                let command = format!("xdg-open '{}'", full_path.replace('\'', "'\\''"));

                LaunchItem {
                    id: format!("file:{}", full_path),
                    name: filename,
                    description: Some(full_path.clone()),
                    icon: Some(icon.to_string()),
                    provider: ProviderType::Plugin(TYPE_ID.into()),
                    command,
                    terminal: false,
                    tags: vec!["file".to_string()],
                    source: ItemSource::Core,
                }
            })
            .collect()
    }
}

impl DynamicProvider for FileSearchProvider {
    fn name(&self) -> &str {
        "Files"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn priority(&self) -> u32 {
        8_000
    }

    /// File search is opt-in: it only runs when `:file` (or the `/` alias) is
    /// active, never on a bare auto-mode keystroke. Each query shells out to
    /// `fd`/`locate`, so running it per-keystroke in auto mode stalls the
    /// whole result set.
    fn runs_in_auto_mode(&self) -> bool {
        false
    }

    fn prefix(&self) -> Option<&str> {
        Some(":file")
    }

    fn icon(&self) -> &str {
        "system-file-manager"
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Files")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("files")
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        self.evaluate(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_search_term() {
        // Markers are stripped upstream; the term arrives clean and is used
        // verbatim (just trimmed). No more `/`, `file `, `find ` magic — that
        // caused `:file find me` to search for "me".
        assert_eq!(
            FileSearchProvider::extract_search_term("config.toml"),
            Some("config.toml")
        );
        assert_eq!(
            FileSearchProvider::extract_search_term("  bashrc  "),
            Some("bashrc")
        );
        assert_eq!(
            FileSearchProvider::extract_search_term("find me"),
            Some("find me"),
            "the leading word must NOT be stripped anymore"
        );
    }

    #[test]
    fn test_extract_search_term_empty() {
        assert_eq!(FileSearchProvider::extract_search_term(""), None);
        assert_eq!(FileSearchProvider::extract_search_term("   "), None);
    }

    #[test]
    fn test_command_exists() {
        // 'which' should exist on any Unix system.
        assert!(FileSearchProvider::command_exists("which"));
        // This should not exist.
        assert!(!FileSearchProvider::command_exists(
            "nonexistent-command-12345"
        ));
    }

    #[test]
    fn test_detect_search_tool() {
        // Just ensure it doesn't panic.
        let _ = FileSearchProvider::detect_search_tool();
    }

    #[test]
    fn test_state_new() {
        let provider = FileSearchProvider::new();
        assert!(!provider.home.is_empty());
    }

    #[test]
    fn test_evaluate_empty() {
        let provider = FileSearchProvider::new();
        assert!(provider.evaluate("").is_empty());
        assert!(provider.evaluate("   ").is_empty());
    }

    #[test]
    fn filesearch_does_not_run_in_auto_mode() {
        let provider = FileSearchProvider::new();
        assert!(
            !provider.runs_in_auto_mode(),
            "file search must be opt-in (prefix only), never auto-mode"
        );
    }

    #[test]
    fn fd_args_are_bounded() {
        let args = FileSearchProvider::fd_args("needle", &[]);
        // Depth cap present.
        let depth_idx = args.iter().position(|a| a == "--max-depth").expect("depth");
        assert_eq!(args[depth_idx + 1], MAX_DEPTH.to_string());
        // Result cap present.
        assert!(args.iter().any(|a| a == "--max-results"));
        // Every noise dir is excluded.
        for ex in EXCLUDES {
            assert!(
                args.windows(2).any(|w| w[0] == "--exclude" && w[1] == *ex),
                "missing --exclude {ex}"
            );
        }
        // Pattern is present; no roots appended.
        assert!(args.contains(&"needle".to_string()));
        assert_eq!(args.last().unwrap(), "needle");
    }

    #[test]
    fn fd_args_append_roots_after_pattern() {
        let roots = vec!["/a".to_string(), "/b".to_string()];
        let args = FileSearchProvider::fd_args("needle", &roots);
        let pat = args.iter().position(|a| a == "needle").unwrap();
        assert_eq!(&args[pat + 1..], &["/a".to_string(), "/b".to_string()]);
    }

    #[test]
    fn expand_tilde_resolves_home() {
        assert_eq!(
            FileSearchProvider::expand_tilde("~/Documents", "/home/u"),
            "/home/u/Documents"
        );
        assert_eq!(FileSearchProvider::expand_tilde("~", "/home/u"), "/home/u");
        assert_eq!(
            FileSearchProvider::expand_tilde("/etc", "/home/u"),
            "/etc",
            "absolute paths pass through untouched"
        );
    }

    #[test]
    fn resolve_roots_drops_nonexistent_and_keeps_dirs() {
        let home = std::env::temp_dir();
        let home_str = home.to_string_lossy().to_string();
        let resolved = FileSearchProvider::resolve_roots(
            vec![
                "~".to_string(),
                "/definitely/not/a/real/dir/xyz".to_string(),
            ],
            &home_str,
        );
        assert_eq!(
            resolved,
            vec![home_str],
            "only existing directories survive; ~ expands to home"
        );
    }

    #[test]
    fn provider_type_is_filesearch_plugin() {
        let p = FileSearchProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("filesearch".into()));
    }
}
