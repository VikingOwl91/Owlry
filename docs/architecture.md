# Architecture

Owlry is a single binary with several entry points:

```text
owlry                          GTK4 UI client, connects to the daemon
owlry -d / owlry daemon        IPC daemon, loads providers, listens on the socket
owlry dmenu                    stdin to selection, no daemon
owlry selector                 selector protocol for rofimoji and similar tools
owlry doctor / providers / config / refresh   diagnostics and config tools
```

## Daemon and providers

The daemon keeps providers and their cached items in memory. A query is a
fuzzy match over that data plus a Unix-socket round-trip, so the UI does not
rescan the filesystem or spawn a subprocess for every keystroke.

- Static providers cache items and refresh on startup, on `owlry refresh`, or
  when their watched inputs change.
- Dynamic providers run per query. Calculator, converter, and web search are
  cheap and run in auto mode. File search is opt-in because it spawns `fd` or
  `locate`.
- The application watcher refreshes entries when `.desktop` files change in
  the XDG application directories.
- The systemd provider caches unit discovery but queries current unit state
  when a unit submenu opens.
- Providers backed by an external program can rebuild when that program
  appears or disappears. See [Provider freshness](provider-freshness.md).

Frecency data is saved every five minutes and flushed on shutdown. The store
is created with mode 0600 and rejects symlinks, hardlinks, FIFOs, and files
owned by another user.

## IPC socket

The default socket is `$XDG_RUNTIME_DIR/owlry/owlry.sock`. The daemon creates
or tightens the `owlry` directory to 0700, binds the socket, and sets it to
0600.

`OWLRY_SOCKET` overrides the path. It must be absolute, and its parent must be
owned by the current user and not writable by group or others. A root-owned
sticky directory such as `/tmp` is accepted for an explicit override. There is
no implicit `/tmp` fallback: if `XDG_RUNTIME_DIR` is unset and no override is
given, the client and daemon report an error.

Both the client and the server check the peer's effective UID with
`SO_PEERCRED` and refuse a different user. The daemon also takes a per-socket
lock so two daemons cannot race the stale-socket check, and it removes only
the socket inode it created.

The UI and daemon must run the same version. The activation schema changed in
2.8.0; restart the daemon after upgrading. See [Upgrading](upgrading.md).

The IPC protocol is newline-delimited JSON over a Unix stream socket. Requests
and responses are size-bounded, and a read timeout prevents a stuck peer from
holding a connection open.

## Performance

The hot path is an in-memory frecency-ranked fuzzy match over cached provider
data plus a socket round-trip. The file search provider is the exception: it
runs only under `:file` or `/` and is bounded by a depth limit, directory
exclusions, a result cap, and a wall-clock timeout.

The following numbers were measured on one machine with a warm daemon. They
are informational, not a benchmark or a cross-machine guarantee.

| Path | Latency |
|---|---|
| Query round-trip, UI to daemon and back | about 2 to 7 ms |
| Result rendering, 15 rows | about 3 ms |
| Frecency-ranked fuzzy match over all providers | low single-digit ms |

## Configuration and reload

The daemon loads `owlry.lua` when the `lua` feature is compiled in, otherwise
TOML. It watches the Lua config and its modules and hot-reloads on save. A
broken edit produces a desktop notification and keeps the previous state.
