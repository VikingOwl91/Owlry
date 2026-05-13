# Owlry Roadmap

Feature ideas and future development plans. For the v2 rewrite story (where 14 packages and the dynamic plugin system went), see [docs/RESTRUCTURE-V2.md](docs/RESTRUCTURE-V2.md).

## Locked-in for an upcoming 2.x release

### Lua-driven configuration (Phase 3)
Replace `config.toml` with `~/.config/owlry/init.lua`. The config is real Lua, evaluated at startup via embedded `mlua` (Lua 5.4). User-defined providers, keybindings, and theme overrides all live in the same file — Hyprland-style configs-as-code. Ships with `owlry migrate-config` (TOML → init.lua) and hot-reload on save.

```lua
local owlry = require("owlry")

owlry.set { theme = "owl", width = 850, tabs = { "app", "cmd", "uuctl" } }
owlry.providers { "app", "cmd", "power", "bookmarks", "systemd" }

owlry.provider {
  id = "hs", prefix = ":hs", tab_label = "Shutdown",
  items = function() return { { name = "Lock", command = "hyprlock" } } end,
}
```

### Widget providers return
Weather, MPRIS media controls, and pomodoro timer were deferred from 2.0 while the widget-row UI is redesigned. They'll come back as a feature group once the placement model is settled. ([D20 in the v2 plan](docs/RESTRUCTURE-V2.md).)

---

## High Value, Low Effort

### Frecency pruning
Add `max_entries` and `max_age_days` config options. Prune old entries on startup to prevent `frecency.json` from growing unbounded.

### `:recent` prefix
Show last N launched items. Data already exists in frecency.json — just needs a provider to surface it.

### Clipboard images
`cliphist` supports images. Extend the clipboard provider to show image thumbnails in results.

### Dynamic providers in `owlry doctor`
Today `doctor` lists static providers only. Surface the dynamic ones (calc, conv, websearch, filesearch) too.

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
