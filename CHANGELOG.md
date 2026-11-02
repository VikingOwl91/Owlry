# Changelog

User-facing changes for Owlry. Older releases are documented at
<https://somegit.dev/Owlibou/owlry/releases>.

## 2.8.1 - 2026-11-02

### Fixed

- The Arch package automatically migrates active systemd sockets at the packaged
  default path when upgrading from 2.7.0. It stops the old daemon and socket
  together before restoring the listener, so the legacy daemon cannot unlink
  the replacement socket. Normal upgrades in this configuration need no manual
  recovery.
- Interrupted migrations retain pending state for a retry. Healthy sockets,
  deliberately inactive units and custom socket paths are left alone.
- Report a missing inherited socket with recovery steps, and document upgrade
  handling for service-only and manually started daemons.

### Upgrade notes

- Service-only users must restart `owlry.service` after upgrading. Manually
  started daemons must also be restarted; custom socket paths need manual
  upgrade handling. See [Upgrading](docs/upgrading.md).
- Check pacman output for migration failures. A failed migration does not roll
  back the package transaction; the upgrade guide explains how to retry.

## 2.8.0 - 2026-10-28

Security hardening release. The launcher no longer builds shell command
strings from provider results, desktop entries, or implicit direct commands.

### Security

- IPC clients and the daemon verify each other's effective UID with
  `SO_PEERCRED`. The socket must live in a user-owned, private directory.
- Provider results use typed argv, stdin, and pipe activations. Legacy shell
  payloads are refused, and script-provider items can no longer reach `sh -c`
  through the old prefix allowlist.
- Desktop `Exec` lines are parsed into argv with field-code expansion. Handlers
  that explicitly invoke a shell are refused.
- `fd` and `locate` searches pass `--` before the user pattern.
- Frecency files are created and tightened to 0600 and reject symlinks,
  hardlinks, FIFOs, and foreign owners.
- systemd submenu actions validate unit names and carry argv.
- Currency fetches have connect and total timeouts, a 1 MiB response cap, and
  per-line parse resilience.

### Changed

- Shell syntax in the search field requires the explicit `:shell` prefix.
  `:cmd` remains a PATH-command filter.
- Lua providers should use `program`, `args`, and `stdin`. `command` strings
  are still parsed, but unquoted shell metacharacters are rejected.
- The IPC activation schema changed. Restart the daemon after upgrading.

### Fixed

- The daemon backs off on transient accept errors instead of spinning.
- Stdin activation no longer risks blocking the UI thread.
- Lock files are validated and tightened before use.

### Dependencies

- tokio updated to 1.53.3 for RUSTSEC-2026-0386.
- Three unmaintained zbus-family advisories are documented exceptions until
  notify-rust moves to zbus 6.
