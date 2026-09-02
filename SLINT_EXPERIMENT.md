# Slint UI feasibility experiment

Isolated experiment on branch `experiment/slint-ui` (worktree `../owlry-slint`,
base commit `385c28647cdea68fc05d49ff84d7152f46f7b0ce`). The original checkout on
`main` is never touched. Goal: answer whether Slint can realistically replace
Owlry's GTK4 UI layer using a real vertical slice, not a demo.

Status: **in progress**.

## Current architecture

Single binary `owlry`. Two process roles share it:

- **Daemon** (`owlry -d`): headless. Owns the ProviderManager (all providers),
  frecency store, config hot-reload (Lua watcher), application-directory
  watcher. Answers newline-delimited JSON over a Unix socket
  (`$XDG_RUNTIME_DIR/owlry/owlry.sock`, honouring `$OWLRY_SOCKET`).
- **UI client** (`owlry`, default subcommand): short-lived GTK4 process started
  per invocation (keybind). Acquires a UI lock file; if another instance runs,
  it sends `Request::Toggle` to the daemon and exits. Otherwise it connects to
  the daemon over IPC, builds a window, and exits when the window closes.
  Every show/hide cycle is a fresh process.

### Process boundary for the UI client (startup path)

`main.rs` → `try_acquire_lock()` → toggle-or-launch →
`app::OwlryApp::new(args)` → GTK `Application::activate`:

1. register bundled gresource icons
2. `Config::load_or_default()` (TOML; Lua layer is daemon-side)
3. dmenu detection (`DmenuProvider::has_stdin_data`) → local backend; else
   `CoreClient::connect_or_start()` → `SearchBackend::Daemon`, fallback local
4. `ProviderFilter::new(...)` from CLI `--mode`/`--profile` + config `general.tabs`
   (profiles can drop the implicit All tab: `show_all_tab` + `hide_all_tab()` +
   `set_single_mode(first)`)
5. `MainWindow::new(app, config, backend, filter, prompt)`
6. gtk4-layer-shell setup: `Layer::Overlay`, `KeyboardMode::Exclusive`,
   anchored top, margin 200
7. CSS: base.css + theme (owl / custom file) + user style + config-generated
   CSS variables (`theme::generate_variables_css`)
8. `present()`, then `schedule_initial_results()` (idle callback → first query)

### Code classification

1. **Backend/domain logic (fully reusable, zero GTK):**
   `providers/*`, `filter.rs`, `backend.rs`, `client.rs`, `ipc.rs`,
   `server.rs`, `config/*`, `data/`, `lua/*`, `paths.rs`, `notify.rs`,
   `application_watcher.rs`, `commands.rs`, `cli.rs`. `backend.rs` exposes
   `search`, `query_async` (futures_channel oneshot), `execute_plugin_action`,
   `query_submenu_actions`, `record_launch`, `available_providers`.

2. **UI-framework-neutral logic currently living in `ui/`:**
   - `main_window/launch.rs` (714 lines): activation/launching. Uses only
     `std::process::Command` + config + backend (gio launch / uwsm / sh -c,
     xdg-mime queries, open-with picker, shell quoting). GTK-free.
   - `main_window/navigation.rs` keyboard *semantics*: Esc/BackSpace-on-empty →
     close (exit 1 in dmenu), Up/Down selection, Right → contextual actions,
     Tab/Shift+Tab cycle, Ctrl+1-9 tab toggle, Shift+Enter → open-with picker.
   - `main_window/signals.rs`: 50 ms debounced search, prefix parsing
     (`ProviderFilter::parse_query`), staleness guard (dispatch-time raw text
     vs current text), direct-command fast path (`CommandProvider::is_direct_command`).
   - `main_window/results.rs`: lazy paging (initial 15, +10 on scroll/near-end
     selection), `LazyLoadState`.
   - `ui/submenu.rs`: submenu decode (typed + legacy `SUBMENU:` strings).
   - `ui/actions.rs`: builds contextual action list per item.
   - `ui/provider_meta.rs`: per-provider icon names/labels (strings).

3. **UI-framework-specific:**
   `app.rs` (bootstrap, layer-shell, CSS), `main_window.rs` (widget tree,
   filter toggle buttons, placeholder/hints composition), `result_row.rs`
   (GTK row widget + icon loading), `main_window/signals.rs` widget glue,
   `theme.rs` output (GTK CSS variables).

