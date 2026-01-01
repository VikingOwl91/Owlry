# Owlry Roadmap

Feature ideas and future development plans for Owlry.

## High Value, Low Effort

### Plugin hot-reload
Detect `.so` file changes in `/usr/lib/owlry/plugins/` and reload without restarting the launcher. The loader infrastructure already exists.

### Frecency pruning
Add `max_entries` and `max_age_days` config options. Prune old entries on startup to prevent `frecency.json` from growing unbounded.

### `:recent` prefix
Show last N launched items. Data already exists in frecency.json — just needs a provider to surface it.

### Clipboard images
`cliphist` supports images. Extend the clipboard plugin to show image thumbnails in results.

---

## Medium Effort, High Value

### Actions on any result
Generalize the submenu system beyond systemd. Every result type gets contextual actions:

| Provider | Actions |
|----------|---------|
| Applications | Open, Open in terminal, Show .desktop location |
| Files | Open, Open folder, Copy path, Delete |
| SSH | Connect, Copy hostname, Edit config |
| Bookmarks | Open, Copy URL, Open incognito |
| Clipboard | Paste, Delete from history |

This is the difference between a launcher and a command palette.

### Plugin settings UI
A `:settings` provider that lists installed plugins and their configurable options. Edit values inline, writes to `config.toml`.

### Result action capture
Calculator shows `= 5+3 → 8`. Allow pressing Tab or Ctrl+C to copy the result to clipboard instead of "launching" it. Useful for calculator, file paths, URLs.

---

## Bigger Bets

### Window switcher with live thumbnails
A `windows` plugin using Wayland screencopy to show live thumbnails of open windows. Hyprland and Sway expose window lists via IPC. Could replace Alt+Tab.

### Cross-device bookmark sync
Firefox and Chrome sync bookmarks across devices. Parse sync metadata to show "recently added on other devices" or "bookmarks from phone".

### Natural language commands
Parse simple natural language into system commands:

```
"shutdown in 30 minutes" → systemd-run --user --on-active=30m systemctl poweroff
"remind me in 1 hour"    → notify-send scheduled via at/systemd timer
"volume 50%"             → wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.5
```

Local pattern matching, no AI/cloud required.

### Plugin marketplace
A curated registry of third-party Lua/Rune plugins with one-command install:

```bash
owlry plugin install github-notifications
owlry plugin install todoist
owlry plugin install spotify-controls
```

The script runtimes make this viable without recompiling.

---

## Technical Debt

### Split monorepo for user build efficiency
Currently, a small core fix requires all 16 AUR packages to rebuild (same source tarball). Split into 3 repos:

| Repo | Contents | Versioning |
|------|----------|------------|
| `owlry` | Core binary | Independent |
| `owlry-plugin-api` | ABI interface (crates.io) | Semver, conservative |
| `owlry-plugins` | 13 plugins + 2 runtimes | Independent per plugin |

**Execution order:**
1. Publish `owlry-plugin-api` to crates.io
2. Update monorepo to use crates.io dependency
3. Create `owlry-plugins` repo, move plugins + runtimes
4. Slim current repo to core-only
5. Update AUR PKGBUILDs with new source URLs

**Benefit:** Core bugfix = 1 rebuild. Plugin fix = 1 rebuild. Third-party plugins possible via crates.io.

### Replace meval with evalexpr
`meval` depends on `nom v1.2.4` which will be rejected by future Rust versions. Migrate calculator plugin and Lua runtime to `evalexpr` v13+.

### Plugin API backwards compatibility
When `API_VERSION` increments, provide a compatibility shim so v3 plugins work with v4 core. Prevents ecosystem fragmentation.

### Per-plugin configuration
Current flat `[providers]` config doesn't scale. Design a `[plugins.weather]`, `[plugins.pomodoro]` structure that plugins can declare and the core validates.

---

## Priority

If we had to pick one: **Actions on any result**. It transforms every provider from "search and launch" to "search and do anything". The ROI is massive.
