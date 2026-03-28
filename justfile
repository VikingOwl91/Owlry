# Owlry build and release automation

default:
    @just --list

# === Build ===

build:
    cargo build --workspace

build-ui:
    cargo build -p owlry

build-daemon:
    cargo build -p owlry-core

release:
    cargo build --workspace --release

release-daemon:
    cargo build -p owlry-core --release

# === Run ===

run *ARGS:
    cargo run -p owlry -- {{ARGS}}

run-daemon *ARGS:
    cargo run -p owlry-core -- {{ARGS}}

# === Quality ===

test:
    cargo test --workspace

check:
    cargo check --workspace
    cargo clippy --workspace

fmt:
    cargo fmt --all

clean:
    cargo clean

# === Install ===

install-local:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Building release..."
    cargo build -p owlry --release --no-default-features
    cargo build -p owlry-core --release
    cargo build -p owlry-lua -p owlry-rune --release

    echo "Creating directories..."
    sudo mkdir -p /usr/lib/owlry/plugins
    sudo mkdir -p /usr/lib/owlry/runtimes

    echo "Installing binaries..."
    sudo install -Dm755 target/release/owlry /usr/bin/owlry
    sudo install -Dm755 target/release/owlryd /usr/bin/owlryd

    echo "Installing runtimes..."
    [ -f target/release/libowlry_lua.so ] && sudo install -Dm755 target/release/libowlry_lua.so /usr/lib/owlry/runtimes/liblua.so
    [ -f target/release/libowlry_rune.so ] && sudo install -Dm755 target/release/libowlry_rune.so /usr/lib/owlry/runtimes/librune.so

    echo "Installing systemd service files..."
    [ -f systemd/owlryd.service ] && sudo install -Dm644 systemd/owlryd.service /usr/lib/systemd/user/owlryd.service
    [ -f systemd/owlryd.socket ] && sudo install -Dm644 systemd/owlryd.socket /usr/lib/systemd/user/owlryd.socket

    echo "Done. Start daemon: systemctl --user enable --now owlryd.service"

# === Version Management ===

