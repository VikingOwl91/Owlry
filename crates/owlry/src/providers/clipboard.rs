//! Clipboard history provider.
//!
//! Static provider that surfaces `cliphist` entries as launch items. Selecting
//! an item runs `cliphist decode | wl-copy` to put the entry back on the
//! clipboard. Requires `cliphist` and `wl-clipboard` to be installed.

use super::{ItemActivation, ItemSource, LaunchItem, Provider, ProviderType};
use crate::paths;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const TYPE_ID: &str = "clipboard";
/// External program this provider's items come from. Declared to the manager
/// via `backing_program` so installing or removing it is noticed at query
/// time; cliphist ships no `.desktop` file, so no watcher sees it.
const CLIPHIST: &str = "cliphist";
const PROVIDER_ICON: &str = "edit-paste";
/// Fallback icon for image entries when the thumbnail can't be decoded.
const IMAGE_FALLBACK_ICON: &str = "image-x-generic";
const DEFAULT_MAX_ENTRIES: usize = 50;
/// Image type tokens cliphist reports in its `[[ binary data ... ]]` preview.
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "svg"];

pub struct ClipboardProvider {
    items: Vec<LaunchItem>,
    max_entries: usize,
}

impl Default for ClipboardProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardProvider {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    fn has_cliphist() -> bool {
        super::on_path(CLIPHIST)
    }

    /// Detect cliphist's binary-image preview and return a clean display name.
    ///
    /// cliphist renders images as `[[ binary data 1.27 MiB png 1920x1080 ]]`;
    /// we surface that as e.g. `Image — png 1920x1080`. Returns `None` for text
    /// or non-image binary entries.
    fn image_display_name(preview: &str) -> Option<String> {
        let inner = preview
            .trim()
            .strip_prefix("[[ binary data")?
            .strip_suffix("]]")?;
        let tokens: Vec<&str> = inner.split_whitespace().collect();
        let ext = tokens
            .iter()
            .map(|t| t.to_lowercase())
            .find(|t| IMAGE_EXTS.contains(&t.as_str()))?;
        // Dimensions token, e.g. "1920x1080" (optional — older cliphist omits it).
        let dims = tokens
            .iter()
            .find(|t| t.contains('x') && t.chars().all(|c| c.is_ascii_digit() || c == 'x'));
        Some(match dims {
            Some(d) => format!("Image — {ext} {d}"),
            None => format!("Image — {ext}"),
        })
    }

    fn parse_cliphist_output(output: &str, max_entries: usize) -> Vec<LaunchItem> {
        let mut items = Vec::new();

        for line in output.lines().take(max_entries) {
            // cliphist format: "id\tpreview"
            let parts: Vec<&str> = line.splitn(2, '\t').collect();

            if parts.is_empty() {
                continue;
            }

            let clip_id = parts[0];
            let raw_preview = parts.get(1).copied();

            // Image entries get a clean name + image fallback icon + "image" tag
            // (refresh() swaps the icon for a decoded thumbnail). Everything else
            // keeps the truncated text preview.
            let (name, icon, tags) = match raw_preview.and_then(Self::image_display_name) {
                Some(image_name) => (
                    image_name,
                    Some(IMAGE_FALLBACK_ICON.to_string()),
                    vec!["clipboard".to_string(), "image".to_string()],
                ),
                None => {
                    let preview = match raw_preview {
                        Some(p) if p.chars().count() > 80 => {
                            let truncated: String = p.chars().take(77).collect();
                            format!("{}...", truncated)
                        }
                        Some(p) => p.to_string(),
                        None => "[binary data]".to_string(),
                    };
                    let preview_clean = preview
                        .replace('\n', " ")
                        .replace('\r', "")
                        .replace('\t', " ");
                    (
                        preview_clean,
                        Some(PROVIDER_ICON.to_string()),
                        vec!["clipboard".to_string()],
                    )
                }
            };

            // Paste this entry. cliphist takes the entry id as an argument;
            // the old `echo '<id>' | cliphist decode` form piped "<id>\n",
            // which cliphist Atoi'd (newline included) and rejected.
            let command = format!(
                "cliphist decode '{}' | wl-copy",
                clip_id.replace('\'', "'\\''")
            );

            items.push(LaunchItem {
                // id carries the real cliphist id (not a positional index) so
                // refresh() can decode the entry's thumbnail.
                id: format!("clipboard:{}", clip_id),
                name,
                description: Some("Copy to clipboard".to_string()),
                icon,
                provider: ProviderType::Plugin(TYPE_ID.into()),
                command,
                terminal: false,
                activation: ItemActivation::Pipe {
                    source_program: "cliphist".into(),
                    source_args: vec!["decode".into(), clip_id.to_string()],
                    dest_program: "wl-copy".into(),
                    dest_args: Vec::new(),
                },
                tags,
                source: ItemSource::Core,
            });
        }

        items
    }

