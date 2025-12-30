# Owlry build and release automation

# Default recipe
default:
    @just --list

# Build debug (all workspace members)
build:
    cargo build --workspace

# Build core binary only
build-core:
    cargo build -p owlry

# Build release
release:
    cargo build --workspace --release

# Run in debug mode
run *ARGS:
    cargo run -p owlry -- {{ARGS}}

# Run tests
test:
    cargo test --workspace

# Check code
check:
    cargo check --workspace
    cargo clippy --workspace

# Format code
fmt:
    cargo fmt --all

# Clean build artifacts
clean:
    cargo clean

# Build a specific plugin (when plugins exist)
plugin name:
    cargo build -p owlry-plugin-{{name}} --release

# Build all plugins
plugins:
    cargo build --workspace --release --exclude owlry

# Install locally (core + plugins + runtimes)
install-local:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Building release..."
    # Build core without embedded Lua (smaller binary)
    cargo build -p owlry --release --no-default-features
    # Build plugins
    cargo build --workspace --release --exclude owlry

    echo "Creating directories..."
    sudo mkdir -p /usr/lib/owlry/plugins
    sudo mkdir -p /usr/lib/owlry/runtimes

    echo "Installing core binary..."
    sudo install -Dm755 target/release/owlry /usr/bin/owlry

    echo "Installing plugins..."
    for plugin in target/release/libowlry_plugin_*.so; do
        if [ -f "$plugin" ]; then
            name=$(basename "$plugin")
            sudo install -Dm755 "$plugin" "/usr/lib/owlry/plugins/$name"
            echo "  → $name"
        fi
    done

    echo "Installing runtimes..."
    if [ -f "target/release/libowlry_lua.so" ]; then
        sudo install -Dm755 target/release/libowlry_lua.so /usr/lib/owlry/runtimes/liblua.so
        echo "  → liblua.so"
    fi
    if [ -f "target/release/libowlry_rune.so" ]; then
        sudo install -Dm755 target/release/libowlry_rune.so /usr/lib/owlry/runtimes/librune.so
        echo "  → librune.so"
    fi

    echo ""
    echo "Installation complete!"
    echo "  - /usr/bin/owlry"
    echo "  - $(ls /usr/lib/owlry/plugins/*.so 2>/dev/null | wc -l) plugins"
    echo "  - $(ls /usr/lib/owlry/runtimes/*.so 2>/dev/null | wc -l) runtimes"

# === Release Management ===

# AUR package directories (relative to project root)
aur_core_dir := "aur/owlry"

# Get current version from Cargo.toml
version := `grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'`

# Show current version
show-version:
    @echo "Current version: {{version}}"

# Bump version (usage: just bump 0.2.0)
bump new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "{{version}}" = "{{new_version}}" ]; then
        echo "Version is already {{new_version}}, skipping bump"
        exit 0
    fi
    echo "Bumping version from {{version}} to {{new_version}}"
    sed -i 's/^version = ".*"/version = "{{new_version}}"/' Cargo.toml
    cargo check
    git add Cargo.toml Cargo.lock
    git commit -m "chore: bump version to {{new_version}}"
    echo "Version bumped to {{new_version}}"

# Create and push a release tag
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    if git rev-parse "v{{version}}" >/dev/null 2>&1; then
        echo "Tag v{{version}} already exists, skipping"
        exit 0
    fi
    echo "Creating tag v{{version}}"
    git tag -a "v{{version}}" -m "Release v{{version}}"
    git push origin "v{{version}}"
    echo "Tag v{{version}} pushed"

# Update AUR package (core)
aur-update:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_core_dir}}"

    url="https://somegit.dev/Owlibou/owlry"

    echo "Updating PKGBUILD to version {{version}}"
    sed -i 's/^pkgver=.*/pkgver={{version}}/' PKGBUILD
    sed -i 's/^pkgrel=.*/pkgrel=1/' PKGBUILD

    # Update checksums (b2sums)
    echo "Updating checksums..."
    b2sum=$(curl -sL "$url/archive/v{{version}}.tar.gz" | b2sum | cut -d' ' -f1)
    sed -i "s/^b2sums=.*/b2sums=('$b2sum')/" PKGBUILD

    # Generate .SRCINFO
    echo "Generating .SRCINFO..."
    makepkg --printsrcinfo > .SRCINFO

    # Show diff
    git diff

    echo ""
    echo "AUR package updated. Review changes above."
    echo "Run 'just aur-publish' to commit and push."

# Publish AUR package (core)
aur-publish:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_core_dir}}"

    git add PKGBUILD .SRCINFO
    git commit -m "Update to v{{version}}"
    git push

    echo "AUR package v{{version}} published!"

# Test AUR package build locally (core)
aur-test:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_core_dir}}"

    echo "Testing PKGBUILD..."
    makepkg -sf

    echo ""
    echo "Package built successfully!"
    ls -lh *.pkg.tar.zst

# Full release workflow (bump + tag + aur)
release-full new_version: (bump new_version)
    #!/usr/bin/env bash
    set -euo pipefail

    # Push version bump
    git push

    # Create and push tag
    just tag

    # Wait for tag to be available
    echo "Waiting for tag to propagate..."
    sleep 2

    # Update AUR
    just aur-update

    echo ""
    echo "Release v{{new_version}} prepared!"
    echo "Review AUR changes, then run 'just aur-publish'"
