# Contributing to Owlry

Owlry is developed on [Gitea](https://somegit.dev/Owlibou/owlry). Issues and
pull requests are welcome there. For security problems, contact the maintainer
privately instead of opening a public issue. The maintainer address is in
`aur/owlry/PKGBUILD`.

## Development setup

The development toolchain is pinned in `rust-toolchain.toml` (Rust 1.97.1).
The crate MSRV is Rust 1.90, declared in `crates/owlry/Cargo.toml`.

System dependencies:

```bash
# Arch
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu / Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

Build and run:

```bash
cargo build --release --features full   # matches the AUR build
cargo run -p owlry                      # UI
cargo run -p owlry -- -d                # daemon
```

`just install-local` installs a development build into `/usr/bin` and the
systemd user units. Prefer the AUR package for normal use.

## Tests and checks

```bash
just fmt-check     # rustfmt
just check         # cargo check and strict Clippy on both feature axes
just test          # tests on both feature axes
just audit         # cargo audit --deny warnings
just verify        # all of the above
```

`just verify` is the gate used by CI. Run it before opening a pull request.
The test suite runs both `--all-features` and `--no-default-features`; changes
to feature-gated providers should keep both green.

Tests that touch environment variables, sockets, or temporary files should
use isolated temporary directories and restore any process state they change.

## Code and commit style

- Keep changes focused. Do not mix unrelated refactors with a fix.
- Follow the existing conventional commit style (`fix:`, `feat:`, `docs:`,
  `chore:`).
- Run `cargo fmt --all` and keep Clippy clean with `-D warnings`.
- Add regression tests for behavior changes, especially provider parsing,
  launch paths, socket handling, and configuration.
- Do not add comments or docstrings to code you did not change.
- Keep user-facing documentation in sync with behavior. The README is the
  landing page; detailed material lives under `docs/`.

## Documentation and media

The README is intentionally short. Put detailed usage, configuration,
theming, architecture, and upgrade notes in `docs/` and link them from the
README.

The media guide in [docs/media.md](docs/media.md) describes how to capture
screenshots and the demo GIF from a real Owlry session. Do not use generated
or synthetic UI images.

## Packaging

The AUR package lives in `aur/owlry/` and has its own git repository. The
maintainer workflow is:

```bash
just bump <version>       # version, Cargo.lock, man page; commits
just tag                  # annotated owlry-v<version> tag
just push-tags
just aur-update           # source URL, b2sum, .SRCINFO
just aur-commit "chore(aur): update owlry to <version>"
just aur-publish          # commit and push the AUR repository
```

`just aur-update` downloads the published tag tarball, so the tag must exist
on the remote first. Do not publish an AUR update before the matching forge
release and source archive are available.

## License

Contributions are accepted under the GNU General Public License v3.0 or later.
See [LICENSE](LICENSE).
