# Upgrading Owlry

## Upgrading to 2.8.0

2.8.0 changes the IPC activation schema. The UI and daemon must run the same
version, so restart the daemon after the package upgrade:

```bash
systemctl --user restart owlry.service
```

The package prints a reminder in its post-upgrade message, but it does not
restart the daemon for you. If you run the daemon manually, stop the old
`owlry -d` process and start it again. If you use the socket unit, restarting
the service is enough.

If the UI reports that a legacy shell payload was refused, the running daemon
is still the old version. Restart it.

### Behavior changes in 2.8.0

- Shell syntax in the search field requires the explicit `:shell` prefix.
  `:cmd` remains a PATH-command filter. Typing `program args` launches argv
  without a shell.
- Lua provider items should use `program`, `args`, and `stdin`. `command`
  strings still work for simple cases, but they are parsed into argv, and
  items with unquoted shell metacharacters are dropped with a warning.
- Desktop handlers that explicitly invoke a shell are refused on the
  terminal-handler and open-with paths. Owlry falls back to `xdg-open` or
  `gio launch`.
- `OWLRY_SOCKET` must be absolute, and its parent must be private to the
  current user or a root-owned sticky directory.
- The frecency file is created and tightened to 0600.

## Upgrading from 1.x

The 2.0 release collapsed the old package set into one `owlry` package. The
AUR package uses `replaces` and `conflicts` so `paru` or `pacman` swaps the
old packages automatically. The retired names include `owlry-core`,
`owlry-lua`, `owlry-rune`, every `owlry-plugin-*` package, and the
`owlry-meta-*` bundles.

The daemon binary and units were renamed:

- `owlryd` is now `owlry -d` (or `owlry daemon`).
- `owlryd.service` and `owlryd.socket` are now `owlry.service` and
  `owlry.socket`.

After upgrading, remove stale user units that shadow the package units:

```bash
ls ~/.config/systemd/user/owlryd.* 2>/dev/null
systemctl --user daemon-reload
systemctl --user enable --now owlry.service
```

If a compositor autostart entry still runs `owlryd`, change it to `owlry -d`.

## TOML to Lua

The Lua configuration is preferred. To migrate an existing `config.toml`:

```bash
owlry migrate-config              # writes owlry.lua; refuses to overwrite
owlry migrate-config --force      # overwrite an existing owlry.lua
```

The migrator is deterministic and emits only values that differ from
defaults. It normalises pre-v2 aliases such as `system` to `power` and
`badge_sys` to `badge_power`.

TOML remains supported through 2.x. When `owlry.lua` exists, `config.toml` is
ignored entirely.

## After upgrading

```bash
owlry --version    # owlry 2.8.0
owlry providers
owlry doctor
```

`owlry doctor` reports config status, the socket, provider availability, and
stale pre-v2 files. It is read-only.
