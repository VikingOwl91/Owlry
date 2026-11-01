# Upgrading Owlry

Package upgrades replace the binary and the shared systemd user units in
`/usr/lib/systemd/user/`. Your enablement, running units, and configuration
under `~/.config/owlry/` are untouched. The `.install` hook only prints
instructions; it does not call `systemctl`.

Restart or rebind the daemon after every upgrade. A running daemon keeps the
IPC activation schema it started with, so a new UI and an old daemon can
disagree about how commands are delivered.

## From 1.x to 2.x

Owlry 2.x merged the old package set into one binary and renamed the units:

- `owlryd` is now `owlry -d`.
- The user units are `owlry.service` and `owlry.socket`.
- Remove stale `owlryd` units from `~/.config/systemd/user/` if they shadow the
  package units in `/usr/lib/systemd/user/`.
- Replace remaining `owlryd` references in compositor autostart configs.

Then reload and start the packaged service as your regular user:

```bash
systemctl --user daemon-reload
systemctl --user enable --now owlry.service
```

`paru` or `pacman` swaps `owlry-core`, `owlry-lua`, `owlry-rune`, and the
retired `owlry-plugin-*` and `owlry-meta-*` packages automatically through the
PKGBUILD `replaces` and `conflicts` arrays.

## Within 2.x

Pick the section that matches how the daemon is started. Do not enable a unit
that is already enabled; upgrade the package and follow the recovery for the
mode you use.

### Socket activation (`owlry.socket`)

With socket activation, systemd owns the listening socket and passes it to the
daemon. Owlry 2.7 and earlier did not adopt that listener: at startup the
daemon replaced the socket file at the path with its own, and at shutdown it
removed that file again. The socket unit stayed active while its original
listener pointed at an unlinked inode. After upgrading to a daemon that does
adopt the listener, two symptoms follow:

- Clients get `ENOENT` when they connect, so activation never fires.
- `systemctl --user restart owlry.service` fails, because the daemon inherits
  the socket unit's listener and refuses to continue when the path it is bound
  to is missing.

The socket unit's path comes back only when that unit stops and starts again.
Stop both units first, then start only the socket:

```bash
systemctl --user stop owlry.service owlry.socket && systemctl --user start owlry.socket && owlry
```

Both units must stop successfully before the socket starts again. If either
stop fails, fix that first: starting an already-active socket unit is a no-op
and leaves the old listener in place. If the service reaches its start limit, see the troubleshooting steps below.

Then launch Owlry normally. The client connects to the fresh path and systemd
starts `owlry.service`. You do not need to enable anything again, and the
package hook does not enable or restart units for you.

Owlry 2.8.0 and later no longer unlink a socket inherited from systemd, so this
recovery is only needed for a path that was already lost before the upgrade.
If both units were stopped at upgrade time, start the socket directly:

```bash
systemctl --user start owlry.socket
```

### Service without socket activation

If `owlry.service` is running, restart it:

```bash
systemctl --user restart owlry.service   # only when it is running
```

If it is not running, start it as a separate step:

```bash
systemctl --user start owlry.service
```

A restart does not enable the unit. Enablement is separate and persists across
upgrades.

### Manually started daemon

If you start the daemon yourself (`owlry -d`) or from compositor autostart,
stop the old process and start it again. Restarting the UI is the simplest way
to make sure it connects to the new daemon. No systemd unit is involved in this
mode.

## Verify the result

```bash
systemctl --user status owlry.socket owlry.service
ls -l "${XDG_RUNTIME_DIR}/owlry/owlry.sock"
owlry doctor
```

`owlry doctor` reports config, socket, and provider state.

## If recovery still fails

For socket activation, check `systemctl --user status owlry.socket owlry.service`.
If a unit has hit its start limit, run:

```bash
systemctl --user reset-failed owlry.service owlry.socket
```

Then repeat the stop-and-start sequence above.
If unit files changed, run `systemctl --user daemon-reload` before retrying.

For service-only or manually started daemons, inspect the service status or
process output for that mode. Do not start the socket unit unless you intend
to switch to socket activation. Check for user unit overrides if the packaged
units are not being used.

## Later upgrades

The install hook prints the same guidance for every 2.x upgrade. It runs as
root and stays informational, so it does not query any user manager and does
not try to detect the mode or distinguish between 2.x versions.

For Owlry 2.8.0 and later:

- Service mode: restart the service when it is running, otherwise start it.
- Socket mode: restarting the service is safe while the socket path exists.
  If the path is ever missing, use the stop-both, start-socket sequence above.
- A service-only restart never recreates a lost socket path. Only restarting
  the socket unit does.
