# Dependency audit notes

Last reviewed: 2026-11-20.

`just audit` runs `cargo audit --deny warnings`. Exceptions live in
`.cargo/audit.toml` and are limited to exact advisory IDs.

## Current state

| Advisory | Crate | Kind | Status |
| --- | --- | --- | --- |
| RUSTSEC-2026-0386 | tokio 1.53.1 | unsound | Fixed by locking tokio 1.53.3. |
| RUSTSEC-2026-0370 | zvariant 5.14.0 | unmaintained | Exception until notify-rust uses zbus 6. |
| RUSTSEC-2026-0371 | zbus_names 4.3.4 | unmaintained | Exception until notify-rust uses zbus 6. |
| RUSTSEC-2026-0372 | zvariant_derive 5.14.0 | unmaintained | Exception until notify-rust uses zbus 6. |

## Upstream chain

```text
owlry 2.8.2
├── notify-rust 4.18.0 -> zbus 5.19.0
└── zbus 5.19.0 (direct: zbus = "5", for StartTransientUnit scope placement)
    ├── zbus_names 4.3.4
    ├── zvariant 5.14.0
    └── zvariant_derive 5.14.0
```

owlry depends on the zbus 5.x family directly (`zbus = "5"` in
`crates/owlry/Cargo.toml`, used to move launched apps into their own transient
systemd user scope) in addition to the transitive path via `notify-rust`. The
locked versions and the advisory exceptions above are unchanged.

The three zbus-family crates were merged into `zbus` 6.0.0. `notify-rust`
4.18.0 still depends on zbus 5.x, and there is no compatible upstream release
that removes these crates. Replacing the notification layer is the only
alternative and is out of scope for the security branch.

## Removal conditions

Remove the three advisory IDs from `.cargo/audit.toml` as soon as neither
`notify-rust` nor owlry's direct zbus dependency pulls in the zbus 5.x crates
(notify-rust moving to zbus 6, or the notification layer being replaced).
Run `cargo audit --deny warnings` after each dependency update. Do not add
crate-wide ignores.

Tokio must remain at 1.53.3 or newer. The advisory is patched in
`>= 1.53.3`, so a future downgrade below that version must fail the audit.