4. **Hidden coupling / migration friction:**
   - **glib main loop** pervades the UI: `glib::timeout_add_local`,
     `spawn_future_local` + futures_channel oneshot, `idle_add_local_once`.
     The async-query pattern is portable but must be re-implemented per
     framework (Slint: `slint::Timer` + `slint::Weak::upgrade_in_event_loop`).
   - **gtk4-layer-shell**: overlay layer + exclusive keyboard interactivity.
     Slint/winit has no layer-shell support; this is the highest-risk item.
   - **GTK CSS theming**: themes are CSS files + config-generated CSS
     variables (`--owlry-*`). Slint needs a token mapping, not CSS.
   - **Icon loading**: GTK IconTheme / freedesktop icon lookup inside
     `ResultRow`. Slint needs explicit icon-file resolution (no native
     freedesktop icon theme integration).
   - **gresource**: bundled icons registered at startup (GTK-specific).
   - **window lifecycle semantics**: close = process exit; per-launch process;
     daemon toggle path assumes this. A Slint frontend must keep this shape or
     the daemon's Toggle/toggle handling needs rework (out of scope).
   - `Rc<RefCell<...>>` shared state (config/backend/filter/results) — works
     only on the glib main thread; portable pattern but re-done per framework.

### Config/theme surface (compatibility requirements)

- Config: `config.toml` (+ Lua layer `owlry.lua` daemon-side). UI-relevant
  keys: `general.max_results` (100), `general.tabs` (app, cmd, uuctl),
  `general.show_icons`, `general.terminal_command`, `general.use_uwsm`,
  `appearance.{width,height,font_size,border_radius,theme,colors}`,
  `providers.*` flags, `profiles.<name>.{modes,show_all_tab}`.
- Theme: `appearance.theme = "owl"` (built-in CSS) or custom
  `$XDG_CONFIG_HOME/owlry/themes/<name>.css` + optional user style file +
  per-key color overrides → `--owlry-*` CSS variables (theme.rs:99 tokens:
  bg, bg-secondary, border, text, text-secondary, accent, accent-bright,
  badge_<provider>, font-size, border-radius).
- Reload: config hot-reload is **daemon-side** (notify watcher, debounced
  200 ms). The UI client is short-lived and re-reads config at every launch,
  so UI "reload" is implicit. No live theme reload inside a window session.
- Backwards compat: serde aliases (`badge_sys`→`badge_power`,
  `system`→`power`), legacy `SUBMENU:` decode, old CSS var names emitted.

## Phase 2 — baseline (GTK frontend)

Build config: `cargo build --release --features full` (AUR-equivalent),
toolchain pinned `rust-toolchain.toml` 1.97.1.

### Methodology

- Live Hyprland Wayland session, prod daemon untouched; all UI runs against
  a dev daemon on `OWLRY_SOCKET=/tmp/owlry-dev.sock`, UI processes killed
  with SIGTERM after sampling.
- Build time: wall clock of `cargo build --release --features full` on a
  warm target dir.
- Startup: built-in `RUST_LOG=debug` BOOT instrumentation (per-step timings
  from `app.rs`); "TTFW" = log-to-log from first log line to
  `window mapped (first paint)`. This excludes exec/dynamic-link time before
  the first log line, so true process-start→first-paint is slightly higher.
- Idle CPU: `/proc/<pid>/stat` utime+stime delta over a sampling window
  (100 ticks/s), idle, no interaction.
- Idle memory: `/proc/<pid>/status` VmRSS; PSS via `smem`.
- Query latency: python probe over the Unix socket, newline-JSON
  `{"type":"query",...}` round trip (connect + send + full response),
  measured daemon-side and therefore frontend-independent. Raw logs in
  `/tmp/opencode/owlry-baseline-build.log`, `owlry-ui-ext2.log`,
  `ipc_lat.py` output.

### Raw values (release, `--features full`)

| Metric | Value |
|---|---|
| Build time (warm) | 1m 16s |
| Binary size | 6,045,912 B (5.77 MiB) |
| TTFW (log→log, 5 runs) | ≈350 ms; `activate` is reached +334 ms after boot (GTK/display/D-Bus init dominates), then window mapped +15.3–16.6 ms after activate (very consistent) |
| Idle RSS (5 runs) | 110.0–110.8 MB |
| Idle PSS (5 runs) | 52.0–54.3 MB |
| Idle CPU | 240–260 ms per 5 s ≈ 5% of one core; steady 10 s sample: 2.4% |
| Query latency, empty (n=10) | min 0.68 / p50 0.81 / max 1.26 ms |
| Query latency, "fir" (n=20) | min 3.77 / p50 3.99 / p95 4.22 / max 4.29 ms |
| Query latency, "firefox" (n=20) | min 3.82 / p50 4.16 / p95 4.36 / max 4.47 ms |
| Initial daemon query (100 items) | 2.0 ms |

