# Owlry Lua API

The owlry Lua configuration system. Lands in **2.1** as an opt-in preview alongside TOML; becomes the canonical config format in **3.0** when TOML support is removed.

> **Status:** design spec for Phase 3 of the v2 rewrite plan. Becomes the user-facing reference doc once Phase 3 ships.

---

## 1. Why Lua

Owlry's 2.0 config is `~/.config/owlry/config.toml`. TOML is fine for flat key/value settings, but it can't express the things people actually want from a launcher config:

- "Use a different SSH command for `:ssh host-foo` than for everything else"
- "Add a Hyprland shutdown menu as a `:hs` provider"
- "Build the bookmarks list from `~/Documents/links.md` instead of Firefox"
- "Conditionally enable systemd-units only on machines where `$HOSTNAME` matches a regex"

These are all one-liners in a real programming language. They're impossible in TOML.

Owlry 2.1 ships a Lua-driven config: the file is real Lua 5.4, evaluated at startup, and the same file holds:

1. **Global settings** (theme, dimensions, terminal command)
2. **Enabled providers** (which built-ins run)
3. **Tab layout** (which providers appear as tab buttons)
4. **User-defined providers** (custom search sources)
5. **Theme overrides**

This is the same model Hyprland adopted (hyprlang) and that AwesomeWM / wezterm / xmonad have always used: **the config language IS the extension language**. No second-class scripting bolted on alongside a static config.

---

## 2. File location & loading order

The config file is **`~/.config/owlry/owlry.lua`**.

> Not `init.lua` — the name `owlry.lua` is intentional brand identity. `init.lua` is Lua's `require` entry-point convention; this file isn't loaded by `require`, it's loaded explicitly by owlry, so the convention doesn't apply.

### Resolution

On startup the daemon resolves config in this order:

1. **`$XDG_CONFIG_HOME/owlry/owlry.lua`** (default: `~/.config/owlry/owlry.lua`) — if this file exists, it's the **only** source of truth. TOML is ignored entirely when Lua is present.
2. **`$XDG_CONFIG_HOME/owlry/config.toml`** — fallback for 2.x users who haven't migrated. Behavior unchanged from 2.0.
3. **Shipped defaults** — `/usr/share/owlry/default.lua` (a stub that calls `owlry.set {}` with reasonable defaults). Used when neither user file exists.

In **3.0**, step 2 is removed. Users who haven't migrated get a clear error from `owlry config validate` and a pointer to `owlry migrate-config`.

### Why Lua "wins" over TOML when both exist

To avoid the "did you remember to delete `config.toml`?" footgun. If you ever write an `owlry.lua`, that's your config — the TOML next to it doesn't quietly partial-override anything. Clear winner: predictable behavior.

---

## 3. Quick reference

```lua
-- ~/.config/owlry/owlry.lua
local owlry = require("owlry")

-- ──────────────────────────────────────────────────────────────────────────
-- Global settings
-- ──────────────────────────────────────────────────────────────────────────
owlry.set {
  theme      = "owl",
  width      = 850,
  height     = 650,
  font_size  = 14,
  terminal   = "kitty",
  use_uwsm   = false,
  show_icons = true,
  max_results = 100,
}

-- ──────────────────────────────────────────────────────────────────────────
-- Which providers run (the "enabled set")
-- ──────────────────────────────────────────────────────────────────────────
owlry.providers {
  "app", "cmd", "calc", "conv", "power",     -- always-on built-ins
  "systemd", "ssh", "websearch", "filesearch", "emoji", "clipboard",
}

-- ──────────────────────────────────────────────────────────────────────────
-- Which providers get a tab button in the UI header bar (Ctrl+1..N)
-- Must be a subset of the enabled set. If omitted, defaults to all providers.
-- ──────────────────────────────────────────────────────────────────────────
owlry.tabs { "app", "cmd", "uuctl" }

-- ──────────────────────────────────────────────────────────────────────────
-- A user-defined provider — automatically joins the enabled set.
-- ──────────────────────────────────────────────────────────────────────────
owlry.provider {
  id         = "hs",
  prefix     = ":hs",
  tab_label  = "Shutdown",
  icon       = "system-shutdown",
  items = function()
    return {
      { name = "Lock",     command = "hyprlock" },
      { name = "Shutdown", command = "systemctl poweroff" },
      { name = "Reboot",   command = "systemctl reboot" },
    }
  end,
}

-- ──────────────────────────────────────────────────────────────────────────
-- Theme selection (or inline definition; see §4.5)
-- ──────────────────────────────────────────────────────────────────────────
owlry.theme("catppuccin-mocha")
```

