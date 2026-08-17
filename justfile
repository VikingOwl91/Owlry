# Owlry build and release automation.
#
# v2 collapsed the workspace from 5 crates to 1 and shipped a single AUR
# package. The multi-crate / multi-package machinery has been removed.

default:
    @just --list

# === Build ===

build:
    cargo build --workspace

# Release build with the AUR feature set (every optional provider compiled in).
release:
    cargo build --workspace --release --features full

# Release build with only the minimal default features.
release-minimal:
    cargo build --workspace --release

# === Run ===

# Launch the UI (auto mode, default features).
run *ARGS:
    cargo run -p owlry -- {{ARGS}}

# Run the daemon in the foreground (alias: `cargo run -- -d`).
run-daemon *ARGS:
    cargo run -p owlry -- -d {{ARGS}}

# Run with dev-logging feature enabled (verbose debug output).
run-debug *ARGS:
    cargo run -p owlry --features dev-logging -- {{ARGS}}

# === Quality ===

# Run the full test matrix used in CI.
test:
    cargo test --locked --workspace --no-default-features
    cargo test --locked --workspace --all-features

# cargo check + strict clippy across both feature axes.
check:
    cargo check --locked --workspace --no-default-features
    cargo check --locked --workspace --all-features
    cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

# Fail on vulnerabilities and informational security warnings such as unsound crates.
audit:
    cargo audit --deny warnings

# Complete local/CI quality gate.
verify: fmt-check check test audit

clean:
    cargo clean

# === Install (local dev) ===

install-local:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Building release with --features full..."
    cargo build -p owlry --release --features full

    echo "Installing binary..."
    sudo install -Dm755 target/release/owlry /usr/bin/owlry

    echo "Installing systemd user units..."
    sudo install -Dm644 systemd/owlry.service /usr/lib/systemd/user/owlry.service
    sudo install -Dm644 systemd/owlry.socket  /usr/lib/systemd/user/owlry.socket

    echo "Installing man page..."
    sudo install -Dm644 data/owlry.1 /usr/share/man/man1/owlry.1

    echo
    echo "Done. Start daemon: systemctl --user enable --now owlry.service"

# === Version Management ===

# Print the current owlry version.
version:
    @grep '^version' crates/owlry/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'

# Bump the owlry version, update Cargo.lock, commit.
bump new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    toml="crates/owlry/Cargo.toml"
    old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
    if [ "$old" = "{{new_version}}" ]; then
        echo "owlry already at {{new_version}}"
        exit 0
    fi
    echo "Bumping owlry from $old to {{new_version}}"
    sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
    # Keep the man page .TH header in lockstep with the crate version (date + version).
    man="data/owlry.1"
    sed -i "s|^\.TH OWLRY .*|.TH OWLRY 1 \"$(date +%F)\" \"owlry {{new_version}}\" \"User Commands\"|" "$man"
    cargo check -p owlry
    git add "$toml" Cargo.lock "$man"
    git commit -m "chore(owlry): bump version to {{new_version}}"

# === Tagging ===

# Tag the current owlry version as owlry-v<version>.
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    ver=$(just version)
    tag="owlry-v$ver"
    if git rev-parse "$tag" >/dev/null 2>&1; then
        echo "Tag $tag already exists"
        exit 0
    fi
    git tag -a "$tag" -m "owlry v$ver"
    echo "Created tag $tag"

# Push all local tags upstream.
push-tags:
    git push --tags

# === AUR ===
#
# Only one AUR package after the v2 collapse: aur/owlry/. Its subdirectory
# has its own .git pointing at aur.archlinux.org — see aur-stage below.