One flawed external TTFW attempt (sleep-polluted deltas) was discarded in
favour of the log-to-log method. The current UI was not optimized; these
numbers are the comparison baseline only.

## Prototype

### Scope

A vertical slice under `crates/owlry/src/ui_slint/`, enabled by the
experimental cargo feature `slint-ui` (NOT part of `full`). Runtime
selector: `OWLRY_UI=slint` routes the UI path to the Slint frontend; a
binary built without the feature silently runs GTK. Both frontends coexist
in one branch and (with the feature) one binary.

- `launcher.slint` — window (no-frame, transparent), header with mode label
  + tab pills, search `LineEdit`, `ListView` (54 px rows, title/description/
  badge letter/badge color), hints bar, `FocusScope` handling
  Down/Up/Tab/Shift+Tab/Right/Esc/Enter/Shift+Enter/Ctrl+1-9 in the
  **capture** phase (the focused LineEdit would otherwise consume
  Up/Down/Right before they bubble).
- `mod.rs` — `run(args) -> !`: real config load, dmenu stdin detection →
  local backend, else `CoreClient::connect_or_start` → daemon backend,
  else local fallback (mirrors `app.rs`); real `ProviderFilter` resolution
  incl. profiles (`show_all_tab`/single-mode); 50 ms Slint `Timer` debounce;
  daemon queries via `backend.query_async` on a worker thread with
  `slint::invoke_from_event_loop` + staleness guard (same semantics as the
  GTK `spawn_future_local` path); activation via real backend
  (`execute_plugin_action` / submenu entry via `query_submenu_actions` /
  launch semantics).
- `launch.rs` — launch path replicated from the GTK semantics (dmenu
  println, frecency record, ScriptPlugin command allowlist, desktop apps
  via uwsm/`gio launch`, commands via `sh -c` + `terminal_command`,
  errors → notify-rust). **Documented divergence:** file-open uses plain
  `xdg-open`; the GTK frontend's terminal-aware default-handler resolution
  is private to `ui/main_window/launch.rs` and was not lifted.
- `theme.rs` — the only adapter between config and Slint: maps
  `AppearanceConfig` (`width/height/font_size/border_radius`, color
  overrides incl. per-provider `badge_*` keys, `sys` alias honoured) onto
  window properties. Unset keys keep the `.slint` markup defaults (owl
  palette). No second config format; the real `config.toml`/`owlry.lua`
  path is used, nothing new for users to learn or rewrite.

Reused framework-neutral code as-is: `ui::actions::for_item` (Right-arrow
action list), `ui::provider_meta::resolve` (tab labels), `SearchBackend`,
`ProviderFilter`, `Config`, `CliArgs`. Badges are colored initial letters
rather than real icons (icon loading deferred).

### Functional coverage vs GTK frontend

| Capability | Slint slice |
|---|---|
| Window, search input, result list, selection | yes |
| Keyboard nav (↑↓, Tab cycling, Ctrl+1-9, Esc, Enter, Shift+Enter semantics partially) | yes (Shift+Enter = open-with picker NOT implemented; GTK-private) |
| Right-arrow contextual actions | yes, via shared `actions::for_item` |
| Profiles / tabs / `show_all_tab` / prefix modes | yes, real config + `ProviderFilter` |
| Theme tokens from real config | yes, via `ui_slint/theme.rs` |
| Real backend path (daemon IPC, local fallback, dmenu) | yes |
| Activation: launch / submenu / plugin action | yes (real `LaunchItem::activation`) |
| Lazy loading (infinite scroll) | partial: initial page + selection-driven load-more not wired; slice renders up to `max_results` |
| dmenu / selector modes | dmenu stdout/exit-code path present; selector mode not wired |
| Open-with picker, drag-drop, icons, tray | not implemented |

### Live verification (dev daemon, Hyprland)

Initial window shows real daemon results with mode label "All", tab pills,
placeholder, user theme colors. Typing "fir" → 50 ms debounce → daemon
query → filtered results with selection on row 0. Down moves selection;
Right opens "Element — Actions" (Open / Open in Terminal / Show Desktop
File Location); Esc exits submenu then closes. External window close is a
graceful winit exit. Screenshots: `/tmp/opencode/slint-smoke-{1..11}*.png`.

### Measurements (release, `--features slint-ui`, same methodology)