show-versions:
    #!/usr/bin/env bash
    echo "=== Crate Versions ==="
    for toml in crates/*/Cargo.toml; do
        name=$(grep '^name' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        ver=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        printf "  %-30s %s\n" "$name" "$ver"
    done

# Get version of a specific crate
crate-version crate:
    @grep '^version' crates/{{crate}}/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'

# Bump a single crate version, update Cargo.lock, commit
bump-crate crate new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    toml="crates/{{crate}}/Cargo.toml"
    [ -f "$toml" ] || { echo "Error: $toml not found"; exit 1; }

    old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
    [ "$old" = "{{new_version}}" ] && { echo "{{crate}} already at {{new_version}}"; exit 0; }

    echo "Bumping {{crate}} from $old to {{new_version}}"
    sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
    cargo check -p {{crate}}
    git add "$toml" Cargo.lock
    git commit -m "chore({{crate}}): bump version to {{new_version}}"
    echo "{{crate}} bumped to {{new_version}}"

# Bump all crates to same version
bump-all new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    for toml in crates/*/Cargo.toml; do
        crate=$(basename $(dirname "$toml"))
        old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        [ "$old" = "{{new_version}}" ] && continue
        echo "Bumping $crate from $old to {{new_version}}"
        sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
    done
    cargo check --workspace
    git add crates/*/Cargo.toml Cargo.lock
    git commit -m "chore: bump all crates to {{new_version}}"
    echo "All crates bumped to {{new_version}}"

# Bump core UI only
bump new_version:
    just bump-crate owlry {{new_version}}

# === Tagging ===

# Tag a specific crate (format: {crate}-v{version})
tag-crate crate:
    #!/usr/bin/env bash
    set -euo pipefail
    ver=$(grep '^version' "crates/{{crate}}/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
    tag="{{crate}}-v$ver"
    if git rev-parse "$tag" >/dev/null 2>&1; then
        echo "Tag $tag already exists"
        exit 0
    fi
    git tag -a "$tag" -m "{{crate}} v$ver"
    echo "Created tag $tag"

# Push all local tags
push-tags:
    git push --tags

# === AUR Package Management ===

# Stage AUR files into the main repo git index.
# AUR subdirs have their own .git (for aur.archlinux.org), which makes
# git treat them as embedded repos. Temporarily hide .git to stage files.
aur-stage pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="aur/{{pkg}}"
    [ -d "$dir" ] || { echo "Error: $dir not found"; exit 1; }

    # Build list of files to stage
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

# Update a specific AUR package PKGBUILD with correct version + checksum
aur-update-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/{{pkg}}"
    [ -d "$aur_dir" ] || { echo "Error: $aur_dir not found"; exit 1; }

    # Determine version
    case "{{pkg}}" in
        owlry-meta-*)
            ver=$(grep '^pkgver=' "$aur_dir/PKGBUILD" | sed 's/pkgver=//')
            echo "Meta-package {{pkg}} at $ver (bump pkgrel manually if needed)"
            (cd "$aur_dir" && makepkg --printsrcinfo > .SRCINFO)
            exit 0
            ;;
        *)
            crate_dir="crates/{{pkg}}"
            [ -d "$crate_dir" ] || { echo "Error: $crate_dir not found"; exit 1; }
            ver=$(grep '^version' "$crate_dir/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
            ;;
    esac

    tag="{{pkg}}-v$ver"
    url="https://somegit.dev/Owlibou/owlry/archive/$tag.tar.gz"

    echo "Updating {{pkg}} to $ver (tag: $tag)"
    sed -i "s/^pkgver=.*/pkgver=$ver/" "$aur_dir/PKGBUILD"
    sed -i 's/^pkgrel=.*/pkgrel=1/' "$aur_dir/PKGBUILD"

    # Update checksum from the tagged tarball
    if grep -q "^source=" "$aur_dir/PKGBUILD"; then
        echo "Downloading tarball and computing checksum..."
        hash=$(curl -sL "$url" | b2sum | cut -d' ' -f1)
        if [ -z "$hash" ] || [ ${#hash} -lt 64 ]; then
            echo "Error: failed to download or hash $url"
            exit 1
        fi
        sed -i "s|^b2sums=.*|b2sums=('$hash')|" "$aur_dir/PKGBUILD"
    fi

    (cd "$aur_dir" && makepkg --printsrcinfo > .SRCINFO)
    echo "{{pkg}} PKGBUILD updated to $ver"

# Shortcut: update core UI AUR package
aur-update:
    just aur-update-pkg owlry

# Publish a specific AUR package to aur.archlinux.org
aur-publish-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/{{pkg}}"
    [ -d "$aur_dir/.git" ] || { echo "Error: $aur_dir has no AUR git repo"; exit 1; }

    cd "$aur_dir"
    ver=$(grep '^pkgver=' PKGBUILD | sed 's/pkgver=//')
    git add -A
    git commit -m "Update to v$ver" || { echo "Nothing to commit"; exit 0; }
    git push origin master
    echo "{{pkg}} v$ver published to AUR!"

# Shortcut: publish core UI to AUR
aur-publish:
    just aur-publish-pkg owlry

# Update and publish ALL AUR packages
aur-update-all:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in aur/*/; do
        pkg=$(basename "$dir")
        [ -f "$dir/PKGBUILD" ] || continue
        echo "=== $pkg ==="
        just aur-update-pkg "$pkg"
        echo ""
    done
    echo "All updated. Run 'just aur-publish-all' to publish."

aur-publish-all:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in aur/*/; do
        pkg=$(basename "$dir")
        [ -d "$dir/.git" ] || continue
        echo "=== $pkg ==="
        just aur-publish-pkg "$pkg"
        echo ""
    done
    echo "All published!"

# Show AUR package status
aur-status:
    #!/usr/bin/env bash
    echo "=== AUR Package Status ==="
    for dir in aur/*/; do
        pkg=$(basename "$dir")
        [ -f "$dir/PKGBUILD" ] || continue
        ver=$(grep '^pkgver=' "$dir/PKGBUILD" | sed 's/pkgver=//')
        if [ -d "$dir/.git" ]; then
            printf "  ✓ %-30s %s\n" "$pkg" "$ver"
        else
            printf "  ✗ %-30s %s (no AUR repo)\n" "$pkg" "$ver"
        fi
    done

# Commit AUR file changes to the main repo (handles embedded .git dirs)
aur-commit msg="chore(aur): update PKGBUILDs":
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in aur/*/; do
        pkg=$(basename "$dir")
        [ -f "$dir/PKGBUILD" ] || continue
        just aur-stage "$pkg"
    done
    git diff --cached --quiet && { echo "No AUR changes to commit"; exit 0; }
    git commit -m "{{msg}}"

# === Release Workflows ===

# Release a single crate: bump → push → tag → update AUR → publish AUR
release-crate crate new_version:
    #!/usr/bin/env bash
    set -euo pipefail

    just bump-crate {{crate}} {{new_version}}
    git push

    just tag-crate {{crate}}
    just push-tags

    echo "Waiting for tag to propagate..."
    sleep 3

    just aur-update-pkg {{crate}}
    just aur-commit "chore(aur): update {{crate}} to {{new_version}}"
    git push

    just aur-publish-pkg {{crate}}
    echo ""
    echo "{{crate}} v{{new_version}} released and published to AUR!"

# === Meta Package Management ===

# Bump meta-package versions
bump-meta new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    for pkg in owlry-meta-essentials owlry-meta-tools owlry-meta-widgets owlry-meta-full; do
        file="aur/$pkg/PKGBUILD"
        old=$(grep '^pkgver=' "$file" | sed 's/pkgver=//')
        if [ "$old" != "{{new_version}}" ]; then
            echo "Bumping $pkg from $old to {{new_version}}"
            sed -i 's/^pkgver=.*/pkgver={{new_version}}/' "$file"
            (cd "aur/$pkg" && makepkg --printsrcinfo > .SRCINFO)
        fi
    done
    echo "Meta-packages bumped to {{new_version}}"

# === Testing ===

# Test a specific AUR package build locally
aur-test-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "aur/{{pkg}}"
    echo "Testing {{pkg}} PKGBUILD..."
    makepkg -sf
    echo "Package built successfully!"
    ls -lh *.pkg.tar.zst