# Stage AUR files into the main repo index, working around the embedded .git
# that would otherwise make git treat aur/owlry/ as an embedded repo.
aur-stage:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="aur/owlry"
    files=("$dir/PKGBUILD" "$dir/.SRCINFO")
    for f in "$dir"/*.install; do
        [ -f "$f" ] && files+=("$f")
    done
    if [ -d "$dir/.git" ]; then
        mv "$dir/.git" "$dir/.git.bak"
        git add "${files[@]}"
        mv "$dir/.git.bak" "$dir/.git"
    else
        git add "${files[@]}"
    fi

# Refresh aur/owlry/PKGBUILD to point at the current Cargo.toml version's
# tagged source tarball (fetches the tarball to recompute the b2sum) and
# regenerates .SRCINFO. Requires the tag to be pushed beforehand.
aur-update:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/owlry"
    ver=$(just version)
    tag="owlry-v$ver"
    url="https://somegit.dev/Owlibou/owlry/archive/$tag.tar.gz"

    echo "Updating aur/owlry/PKGBUILD to $ver (tag: $tag)"
    sed -i "s/^pkgver=.*/pkgver=$ver/" "$aur_dir/PKGBUILD"
    sed -i 's/^pkgrel=.*/pkgrel=1/' "$aur_dir/PKGBUILD"

    echo "Fetching tagged tarball and computing b2sum..."
    hash=$(curl -fsL "$url" | b2sum | cut -d' ' -f1)
    if [ -z "$hash" ] || [ ${#hash} -lt 64 ]; then
        echo "Error: failed to download or hash $url"
        echo "  (Is the tag pushed? Run 'just push-tags' first.)"
        exit 1
    fi
    sed -i "s|^b2sums=.*|b2sums=('$hash')|" "$aur_dir/PKGBUILD"

    (cd "$aur_dir" && makepkg --printsrcinfo > .SRCINFO)
    echo "aur/owlry/ PKGBUILD updated to $ver."

# Push aur/owlry/ to aur.archlinux.org (via the embedded .git remote).
aur-publish:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/owlry"
    [ -d "$aur_dir/.git" ] || {
        echo "Error: $aur_dir has no AUR git repo. Clone it first:"
        echo "  cd $aur_dir && git init && git remote add origin ssh://aur@aur.archlinux.org/owlry.git"
        exit 1
    }
    cd "$aur_dir"
    ver=$(grep '^pkgver=' PKGBUILD | sed 's/pkgver=//')
    git add -A
    git commit -m "Update to v$ver" || { echo "Nothing to commit"; exit 0; }
    git push origin master
    echo "owlry v$ver published to AUR."

# Show the current AUR PKGBUILD version.
aur-status:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="aur/owlry"
    ver=$(grep '^pkgver=' "$dir/PKGBUILD" | sed 's/pkgver=//')
    rel=$(grep '^pkgrel=' "$dir/PKGBUILD" | sed 's/pkgrel=//')
    if [ -d "$dir/.git" ]; then
        printf "  ✓ owlry  %s-%s\n" "$ver" "$rel"
    else
        printf "  ✗ owlry  %s-%s  (no embedded AUR .git — clone first)\n" "$ver" "$rel"
    fi

# Stage + commit PKGBUILD/.SRCINFO/.install changes into the main repo.
aur-commit msg="chore(aur): update PKGBUILD":
    #!/usr/bin/env bash
    set -euo pipefail
    just aur-stage
    git diff --cached --quiet && { echo "No AUR changes to commit"; exit 0; }
    git commit -m "{{msg}}"

# === Release Workflow ===

# Full release pipeline: bump → push → tag → aur-update → aur-commit → aur-publish.
# Stops between push and tag to give the tagged tarball time to materialise on
# somegit.dev before fetching it for b2sum.
release-owlry new_version:
    #!/usr/bin/env bash
    set -euo pipefail

    just bump {{new_version}}
    git push

    just tag
    just push-tags

    echo "Waiting for tag to propagate..."
    sleep 3

    just aur-update
    just aur-commit "chore(aur): update owlry to {{new_version}}"
    git push

    just aur-publish
    echo
    echo "owlry v{{new_version}} released and published to AUR."

# === Testing ===

# Quick local PKGBUILD build (no chroot, uses host deps).
aur-test-pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    cd aur/owlry
    echo "Testing PKGBUILD via makepkg -sf..."
    makepkg -sf
    echo "Package built successfully."
    ls -lh *.pkg.tar.zst

# Build aur/owlry/ from the local working tree inside a clean extra chroot.
# Patches the PKGBUILD source line to a working-tree tarball; restores on exit.
#
# Requires sudo (extra-x86_64-build runs as root). See scripts/aur-local-test
# for the full implementation.
aur-local-test *args="owlry":
    #!/usr/bin/env bash
    set -euo pipefail
    ts=$(date +%Y%m%d-%H%M%S)
    outfile="build-logs/aur-test-$ts.log"
    mkdir -p build-logs
    scripts/aur-local-test {{args}} 2>&1 | tee "$outfile"
    echo "⌁ Output saved to $outfile"
