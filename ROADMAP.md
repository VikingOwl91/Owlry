# Owlry Roadmap

This is the active roadmap. It separates work that is committed from work that
is proposed or exploratory. Uncommitted items do not have assigned releases or
dates.

- Release history: [CHANGELOG.md](CHANGELOG.md)
- Published releases: <https://somegit.dev/Owlibou/owlry/releases>
- v2 rewrite story: [docs/RESTRUCTURE-V2.md](docs/RESTRUCTURE-V2.md)
- Shipped implementation notes: [docs/roadmap-archive.md](docs/roadmap-archive.md)

Labels: `[committed]` is planned for the current cycle, `[proposed]` is a
concrete candidate that needs a decision, and `[exploratory]` is an idea
without a design or schedule.

## Now

### 2.8.0 integration verification `[committed]`

The 2.8.0 release passed the unit, formatting, Clippy, and audit gates. These
live paths still need verification. They are unverified integration paths, not
known vulnerabilities.

- Cross-user IPC: a second UID must be rejected by both client and daemon.
- Real systemd socket activation, including multi-FD handoff and restart.
- Desktop and terminal handlers in a live Wayland session.
- Clipboard delivery, including byte-for-byte content.
- `locate` fallback when `fd` is absent.
- Mixed-version UI/daemon behavior and the restart requirement.

The detailed checklist is under Maintenance & Technical Debt.

### 2.8.0 documentation refresh `[committed]`

Refresh the README, split detailed material into `docs/`, keep the
configuration examples and CLI reference aligned with 2.8.0, and validate
internal links and cross-references.

## Next

### Window switcher with live thumbnails `[proposed]`

A `windows` provider using Wayland screencopy and compositor IPC to show live
thumbnails, potentially replacing Alt+Tab. Hyprland and Sway expose window
lists; a shared abstraction is the main design question.

This remains the first pick for a new workflow, carried over from the previous
roadmap. The priority is not changed here.

### `owlry doctor` installation and version checks `[proposed]`

Add report-only diagnostics for common upgrade problems:

- A local `owlry` earlier in `PATH` than the packaged `/usr/bin/owlry`.
- User systemd drop-ins that override the packaged `ExecStart`.
- A running daemon whose executable differs from the installed package.
- A running UI and daemon with different versions.

The checks should print the exact paths, unit files, and versions, and suggest
manual fixes. They must never modify `PATH`, units, symlinks, or services, and
must never restart the daemon. A small read-only IPC version query may be
needed for the UI/daemon comparison.

This is the smaller operational candidate. See the priority note at the end.

### Structured Query Engine `[proposed]`

Add a provider-agnostic query layer so filters can appear anywhere in the
input instead of only as a leading prefix:

- Position-independent filters such as `is:failed` and `type:service`.
- Free-text terms and filters may appear in any order.
- Negation, for example `-is:running`.
- Provider-extensible filter definitions.
- Explicit semantics for repeated filters, unknown filters, and quoted values.
- Initial implementation target: `uuctl`.
- Potential reuse for future providers, especially the proposed `windows`
  provider.

No release target is assigned, and this does not take priority above the
window switcher.

## Later

### Widget providers `[exploratory]`

Weather, MPRIS media controls, and pomodoro timer, deferred from 2.0 while the
widget-row placement model is designed. See [D20](docs/RESTRUCTURE-V2.md).

### Bookmarks `[exploratory]`

Firefox and Chromium bookmarks, deferred from 2.0. The v2 plan avoided a
SQLite dependency; Chromium's file is JSON and Firefox exposes JSON backups.
The provider design is still open. See [D22](docs/RESTRUCTURE-V2.md).

### Dynamic Lua providers `[exploratory]`

`owlry.provider { dynamic = true }` for per-keystroke items. Current releases
reject `dynamic = true` and support only static providers. No release
assigned.

### Runtime keybindings and HTTP helper `[exploratory]`

`owlry.bind` for runtime keybindings and `owlry.util.http_get` for user
providers. Neither has a design or release assigned.

### Natural-language commands `[exploratory]`

Map simple phrases to system actions, such as "shutdown in 30 minutes" or
"volume 50%". Local pattern matching, no cloud dependency. The grammar, safety
rules, and confirmation model need design work.

### Cross-device bookmark sync `[exploratory]`