| Metric | Slint slice | GTK baseline |
|---|---|---|
| TTFW (3 runs) | **78.7 / 80.7 / 81.9 ms** (process boot → window shown) | ≈350 ms log→log |
| Idle RSS | 215.7 MB | 110.0–110.8 MB |
| Idle PSS | 90.0 MB | 52.0–54.3 MB |
| Idle CPU (5 s) | 31 ticks ≈ **0.6%** of one core | 2.4–5% |
| Binary size (default features + slint-ui) | 20,193,048 B (19.26 MiB) | default (no slint): 5,390,712 B (5.14 MiB); `--features full` GTK: 5.77 MiB |
| Release build time | 2m 34s | 27s (default) / 1m 16s (full) |

Interpretation: ~4.4× faster to first paint (GTK's cost is display/D-Bus
init, not widget construction). Slint is much cheaper idle (no CSS
animation/poll timers), but roughly 2× RSS / 1.7× PSS (femtovg/EGL
renderer). The +14.1 MiB binary delta exists because gtk4 is still linked
in as a hard dependency of `app.rs`/`ui/`; a real migration would
feature-gate gtk4 and reclaim most of it. Query latency is daemon-side and
identical for both frontends.

## Decision report

### Layer-shell (the decisive hazard)

Owlry's GTK UI is a wlr-layer-shell overlay: `Layer::Overlay` +
`KeyboardMode::Exclusive`, anchored top. **Slint does not support
wlr-layer-shell** (winit#2582 open; PRs #2832/#4044 unmerged as of
2026-09). Known workarounds (layer-shika 0.3.1, VimYoung/Spell) are
early-dev, interpreter-based, and smuggle window adapters into the Slint
platform — no stable API. The slice uses a plain no-frame window: it will
not render above fullscreen apps and focus is compositor-managed (no
exclusive keyboard grab). For a launcher this is a real product-semantics
regression, not a cosmetic one.

### Config / theme compatibility

**Full.** The slice reads the real config path (`config.toml` /
`owlry.lua` daemon-side), honours profiles, tabs, `show_all_tab`, provider
flags, and maps every existing `--owlry-*` theme token incl. `badge_*`
and the `sys` alias. The adapter is one small file (`ui_slint/theme.rs`).
No user config or theme rewrite required. Config hot-reload is
daemon-side and unaffected; UI config is read per launch in both
frontends.

### Advantages

- Startup: ~80 ms vs ~350 ms to first paint, deterministic.
- Idle CPU: ~0.6% vs 2.4–5%; the GTK frontend burns measurable idle time.
- Single declarative UI file; keyboard handling in one capture handler
  instead of 522 lines of controller code.
- Retained-mode UI with an explicit model; less per-widget ceremony than
  GTK4-rs.

### Disadvantages

- No layer-shell: loses overlay layering + exclusive keyboard. Highest
  migration hazard.
- Memory: ~2× RSS / ~1.7× PSS (GPU renderer) for a launcher that is idle
  99% of its life.
- Binary +14 MiB while gtk4 remains linked; needs feature-gating work.
- Launch semantics and open-with picker are GTK-private; a migration must
  lift `ui/main_window/launch.rs` (714 lines) and the picker to a
  framework-neutral module first.
- Missing parity work: real icons, lazy loading wiring, Shift+Enter
  open-with, selector mode, dmenu edge cases.
- Ecosystem maturity: Slint + winit + layer-shell is a moving target.

### Migration hazards (ordered)

1. **Layer-shell** — blocked upstream (winit#2582); plain window is a
   behavior change users will notice.
2. Launch/picker logic entangled with GTK (`main_window/launch.rs` is
   private inherent methods) — must be lifted before any migration.
3. gtk4 feature-gating across `app.rs`/`ui/` (build impact).
4. Icon loading (GTK IconTheme/gresource → Slint image API).
5. dmenu/selector parity + lazy-load + Shift+Enter semantics.

### Follow-ups if pursued

- Lift launch/open-with logic into a framework-neutral module (benefits
  GTK frontend too).
- Prototype layer-shell via layer-shika OR wait on winit#2582.
- Wire lazy loading + selector mode; real icons.
- Feature-gate gtk4 to measure true Slint-only binary size.

### Recommendation: YES (conditional)

Slint is technically viable for Owlry's UI: the slice reproduces the core
launcher loop against the real backend, config, themes, and actions with
one small adapter, and wins clearly on startup latency and idle CPU.
BUT it is not adoptable today as a replacement: the missing
wlr-layer-shell support removes overlay + exclusive-keyboard behavior that
the launcher depends on, and memory is ~2×. Recommendation: keep the
experiment branch, watch winit#2582 / layer-shika maturity, and until then
treat Slint as the frontend for a possible future "fast launcher" variant
rather than a drop-in GTK replacement. If layer-shell lands upstream,
re-evaluate; the remaining migration work (launch-logic lift, icons,
parity) is bounded and the config/theme story is already clean.