    /// Cache path for a decoded clipboard thumbnail. Extension-less — GTK's
    /// `Image::from_file` sniffs the format from content.
    fn thumb_path(clip_id: &str) -> Option<PathBuf> {
        // cliphist ids are numeric; accepting nothing else also prevents path
        // traversal and keeps cache pruning scoped to files Owlry owns.
        if !Self::valid_clip_id(clip_id) {
            return None;
        }
        Some(paths::owlry_cache_dir()?.join("clip-thumbs").join(clip_id))
    }

    fn valid_clip_id(clip_id: &str) -> bool {
        !clip_id.is_empty() && clip_id.bytes().all(|byte| byte.is_ascii_digit())
    }

    /// Remove cached thumbnails whose cliphist ids are no longer present.
    ///
    /// Only numeric files and Owlry's numeric `.tmp` files are candidates;
    /// unrelated files and every directory are deliberately left untouched.
    fn prune_thumb_cache_dir(
        cache_dir: &Path,
        active_image_ids: &HashSet<&str>,
    ) -> io::Result<usize> {
        let entries = match std::fs::read_dir(cache_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        };
        let mut removed = 0;

        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() && !file_type.is_symlink() {
                continue;
            }

            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let (clip_id, is_temporary) = match name.strip_suffix(".tmp") {
                Some(id) if Self::valid_clip_id(id) => (id, true),
                _ if Self::valid_clip_id(name) => (name, false),
                _ => continue,
            };

            if is_temporary || !active_image_ids.contains(clip_id) {
                std::fs::remove_file(entry.path())?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Decode an image clipboard entry to a cached file, returning its absolute
    /// path. Cached by clip id; a present file is reused without re-decoding.
    fn decode_thumb(clip_id: &str) -> Option<String> {
        let path = Self::thumb_path(clip_id)?;
        if !path.exists() {
            std::fs::create_dir_all(path.parent()?).ok()?;
            // cliphist takes the entry id as an argument and writes the raw
            // bytes to stdout. Fails (exit 1) on a missing id → keep fallback.
            let out = Command::new("cliphist")
                .arg("decode")
                .arg(clip_id)
                .output()
                .ok()?;
            if !out.status.success() || out.stdout.is_empty() {
                return None;
            }
            // Atomic publish: a killed/partial decode must never leave a corrupt
            // thumbnail that the path.exists() check would then serve forever.
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, &out.stdout).ok()?;
            std::fs::rename(&tmp, &path).ok()?;
        }
        Some(path.to_string_lossy().into_owned())
    }
}

impl Provider for ClipboardProvider {
    fn name(&self) -> &str {
        "Clipboard"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        self.items.clear();

        if !Self::has_cliphist() {
            return;
        }

        let output = match Command::new("cliphist").arg("list").output() {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut items = Self::parse_cliphist_output(&stdout, self.max_entries);

        let active_image_ids: HashSet<&str> = items
            .iter()
            .filter(|item| item.tags.iter().any(|tag| tag == "image"))
            .filter_map(|item| item.id.strip_prefix("clipboard:"))
            .collect();
        if let Some(cache_dir) = paths::owlry_cache_dir().map(|dir| dir.join("clip-thumbs")) {
            match Self::prune_thumb_cache_dir(&cache_dir, &active_image_ids) {
                Ok(removed) if removed > 0 => {
                    log::debug!("pruned {removed} stale clipboard thumbnail(s)");
                }
                Ok(_) => {}
                Err(error) => log::warn!("failed to prune clipboard thumbnails: {error}"),
            }
        }

        // Decode a thumbnail for each image entry and point its icon at the
        // cached file. Failures keep the image-x-generic fallback icon.
        for item in &mut items {
            if item.tags.iter().any(|t| t == "image")
                && let Some(clip_id) = item.id.strip_prefix("clipboard:")
                && let Some(path) = Self::decode_thumb(clip_id)
            {
                item.icon = Some(path);
            }
        }

        self.items = items;
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn backing_program(&self) -> Option<&str> {
        Some(CLIPHIST)
    }

    fn prefix(&self) -> Option<&str> {
        Some(":clip")
    }

    fn icon(&self) -> &str {
        PROVIDER_ICON
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Clipboard")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("clipboard entries")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_provider_new() {
        let p = ClipboardProvider::new();
        assert!(p.items.is_empty());
        assert_eq!(p.max_entries, DEFAULT_MAX_ENTRIES);
    }

    #[test]
    fn test_preview_truncation() {
        // Long ASCII strings get truncated char-safely to 80 chars with "..." suffix.
        let long_text = "a".repeat(100);
        let truncated = if long_text.chars().count() > 80 {
            let t: String = long_text.chars().take(77).collect();
            format!("{}...", t)
        } else {
            long_text.clone()
        };
        assert_eq!(truncated.chars().count(), 80);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_preview_truncation_utf8() {
        // Multi-byte UTF-8 chars must not split mid-codepoint.
        let utf8_text = "├── ".repeat(30);
        let truncated = if utf8_text.chars().count() > 80 {
            let t: String = utf8_text.chars().take(77).collect();
            format!("{}...", t)
        } else {
            utf8_text.clone()
        };
        assert_eq!(truncated.chars().count(), 80);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_preview_cleaning() {
        let dirty = "line1\nline2\tcolumn\rend";
        let clean = dirty
            .replace('\n', " ")
            .replace('\r', "")
            .replace('\t', " ");
        assert_eq!(clean, "line1 line2 columnend");
    }

    #[test]
    fn test_command_escaping() {
        let output = "7\ttext with ' quote\n";
        let items = ClipboardProvider::parse_cliphist_output(output, DEFAULT_MAX_ENTRIES);
        // Paste passes the entry id as a cliphist argument (not piped stdin).
        assert_eq!(items[0].command, "cliphist decode '7' | wl-copy");
    }

    #[test]
    fn test_has_cliphist_runs() {
        // Just ensure it doesn't panic — cliphist may or may not be installed.
        let _ = ClipboardProvider::has_cliphist();
    }

    #[test]
    fn parse_cliphist_output_builds_items_with_plugin_provider_type() {
        let output = "1\thello world\n2\tanother entry\n";
        let items = ClipboardProvider::parse_cliphist_output(output, DEFAULT_MAX_ENTRIES);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "hello world");
        assert_eq!(items[0].provider, ProviderType::Plugin("clipboard".into()));
        assert_eq!(items[0].source, ItemSource::Core);
        assert!(items[0].command.contains("cliphist decode"));
        assert!(items[0].command.contains("wl-copy"));
        assert!(!items[0].terminal);
    }

    #[test]
    fn parse_cliphist_output_respects_max_entries() {
        let output = "1\ta\n2\tb\n3\tc\n4\td\n";
        let items = ClipboardProvider::parse_cliphist_output(output, 2);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn parse_cliphist_output_handles_missing_preview_as_binary() {
        let output = "42\n";
        let items = ClipboardProvider::parse_cliphist_output(output, DEFAULT_MAX_ENTRIES);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "[binary data]");
    }

    #[test]
    fn provider_type_is_clipboard_plugin() {
        // CLI back-compat: `-m clip` / `:clip` resolve to this type_id.
        let p = ClipboardProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("clipboard".into()));
    }

    #[test]
    fn provider_metadata_exposes_clip_prefix_and_labels() {
        let p = ClipboardProvider::new();
        assert_eq!(p.prefix(), Some(":clip"));
        assert_eq!(p.icon(), "edit-paste");
        assert_eq!(p.tab_label(), Some("Clipboard"));
        assert_eq!(p.search_noun(), Some("clipboard entries"));
    }

    #[test]
    fn image_display_name_detects_image_with_dimensions() {
        let name =
            ClipboardProvider::image_display_name("[[ binary data 1.27 MiB png 1920x1080 ]]");
        assert_eq!(name.as_deref(), Some("Image — png 1920x1080"));
    }

    #[test]
    fn image_display_name_detects_image_without_dimensions() {
        let name = ClipboardProvider::image_display_name("[[ binary data 4 KiB gif ]]");
        assert_eq!(name.as_deref(), Some("Image — gif"));
    }

    #[test]
    fn image_display_name_ignores_text_and_non_image_binary() {
        assert_eq!(ClipboardProvider::image_display_name("hello world"), None);
        // Binary but not a known image type.
        assert_eq!(
            ClipboardProvider::image_display_name("[[ binary data 2 MiB pdf ]]"),
            None
        );
    }

    #[test]
    fn parse_cliphist_output_marks_image_entries() {
        let output = "523\t[[ binary data 1.27 MiB png 1920x1080 ]]\n524\tplain text\n";
        let items = ClipboardProvider::parse_cliphist_output(output, DEFAULT_MAX_ENTRIES);
        assert_eq!(items.len(), 2);

        // Image entry: clean name, image tag, fallback icon, id carries clip id.
        assert_eq!(items[0].name, "Image — png 1920x1080");
        assert!(items[0].tags.iter().any(|t| t == "image"));
        assert_eq!(items[0].icon.as_deref(), Some(IMAGE_FALLBACK_ICON));
        assert_eq!(items[0].id, "clipboard:523");

        // Text entry unchanged.
        assert_eq!(items[1].name, "plain text");
        assert!(!items[1].tags.iter().any(|t| t == "image"));
        assert_eq!(items[1].id, "clipboard:524");
    }

    #[test]
    fn thumb_path_rejects_traversal_ids() {
        assert!(ClipboardProvider::thumb_path("../etc/passwd").is_none());
        assert!(ClipboardProvider::thumb_path("a/b").is_none());
        assert!(ClipboardProvider::thumb_path("not-a-number").is_none());
        assert!(ClipboardProvider::thumb_path("").is_none());
    }

    #[test]
    fn prune_thumb_cache_removes_only_owned_stale_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("11"), b"active").expect("active thumb");
        std::fs::write(dir.path().join("22"), b"stale").expect("stale thumb");
        std::fs::write(dir.path().join("11.tmp"), b"partial").expect("temporary thumb");
        std::fs::write(dir.path().join("notes.txt"), b"foreign").expect("foreign file");
        std::fs::create_dir(dir.path().join("33")).expect("numeric directory");
        let active = HashSet::from(["11"]);

        let removed =
            ClipboardProvider::prune_thumb_cache_dir(dir.path(), &active).expect("prune cache");

        assert_eq!(removed, 2);
        assert!(dir.path().join("11").exists());
        assert!(!dir.path().join("22").exists());
        assert!(!dir.path().join("11.tmp").exists());
        assert!(dir.path().join("notes.txt").exists());
        assert!(dir.path().join("33").is_dir());
    }

    #[test]
    fn prune_thumb_cache_tolerates_missing_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing");
        assert_eq!(
            ClipboardProvider::prune_thumb_cache_dir(&missing, &HashSet::new())
                .expect("missing cache is empty"),
            0
        );
    }
}
