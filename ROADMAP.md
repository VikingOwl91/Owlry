# Owlry Roadmap

Feature ideas and future development plans. For the v2 rewrite story (where 14 packages and the dynamic plugin system went), see [docs/RESTRUCTURE-V2.md](docs/RESTRUCTURE-V2.md).

## Locked-in for an upcoming 2.x release

### Lua-driven configuration — **shipped in 2.1**
`~/.config/owlry/owlry.lua` is the canonical config from 2.1 onwards (TOML stays supported as a back-compat fallback until 3.0). Real Lua 5.4 via vendored `mlua`. The full surface is documented in [`docs/lua-api.md`](docs/lua-api.md):

- `owlry.set` / `owlry.providers` / `owlry.tabs` / `owlry.theme` / `owlry.profiles` for declarative config
- `owlry.provider {}` for user-defined providers (Hyprland-style configs-as-code)
- `owlry.util.{shell, shell_lines, read_file, glob, env, hostname}` host helpers inside provider callbacks
- Hot reload on save via the `notify` crate — broken edits surface as desktop notifications and keep the previous state live
- `owlry migrate-config [--force]` — deterministic TOML → Lua migration
- `owlry config validate` — categorised report (errors exit 1, warnings exit 2)

```lua
owlry.set { theme = "owl", width = 850 }
owlry.providers { "app", "cmd", "power", "systemd" }
owlry.tabs { "app", "cmd", "uuctl" }

owlry.provider {
  id = "hs", prefix = ":hs", tab_label = "Shutdown",
  items = function() return { { name = "Lock", command = "hyprlock" } } end,
}
```

**2.2 follow-ups:** dynamic (per-keystroke) Lua providers via `dynamic = true`; `owlry.bind` for runtime keybindings; `owlry.util.http_get`.

### Widget providers return
Weather, MPRIS media controls, and pomodoro timer were deferred from 2.0 while the widget-row UI is redesigned. They'll come back as a feature group once the placement model is settled. ([D20 in the v2 plan](docs/RESTRUCTURE-V2.md).)

### Bookmarks return
Firefox + Chromium bookmarks were deferred from 2.0 — the `rusqlite` dep used to read Firefox's `places.sqlite` made the AUR chroot build brittle (`libsqlite3-sys`'s `bundled` feature kept slipping out of the resolved feature graph). Returns when we wire up a pure-Rust path: Chromium's bookmarks file is already JSON, and Firefox exposes JSON backups under `~/.mozilla/firefox/<profile>/bookmarkbackups/`. No SQLite required.

---

## High Value, Low Effort

### ~~Frecency pruning — expose thresholds~~ — **landed (ships in 2.4)**
The startup prune thresholds are now configurable via `[providers]`: `frecency_max_entries` (0 = unbounded), `frecency_max_age_days` (0 = no age pruning), `frecency_min_launches` — defaulting to the historical 5000/180/3. Honored by the daemon and the daemonless (dmenu / fallback) stores alike. Applied at store load; a daemon restart is needed to re-prune after a config change.

### `:recent` prefix
Show last N launched items. Note: `FrecencyEntry` stores only `launch_count` + `last_launch` keyed by `item.id` — no name/command — so the daemon must resolve ids against its in-memory provider items and drop dangling ones (e.g. uninstalled apps).

### Clipboard images
`cliphist` supports images. Extend the clipboard provider to show image thumbnails in results.

### ~~Dynamic providers in `owlry doctor`~~ — **landed (ships in 2.4)**
`available_providers()` already enumerated the dynamic providers; `doctor` now labels each `[static]` / `[dynamic]` via a `kind` field on `ProviderDesc`.

### ~~Orphan reporting in `owlry doctor`~~ — **landed (ships in 2.4)**
`doctor` now reports stale system-level remnants under `[stale system files]` (path + size + `pacman -Qo` ownership): the pre-v2 `/usr/lib/owlry` runtimes dir, old `owlryd.{service,socket}` units, and an uncompressed `owlry.1` man page *only when* the packaged `.gz` also exists (shadowing). Report-only — `doctor` never deletes.

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

This is the difference between a launcher and a command palette. The `Provider::submenu_actions()` trait method (added in 2.0 for the systemd provider) is the foundation.

### Result action capture
Calculator shows `= 5+3 → 8`. Allow pressing Tab or Ctrl+C to copy the result to clipboard instead of launching. Useful for calculator output, file paths, URLs.

### Split the 1000+ LOC files
`providers/mod.rs`, `ui/main_window.rs`, `providers/converter/units.rs` are all over a thousand lines each. Carve them into focused modules to make code review and onboarding less painful. (Phase 5 hygiene in the v2 plan.)

---

## Bigger Bets

### Window switcher with live thumbnails
A `windows` provider using Wayland screencopy to show live thumbnails of open windows. Hyprland and Sway expose window lists via IPC. Could replace Alt+Tab.

### Cross-device bookmark sync
Firefox and Chrome sync bookmarks across devices. Parse sync metadata to show "recently added on other devices".

### Natural language commands
Parse simple natural language into system commands:

```
"shutdown in 30 minutes" → systemd-run --user --on-active=30m systemctl poweroff
"remind me in 1 hour"    → notify-send scheduled via at/systemd timer
"volume 50%"             → wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.5
```

Local pattern matching, no cloud required.

### Drop the daemon
After Lua config lands and we profile startup honestly: if the daemon's keep-providers-warm justification doesn't hold up against an mmap'd cache, collapse to a single-process model. Eliminates the socket protocol, IPC types, and most of `client.rs` + `server.rs`. ([D17 deferred this decision past 2.0.](docs/RESTRUCTURE-V2.md))

---

## Technical Debt

### `expr-solver-lib` evaluation
The calculator's `expr-solver-lib` dep is small and old. If it stagnates further or becomes unsupported, switch to `evalexpr` v13+ which is actively maintained.

### Submenu protocol redesign
The `SUBMENU:<plugin_id>:<data>` string-encoded command is a workable hack. A typed IPC variant would be cleaner — keep the surface in `Request::Submenu` but stop overloading the `command` field. (Phase 5 hygiene.)

### Double-daemon spawn
Pre-2.0, the systemd unit and a Hyprland `exec-once = owlryd` could both spawn a daemon. The 2.0 socket-activation path (`owlry.socket`) eliminates the need for an explicit autostart — verify nothing else still launches the daemon directly. (Phase 5 hygiene.)

---

## Priority

If we had to pick one for the next release: **Actions on any result**. It transforms every provider from "search and launch" to "search and do anything". The ROI is massive and the trait foundation is already in 2.0.