Show bookmarks recently added on other devices. This depends on the bookmarks
provider and on parsing each browser's sync metadata.

### Remove individual clipboard entries `[exploratory]`

Add a contextual action to delete one cliphist entry without clearing the
whole history.

### Daemon architecture evaluation `[exploratory]`

After Lua configuration and honest startup profiling, compare the warm daemon
against an mmap'd cache and a single-process model. Removing the daemon would
delete the IPC protocol and much of `client.rs` and `server.rs`. This is an
evaluation, not a committed rewrite. See [D17](docs/RESTRUCTURE-V2.md).

## Maintenance & Technical Debt

### Owlry 2.8.0 integration verification `[committed]`

Run these in a disposable environment and record the result:

- [ ] Two-UID IPC test: client and server reject the other UID.
- [ ] systemd socket activation with the packaged units, including restart.
- [ ] Multi-FD activation with a custom unit.
- [ ] Live desktop handler and terminal handler launch.
- [ ] Clipboard text and image delivery with exact bytes.
- [ ] `locate` fallback with `fd` absent.
- [ ] New UI against an old daemon and old UI against a new daemon.

Accepted risks from the 2.8 review are not defects: heuristic desktop
shell-wrapper detection, fallback launchers re-resolving a refused handler,
non-atomic frecency writes, and environment-dependent pipe capacity. Keep them
documented; add tests only when a concrete failure is observed.

### Split the largest modules `[maintenance]`

The remaining large files are `providers/mod.rs` (about 2,050 lines),
`server.rs` (about 1,180), `ui/main_window/launch.rs` (about 1,130),
`lua/migrate.rs` (about 1,180), and `config/mod.rs` (about 1,040).
`ui/main_window.rs` is now under 400 lines and
`providers/converter/units.rs` is under 1,000. Extract focused units; do not
do a blanket rewrite.

### Double-daemon launch audit `[maintenance]`

The 2.8 per-socket lock prevents a second daemon from binding the same socket.
Audit external compositor autostarts and user units for `owlryd` or direct
`owlry -d` entries. Prefer the packaged `owlry.service`; keep the lock as the
last line of defense.

### `expr-solver-lib` evaluation `[maintenance]`

The calculator uses `expr-solver-lib`. Evaluate `evalexpr` only if the current
crate stagnates or becomes unsupported. No change is planned now.

### Frecency persistence `[maintenance]`

Writes are not atomic; a crash during save can truncate the file. This was
accepted in the 2.8 review. Consider an atomic replace only if data loss is
observed in practice.

### Dependency advisories `[maintenance]`

Three unmaintained zbus-family crates arrive through `notify-rust`. They are
documented exceptions, not known vulnerabilities. Remove the exceptions when
`notify-rust` moves to zbus 6 or the notification layer is replaced. See
[docs/dependency-audit.md](docs/dependency-audit.md).

## Shipped & History

Detailed implementation notes are in
[docs/roadmap-archive.md](docs/roadmap-archive.md). The release history is in
[CHANGELOG.md](CHANGELOG.md).

- **2.8.0**: security hardening, typed activation across IPC and launch paths,
  IPC peer authentication, socket lifecycle, frecency permissions, desktop
  `Exec` parsing, `fd`/`locate` terminators, systemd unit validation, and
  bounded currency fetches. See the
  [2.8.0 release](https://somegit.dev/Owlibou/owlry/releases/tag/owlry-v2.8.0).
- **2.7.0**: provider freshness, tab order from configuration, and
  `show_all_tab`.
- **2.6.0**: rofimoji integration.
- **2.5.0**: typed contextual actions, the submenu protocol redesign, and
  result action capture.
- **2.4.0**: frecency pruning thresholds, `:recent`, clipboard image
  thumbnails, and doctor provider labels and orphan reporting.
- **2.1.0**: Lua configuration, `owlry.provider`, hot reload,
  `owlry migrate-config`, and `owlry config validate`.
- **2.0.0**: package collapse and the single-binary model. See
  [docs/RESTRUCTURE-V2.md](docs/RESTRUCTURE-V2.md).

## Priority

The first pick for a new workflow remains the **window switcher with live
thumbnails**, carried over from the previous roadmap. The `owlry doctor`
installation and version checks are the smaller operational candidate. The
Structured Query Engine is a proposed feature and does not take priority above
the window switcher. Changing this order requires maintainer approval; this
document does not change it silently.
