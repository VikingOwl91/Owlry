# Roadmap archive

Implementation notes moved out of [ROADMAP.md](../ROADMAP.md). This file
records what shipped and why. It is not a list of active commitments. Release
history is in [CHANGELOG.md](../CHANGELOG.md).

## 2.8.0: security hardening

The 2.8.0 release replaced shell command strings in the launch path with typed
activations, tightened the local IPC boundary, and hardened the files written
under the user's data directory.

- Provider results carry `Argv`, `Stdin`, or `Pipe` activations. Legacy shell
  payloads are refused. Script-provider items can no longer reach `sh -c`
  through the old prefix allowlist.
- Lua `command` strings are parsed into argv. Unquoted shell metacharacters
  are rejected; `program`, `args`, and `stdin` are preferred.
- Desktop `Exec` lines are parsed into argv with field-code expansion.
  Handlers that explicitly invoke a shell are refused.
- IPC clients and the daemon check the peer's effective UID with
  `SO_PEERCRED`. The socket must live in a user-owned, private directory.
- The silent `/tmp` socket fallback was removed. The runtime directory is
  0700, the socket is 0600, stale sockets are replaced only when owned by the
  current user and provably dead, and shutdown removes only the created inode.
- Frecency files are created and tightened to 0600 and reject symlinks,
  hardlinks, FIFOs, and foreign owners.
- `fd` and `locate` receive `--` before the user pattern.
- systemd unit names are validated, and unit actions carry argv.
- Currency fetches have connect and total timeouts, a 1 MiB response cap, and
  per-line parse resilience.

Live cross-user, systemd activation, desktop handler, clipboard byte, and
`locate` checks remain in the active roadmap.

## 2.7.0: provider freshness and profiles

- Application and systemd provider state stays fresh while the launcher runs.
  The application watcher covers absent XDG data directories and `.desktop`
  changes.
- Providers backed by an external program rebuild when that program appears or
  disappears. This covers clipboard (`cliphist`) and rofimoji.
- Tab order follows the configured provider list instead of forcing core
  providers to the front.
- Profiles can set `show_all_tab = false` so Tab cycling stays inside the
  profile's tabs.

## 2.6.0: rofimoji

The `rofimoji` integration adds an emoji, kaomoji, and Unicode character
picker. It is an optional cargo feature and an AUR opt-in dependency.

## 2.5.0: typed actions and submenus

- Results gained a typed activation for launches, submenus, and provider
  actions. The 2.8.0 release added `Argv`, `Stdin`, and `Pipe`. The old
  `SUBMENU:<plugin_id>:<data>` encoding is decoded only at compatibility
  boundaries.
- Right Arrow opens contextual actions. Plain Enter keeps the primary action.
- The action matrix:

  | Provider | Actions |
  |---|---|
  | Any launchable result | Primary Open, Run, Copy, or Connect |
  | Applications | Open in terminal, show `.desktop` location |
  | Files | Open containing folder, copy path, move to trash |
  | Commands | Run in terminal |
  | SSH | Copy host alias, edit SSH config |

- Calculator and converter results expose Copy as their primary action. File
  paths and SSH host aliases expose explicit Copy actions.
- Bookmark actions remain pending until bookmarks return. Removing individual
  clipboard-history entries is still future work.

## 2.4.0: frecency, recent items, clipboard images, doctor

- Frecency startup pruning is configurable through `[providers]`:
  `frecency_max_entries` (0 = unbounded), `frecency_max_age_days` (0 = no age
  pruning), and `frecency_min_launches`. Defaults are 5000, 180, and 3. The
  daemon applies them at store load; a restart re-prunes after a config change.
- `:recent` lists recently launched items, newest first. The daemon resolves
  stored item ids against its in-memory static providers and drops dangling
  entries. It is routed through the unknown-prefix fallback, with no IPC
  change.
- Clipboard image entries are detected from cliphist's binary preview, named
  from the image type and dimensions, and decoded to a cached thumbnail under
  `$XDG_CACHE_HOME/owlry/clip-thumbs/`. The paste path passes the entry id as
  an argument to `cliphist decode`; the old piped form included a newline and
  was rejected. Each refresh prunes thumbnails for entries no longer present.
- `owlry doctor` labels providers as `[static]` or `[dynamic]` and reports
  stale system-level remnants under `[stale system files]`: the pre-v2
  `/usr/lib/owlry` runtimes directory, old `owlryd.{service,socket}` units, and
  an uncompressed `owlry.1` man page only when the packaged `.gz` also exists.
  The report is read-only; doctor never deletes anything.

## 2.1.0: Lua configuration

`~/.config/owlry/owlry.lua` became the canonical configuration. TOML remains
supported through 2.x as a fallback. The Lua surface includes:

- `owlry.set`, `owlry.providers`, `owlry.tabs`, `owlry.theme`, and
  `owlry.profiles`.
- `owlry.provider {}` for user-defined static providers.
- `owlry.util.{shell, shell_lines, read_file, glob, env, hostname}`.
- Hot reload on save. Broken edits surface as desktop notifications and keep
  the previous state.
- `owlry migrate-config [--force]` and `owlry config validate`.

Current examples should prefer `program` + `args` over `command` strings. See
[docs/lua-api.md](lua-api.md).

## 2.0.0: package collapse

The 2.0 release replaced the pre-v2 package set with one `owlry` package. The
UI, daemon, and providers moved into one binary. `owlryd` became `owlry -d`
(`owlry daemon`), and the units became `owlry.service` and `owlry.socket`.

- Widget providers (weather, MPRIS media, pomodoro) were deferred because the
  widget-row placement model was unresolved. See D20 in
  [docs/RESTRUCTURE-V2.md](RESTRUCTURE-V2.md).
- Bookmarks were deferred to avoid a hard `rusqlite` / `libsqlite3-sys`
  dependency in the AUR chroot build. Chromium's bookmarks file is JSON and
  Firefox exposes JSON backups, so a pure-Rust path is possible. See D22 in
  [docs/RESTRUCTURE-V2.md](RESTRUCTURE-V2.md).
- Before 2.0, the systemd unit and a compositor `exec-once = owlryd` could both
  spawn a daemon. The 2.8 per-socket lock now prevents a second daemon from
  binding the same socket. Audit external autostart entries for `owlryd` or
  direct `owlry -d` lines; prefer the packaged service.
