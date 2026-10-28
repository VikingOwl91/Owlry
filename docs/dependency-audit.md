# Dependency audit notes

Last reviewed: 2026-10-28.

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
owlry 2.8.0
└── notify-rust 4.18.0
    └── zbus 5.19.0
        ├── zbus_names 4.3.4
        ├── zvariant 5.14.0
        └── zvariant_derive 5.14.0
```

The three zbus-family crates were merged into `zbus` 6.0.0. `notify-rust`
4.18.0 still depends on zbus 5.x, and there is no compatible upstream release
that removes these crates. Replacing the notification layer is the only
alternative and is out of scope for the security branch.

## Removal conditions

Remove the three advisory IDs from `.cargo/audit.toml` as soon as
`notify-rust` uses zbus 6 or the notification layer no longer depends on the
zbus 5.x crates. Run `cargo audit --deny warnings` after each dependency
update. Do not add crate-wide ignores.

Tokio must remain at 1.53.3 or newer. The advisory is patched in
`>= 1.53.3`, so a future downgrade below that version must fail the audit.