---

## 4. API reference

The `owlry` module is the only thing in scope. `require("owlry")` returns it. All functions are callable in any order, multiple times — last-write-wins on most settings; some accumulate (see each function's note).

### 4.1 `owlry.set { ... }` — global settings

Sets top-level config values. Takes a table of key-value pairs. Calling `owlry.set` multiple times **merges** — later calls override earlier values for the same key. Keys not mentioned keep their default.

| Key | Type | Default | What it does |
|---|---|---|---|
| `theme` | `string` | `nil` (system theme) | Theme name (see §4.5 for options) |
| `width` | `integer` | `850` | Launcher window width in pixels |
| `height` | `integer` | `650` | Launcher window height in pixels |
| `font_size` | `integer` | `14` | Base font size in points |
| `border_radius` | `integer` | `12` | Window corner radius in pixels |
| `terminal` | `string \| nil` | autodetected | Terminal command for items marked `terminal=true`. Falls back to `$TERMINAL` → `xdg-terminal-exec` → common terminals |
| `use_uwsm` | `boolean` | `false` | Launch apps via `uwsm app --` for systemd session integration |
| `show_icons` | `boolean` | `true` | Show provider icons in results |
| `max_results` | `integer` | `100` | Cap on results returned per query |
| `frecency` | `boolean` | `true` | Boost frequently/recently used items |
| `frecency_weight` | `number` | `0.3` | Frecency boost weight (0.0 = off, 1.0 = strong) |
| `search_engine` | `string` | `"duckduckgo"` | Engine for `:web` / `?` queries (see §6) |

**Example:**

```lua
owlry.set { theme = "owl", width = 900 }
owlry.set { font_size = 16 }   -- merged: theme + width still applied
```

Unknown keys produce a `owlry config validate` warning, not an error — forward-compat for 2.2+ keys.

---

### 4.2 `owlry.providers { ... }` — enabled set

Lists **which providers run**. Takes a sequence (array) of provider IDs. **Order doesn't matter.**

```lua
owlry.providers { "app", "cmd", "calc", "conv", "power" }
```

**Rules:**

- A provider must be in this list to produce any results. If it's not enabled, neither typing its prefix (`:foo`) nor including it in `owlry.tabs` will make it active.
- Built-in IDs are: `app`, `cmd`, `dmenu`, `calc`, `conv`, `power`, `systemd` (alias: `uuctl`), `ssh`, `clipboard`, `emoji`, `websearch`, `filesearch`. Pre-2.0 aliases (`sys`, `system` → `power`; `uuctl` → `systemd`) still work.
- User providers defined via `owlry.provider {}` **auto-join the enabled set** — you don't list them here unless you want to.
- Calling `owlry.providers` multiple times **replaces** the list (it's not additive). Use one call.
- If a feature isn't compiled into the binary (e.g. someone built with `--no-default-features`), the provider is silently ignored at runtime — `owlry doctor` reports the mismatch.

**Default if omitted:** all compiled-in providers are enabled. (The AUR build has `--features full`, so this defaults to everything.)

---

### 4.3 `owlry.tabs { ... }` — UI tab buttons

Lists which providers appear as **tab buttons** in the header bar. Tabs cycle with `Tab` / `Shift+Tab` and can be jumped to with `Ctrl+1..9`.

```lua
owlry.tabs { "app", "cmd", "uuctl" }
```

**Rules:**

- Each entry must be in `owlry.providers` (or be a `owlry.provider {}` user provider). Listing an unknown ID produces a `owlry config validate` warning and is silently dropped at runtime.
- Order is preserved — `Ctrl+1` targets the first entry, `Ctrl+2` the second, etc.
- The implicit "All" tab is always present at position 0 (`Ctrl+0`).
- **Default if omitted:** all enabled providers get tabs in the order they were registered.
- **`owlry.tabs {}` (empty)** is valid: hides all tab buttons. The "All" tab is still there.

**The `tabs` ⊆ `providers` rule, made obvious:**

```
provider IS enabled         ┌──────────────────────────────────────────┐
(runs, contributes results) │ app cmd power calc conv systemd ssh emoji│   <- owlry.providers
                            └─────┬─────┬───────────────┬──────────────┘
                                  │     │               │
                            ┌─────▼─────▼───────────────▼──────────────┐
                            │ app cmd                  uuctl           │   <- owlry.tabs
                            │ ↑1   ↑2                  ↑3              │      (subset; Ctrl+N order)
                            └──────────────────────────────────────────┘
```

A provider in `providers` but not in `tabs` is fully searchable (auto mode + `:prefix` works), it just doesn't get a permanent tab button. This is the right place for calc/conv/websearch/filesearch — they're triggered with `=` / `>` / `?` / `/` and don't need to take up tab-bar real estate.

---

### 4.4 `owlry.provider { ... }` — user-defined provider

Defines a custom provider that runs alongside the built-ins. The minimum useful shape:

```lua
owlry.provider {
  id     = "hs",                         -- type_id, used internally
  prefix = ":hs",                        -- search prefix (optional)
  items  = function(query)               -- called to populate results
    return {
      { name = "Lock",     command = "hyprlock" },
      { name = "Shutdown", command = "systemctl poweroff" },
    }
  end,
}
```

**Full field reference:**

| Field | Type | Required | Default | What it does |
|---|---|---|---|---|
| `id` | `string` | yes | — | Unique provider ID. Used for filter matching (`-m <id>`) and tab listings. Must be lowercase, alphanumeric + `-`/`_`. |
| `items` | `function(query: string) -> table` | yes | — | Returns the list of items. Called on every query if `dynamic = true`, otherwise cached after the first call (see `refresh` below). |
| `prefix` | `string \| nil` | no | `nil` | Search prefix (e.g. `":hs "`). Typing the prefix narrows results to this provider. |
| `name` | `string` | no | `id` capitalised | Display name shown in `owlry providers`. |
| `tab_label` | `string` | no | `name` | Tab button text. |
| `icon` | `string` | no | `"application-x-addon"` | XDG icon name shown next to items. |
| `search_noun` | `string` | no | `id` | Search placeholder noun (e.g. "Search SSH hosts…"). |
| `dynamic` | `boolean` | no | `false` | If true, `items` is called on every keystroke. If false, `items` is called once at startup and cached. **2.1 ships only `false` (static); `true` arrives in 2.2.** |
| `priority` | `integer` | no | `0` | Tiebreaker for ordering when multiple providers match. Higher = first. |

**The item table:**

Each entry in the table returned by `items` is itself a table:

| Field | Type | Required | What it does |
|---|---|---|---|
| `name` | `string` | yes | Item title shown in the result row |
| `command` | `string` | yes | Shell command executed on launch |
| `description` | `string` | no | Secondary text below the title |
| `icon` | `string` | no | Per-item icon (overrides provider icon) |
| `terminal` | `boolean` | no | If true, launches inside a terminal |
| `tags` | `table` | no | List of tag strings for `:tag:X` filtering |

**Multiple `owlry.provider` calls accumulate** — each call registers an additional provider. Calling twice with the same `id` overrides the previous registration (warning emitted).

**Lifecycle:** the provider's `items` function is called by the daemon. It runs in the Lua state with the full host API available (see §5). It must return within a reasonable time — slow `items` functions block the UI. If you need to do work that takes >100ms, cache it externally and refresh on demand (see `dynamic = true` for the 2.2 path).

---

### 4.5 `owlry.theme(...)` — theme selection

Two forms:

```lua
-- Built-in or user theme by name
owlry.theme("catppuccin-mocha")
```

Looks up the theme in order: `~/.config/owlry/themes/{name}.css` → bundled theme. Built-in themes: `owl`, `catppuccin-mocha`, `nord`, `rose-pine`, `dracula`, `gruvbox-dark`, `tokyo-night`, `solarized-dark`, `one-dark`, `apex-neon`.

```lua
-- Inline color overrides (merges on top of selected theme or system defaults)
owlry.theme {
  background       = "#1e1e2e",
  background_secondary = "#313244",
  border           = "#45475a",
  text             = "#cdd6f4",
  text_secondary   = "#a6adc8",
  accent           = "#cba6f7",
  accent_bright    = "#f5c2e7",
  -- Per-provider badge colors (optional)
  badge_app        = "#a6e3a1",
  badge_cmd        = "#fab387",
  badge_power      = "#f38ba8",
  badge_uuctl      = "#9ece6a",
}
```

The two forms can be combined — call `owlry.theme(name)` first, then `owlry.theme { ... }` to layer overrides on top.

---

## 5. The host API in Lua scope

User code runs in a Lua 5.4 state with the `mlua` runtime. Beyond the standard library, owlry exposes a small **host API** for things provider code is likely to need.

### 5.1 What's available in `owlry.lua`

- **Full Lua 5.4 stdlib** — `math`, `string`, `table`, `os`, `io`, `coroutine`, `package`. No sandboxing. It's your config file on your machine; deal with it.
- **The `owlry` module** — `set`, `providers`, `tabs`, `provider`, `theme`. (Documented above.)
- **Convenience helpers under `owlry.util`** (added incrementally; ship in 2.1):
  - `owlry.util.shell(cmd) -> string` — run a shell command, return stdout. Blocking; use sparingly.
  - `owlry.util.shell_lines(cmd) -> table` — same, split into a list of lines.
  - `owlry.util.read_file(path) -> string|nil` — read a file's contents, nil if missing.
  - `owlry.util.glob(pattern) -> table` — list paths matching a glob.
  - `owlry.util.env(name, default?) -> string` — read an env var with a fallback.
  - `owlry.util.hostname() -> string` — current hostname.

### 5.2 What's NOT in scope

- **No process spawning at config-load time for non-trivial setup.** Provider `items` functions can shell out via `owlry.util.shell`, but the top-level config eval should be fast. If the daemon spends >500ms running `owlry.lua` at startup, something's wrong.
- **No network helpers in 2.1.** If a user provider needs HTTP, they can `os.execute("curl ...")` for now. A first-class `owlry.util.http_get` lands in 2.2.
- **No reactive state.** Each provider's `items` function should be stateless (or use local upvalues for caching). Cross-call state via shared mutable globals will work but isn't a supported pattern.

### 5.3 Sandbox stance

**None in 2.1.** This is the user's config file on the user's machine, by analogy to `bashrc` or `init.vim`. If a future "share your config" community emerges, sandbox concerns kick in then. For now: assume the user wrote the file they're running.

---

## 6. How `providers` + `tabs` + `provider {}` compose at runtime

The single most-asked question this section pre-empts.

### 6.1 The three axes

1. **Compiled in** — cargo features at build time. AUR build = all of them. Users on `cargo install --no-default-features` only have a minimal set. Cannot be enabled at runtime.
2. **Enabled** — `owlry.providers { ... }` decides which compiled-in providers actually run. Plus all `owlry.provider {}` user definitions are auto-enabled.
3. **Shown as tab** — `owlry.tabs { ... }` decides which enabled providers get a button in the header bar.

A provider must be **compiled in** AND **enabled** to do anything. Being shown as a tab is purely a UI convenience.

### 6.2 Selection at use time

On top of the three axes, the user picks at launch time:

```bash
owlry                # auto mode — every enabled provider contributes, tabs cycle through
owlry -m auto        # explicit form of above
owlry -m app         # single-mode — only `app` runs, even though others are enabled
owlry --profile dev  # profile (see §6.5) — pre-baked subset of enabled providers
```

Plus the **prefix override** inside the UI: typing `:uuctl foo` narrows the current query to the systemd provider regardless of the mode.

### 6.3 Worked example

Given this config:

```lua
owlry.providers { "app", "cmd", "calc", "conv", "power", "systemd", "websearch" }
owlry.tabs      { "app", "cmd", "systemd" }
owlry.provider  { id = "hs", prefix = ":hs", items = function() ... end }
```

Behavior:

| Invocation / typed prefix | What runs | Tabs shown |
|---|---|---|
| `owlry` (no flags), empty query | every enabled provider scored by frecency. `app`, `cmd`, `calc`, `conv`, `power`, `systemd`, `websearch`, `hs` all contribute | `app`, `cmd`, `systemd`, `hs` (`hs` auto-added because it wasn't in `owlry.tabs` → defaults policy kicks in for user providers; see note below) |
| `owlry` then user types `:hs ` | only the `hs` provider | tabs unchanged |
| `owlry -m app` | only `app` | only the `app` tab is highlighted |
| `owlry`, user types `= 2+3` | calculator catches the `=` trigger, returns `5` | tabs unchanged |
| `owlry`, user types `:foo bar` | nothing (no provider with id `foo`) | tabs unchanged |
| `owlry`, user types `:bookmarks rust` | nothing (`bookmarks` not in `providers` → not enabled, even though it's compiled in) | warning surfaces in `owlry doctor` |

> **Default tab policy when `owlry.tabs` is omitted:** all enabled providers get tabs.
> **When `owlry.tabs` IS present:** user providers from `owlry.provider {}` are NOT auto-added — the user explicitly chose their tab list. To pin a user provider, include its `id` in `owlry.tabs`. (Open: do we want auto-add behavior here? See §10.)

### 6.4 What happens if a config refers to something missing

| Situation | Behavior | Where surfaced |
|---|---|---|
| `owlry.providers` lists a provider not compiled in | Silently dropped at runtime; warning logged | `owlry doctor` lists it under "Not compiled in" |
| `owlry.providers` lists an unknown id | Warning at config-validate time | `owlry config validate` prints the offending line |
| `owlry.tabs` lists an id not in `owlry.providers` | Warning, dropped from tab bar | `owlry config validate` |
| `owlry.provider { items = function() ... end }` errors | Provider returns 0 items for that query; full traceback in daemon log | `owlry doctor` shows the provider as "errored" with the last exception |
| Two `owlry.provider {}` calls with same `id` | Second one wins; warning emitted | `owlry config validate` |

### 6.5 Profiles

Pre-baked alternate enabled sets (a la 2.0's `--profile`). Inline in Lua:

```lua
owlry.profiles {
  dev    = { "app", "cmd", "ssh" },
  media  = { "emoji", "clipboard" },
  minimal = { "app" },
}
```

```bash
owlry --profile dev    # uses { app, cmd, ssh } instead of owlry.providers's full set
```

Profiles override `owlry.providers` for the launch but inherit `owlry.set` / `owlry.theme` / `owlry.tabs`.

---

## 7. Hot reload

The daemon watches `owlry.lua` and any files it `require`s. On save:

1. Re-evaluate the Lua state in an isolated context.
2. If eval succeeds → swap the new config into the running daemon. Providers reload. No window flicker; no socket disconnect.
3. If eval fails → keep the old config alive. The error is logged to the daemon journal and surfaced on the next `owlry doctor` invocation.

There is **no `systemctl reload`** required. Edit the file, save, the next query reflects the change.

Hot reload uses the `notify` filesystem watcher (re-added to deps in Phase 3 — it was removed in v2 demolition).

---

## 8. Validation & error reporting

```bash
owlry config validate
```

Runs the Lua file in dry-run mode (no side effects, no daemon swap). Reports:

- Syntax errors with line numbers.
- Unknown keys in `owlry.set`.
- Unknown IDs in `owlry.providers` / `owlry.tabs`.
- `owlry.tabs` entries not in `owlry.providers`.
- Duplicate `id` in `owlry.provider {}` calls.
- Providers compiled out (warning, not error).

Exit code: `0` on clean, `1` on any error, `2` on warnings only (configurable).

```bash
owlry config show
```

Prints the **resolved** config as TOML (yes, TOML — for diffability and consistency with how `owlry config show` works today). Useful for debugging "what does owlry actually see?"

```bash
owlry config show --lua
```

Prints the resolved config as a Lua file (round-trippable). Useful for `owlry migrate-config` diff comparison.

---

## 9. Migration from TOML

```bash
owlry migrate-config
```

Reads `~/.config/owlry/config.toml`, writes `~/.config/owlry/owlry.lua` with the equivalent settings. Refuses to overwrite an existing `owlry.lua` unless `--force` is passed.

**Mapping:**

| TOML | Lua |
|---|---|
| `[general] theme = "owl"` | `owlry.set { theme = "owl" }` |
| `[general] tabs = ["app", "cmd"]` | `owlry.tabs { "app", "cmd" }` |
| `[providers] app = true` | included in `owlry.providers { ... }` |
| `[providers] app = false` | excluded from `owlry.providers { ... }` |
| `[appearance.colors] badge_app = "..."` | merged into `owlry.theme { badge_app = "..." }` |
| `[profiles.dev] modes = [...]` | `owlry.profiles { dev = { ... } }` |

Edge cases:

- Pre-v2 aliases (`system` config key, `badge_sys` color) are normalized to v2 names (`power`, `badge_power`) in the emitted Lua.
- Comments in the TOML are translated to Lua comments where they survive serialization round-trips. Order is preserved as much as possible.
- Unknown TOML keys (e.g. removed config options) are commented out in the output with a `-- DROPPED:` prefix.

The migrator is **deterministic** — running it twice on the same input produces byte-identical output.

---

## 10. Open questions (resolved before Phase 3 ships)

| Q | Default proposed | Notes |
|---|---|---|
| Auto-add user providers to `owlry.tabs` when tabs is non-empty? | **No** — explicit tabs list is explicit | Easy to revisit; user feedback after 2.1 will decide |
| `owlry.theme` separate vs folded into `owlry.set`? | **Separate** — themes are big enough to deserve their own verb | Matches Hyprland's `general` / `decoration` split |
| Profile keybinds (per-key Lua functions)? | **Deferred to 2.2** | Needs careful design re: which thread runs the function |
| Error reporting depth: traceback in UI banner? | **No in 2.1** — log to journal, surface via `owlry doctor` | UI banner gets noisy fast |
| Allow `owlry.lua` to be a directory of files merged together? | **No** — single file. Users wanting modularity use Lua `require` | |
| Multi-file config via `require`? | **Yes, supported transparently** — same hot-reload watcher follows `require`d files | Standard Lua semantics |

---

## 11. Version & compatibility roadmap

| Owlry version | Lua config | TOML config | Notes |
|---|---|---|---|
| **2.0** (shipped) | not supported | canonical | `owlry migrate-config` exists as a stub |
| **2.1** (Phase 3) | opt-in preview | still works | `owlry migrate-config` becomes functional |
| **2.2** (Phase 3 polish) | preferred; documented | works but warning emitted on load | dynamic providers, `owlry.bind`, `owlry.util.http_get` |
| **3.0** (D18) | canonical | **removed** | `owlry config validate` errors loudly when TOML is found without `owlry.lua` |

---

## 12. Implementation outline (engineering, not user-facing)

Tracked in `docs/RESTRUCTURE-V2.md` Phase 3. High level:

1. Add `mlua` dep (Lua 5.4, vendored, send, serialize)
2. New `crates/owlry/src/lua/` module: runtime, api surface, error types, host API utilities
3. `crates/owlry/src/config/loader.rs`: resolve order Lua → TOML → defaults; both paths produce the same `Config` struct
4. `LuaProvider` impl `Provider`/`DynamicProvider` from a Lua closure
5. `notify` watcher re-added; daemon listens for `owlry.lua` changes, re-evaluates in an isolated state, hot-swaps
6. `owlry migrate-config` functional; reads TOML, emits Lua
7. Characterization tests for every section of this doc; integration test that loads each example end-to-end
8. README + CLAUDE.md + `owlry(1)` updated

Target release: **2.1.0**.
