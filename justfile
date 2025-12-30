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

    echo "Cleaning up stale files..."
    # Remove runtime files that may have ended up in plugins dir (from old installs)
    sudo rm -f /usr/lib/owlry/plugins/libowlry_lua.so /usr/lib/owlry/plugins/libowlry_rune.so
    # Remove old short-named plugin files (from old AUR packages before naming standardization)
    sudo rm -f /usr/lib/owlry/plugins/libbookmarks.so /usr/lib/owlry/plugins/libcalculator.so \
        /usr/lib/owlry/plugins/libclipboard.so /usr/lib/owlry/plugins/libemoji.so \
        /usr/lib/owlry/plugins/libfilesearch.so /usr/lib/owlry/plugins/libmedia.so \
        /usr/lib/owlry/plugins/libpomodoro.so /usr/lib/owlry/plugins/libscripts.so \
        /usr/lib/owlry/plugins/libssh.so /usr/lib/owlry/plugins/libsystem.so \
        /usr/lib/owlry/plugins/libsystemd.so /usr/lib/owlry/plugins/libweather.so \
        /usr/lib/owlry/plugins/libwebsearch.so

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

# Get current version from core crate
version := `grep '^version' crates/owlry/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'`

# Show current version
show-version:
    @echo "Current version: {{version}}"

# Show all crate versions
show-versions:
    #!/usr/bin/env bash
    echo "=== Crate Versions ==="
    for toml in Cargo.toml crates/*/Cargo.toml; do
        name=$(grep '^name' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        ver=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        printf "  %-30s %s\n" "$name" "$ver"
    done

# Get version of a specific crate
crate-version crate:
    @grep '^version' crates/{{crate}}/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'

# Bump a specific crate version (usage: just bump-crate owlry-plugin-calculator 0.2.0)
bump-crate crate new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    toml="crates/{{crate}}/Cargo.toml"
    if [ ! -f "$toml" ]; then
        echo "Error: $toml not found"
        exit 1
    fi
    old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
    if [ "$old" = "{{new_version}}" ]; then
        echo "{{crate}} is already at {{new_version}}, skipping"
        exit 0
    fi
    echo "Bumping {{crate}} from $old to {{new_version}}"
    sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
    cargo check -p {{crate}}
    git add "$toml" Cargo.lock
    git commit -m "chore({{crate}}): bump version to {{new_version}}"
    echo "{{crate}} bumped to {{new_version}}"

# Bump all plugins to same version (usage: just bump-plugins 0.2.0)
bump-plugins new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    for toml in crates/owlry-plugin-*/Cargo.toml; do
        crate=$(basename $(dirname "$toml"))
        old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        if [ "$old" != "{{new_version}}" ]; then
            echo "Bumping $crate from $old to {{new_version}}"
            sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
        fi
    done
    cargo check --workspace
    git add crates/owlry-plugin-*/Cargo.toml Cargo.lock
    git commit -m "chore(plugins): bump all plugins to {{new_version}}"
    echo "All plugins bumped to {{new_version}}"

# Bump meta-packages (no crate, just AUR version)
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

# Bump all crates (core + plugins + runtimes) to same version
bump-all new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    # Bump core
    toml="crates/owlry/Cargo.toml"
    old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
    if [ "$old" != "{{new_version}}" ]; then
        echo "Bumping owlry from $old to {{new_version}}"
        sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
    fi
    # Bump plugins (including plugin-api)
    for toml in crates/owlry-plugin-*/Cargo.toml; do
        crate=$(basename $(dirname "$toml"))
        old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        if [ "$old" != "{{new_version}}" ]; then
            echo "Bumping $crate from $old to {{new_version}}"
            sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
        fi
    done
    # Bump runtimes
    for toml in crates/owlry-lua/Cargo.toml crates/owlry-rune/Cargo.toml; do
        if [ -f "$toml" ]; then
            crate=$(basename $(dirname "$toml"))
            old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
            if [ "$old" != "{{new_version}}" ]; then
                echo "Bumping $crate from $old to {{new_version}}"
                sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
            fi
        fi
    done
    cargo check --workspace
    git add crates/*/Cargo.toml Cargo.lock
    git commit -m "chore: bump all crates to {{new_version}}"
    echo "All crates bumped to {{new_version}}"

# Bump core version (usage: just bump 0.2.0)
bump new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "{{version}}" = "{{new_version}}" ]; then
        echo "Version is already {{new_version}}, skipping bump"
        exit 0
    fi
    echo "Bumping core version from {{version}} to {{new_version}}"
    sed -i 's/^version = ".*"/version = "{{new_version}}"/' crates/owlry/Cargo.toml
    cargo check -p owlry
    git add crates/owlry/Cargo.toml Cargo.lock
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

# === AUR Package Management (individual packages) ===

# Update a specific AUR package (usage: just aur-update-pkg owlry-plugin-calculator)
aur-update-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/{{pkg}}"

    if [ ! -d "$aur_dir" ]; then
        echo "Error: $aur_dir not found"
        exit 1
    fi

    url="https://somegit.dev/Owlibou/owlry"

    # Determine crate version (unified versioning: all crates share same version)
    case "{{pkg}}" in
        owlry-meta-essentials|owlry-meta-tools|owlry-meta-widgets|owlry-meta-full)
            # Meta-packages use static versioning (1.0.0), only bump pkgrel for dep changes
            crate_ver=$(grep '^pkgver=' "$aur_dir/PKGBUILD" | sed 's/pkgver=//')
            ;;
        *)
            # Get version from crate
            crate_dir="crates/{{pkg}}"
            if [ ! -d "$crate_dir" ]; then
                echo "Error: $crate_dir not found"
                exit 1
            fi
            crate_ver=$(grep '^version' "$crate_dir/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
            ;;
    esac

    cd "$aur_dir"

    echo "Updating {{pkg}} PKGBUILD:"
    echo "  pkgver=$crate_ver"

    sed -i "s/^pkgver=.*/pkgver=$crate_ver/" PKGBUILD
    sed -i 's/^pkgrel=.*/pkgrel=1/' PKGBUILD

    # Update checksums (unified versioning: all packages use same version)
    if grep -q "^source=" PKGBUILD; then
        echo "Updating checksums..."
        b2sum=$(curl -sL "$url/archive/v$crate_ver.tar.gz" | b2sum | cut -d' ' -f1)
        sed -i "s/^b2sums=.*/b2sums=('$b2sum')/" PKGBUILD
    fi

    # Generate .SRCINFO
    echo "Generating .SRCINFO..."
    makepkg --printsrcinfo > .SRCINFO

    git diff --stat
    echo ""
    echo "{{pkg}} updated. Run 'just aur-publish-pkg {{pkg}}' to publish."

# Publish a specific AUR package
aur-publish-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/{{pkg}}"

    if [ ! -d "$aur_dir" ]; then
        echo "Error: $aur_dir not found"
        exit 1
    fi

    cd "$aur_dir"
    ver=$(grep '^pkgver=' PKGBUILD | sed 's/pkgver=//')

    git add PKGBUILD .SRCINFO
    git commit -m "Update to v$ver"
    git push origin master

    echo "{{pkg}} v$ver published!"

# Test a specific AUR package build locally
aur-test-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "aur/{{pkg}}"

    echo "Testing {{pkg}} PKGBUILD..."
    makepkg -sf

    echo ""
    echo "Package built successfully!"
    ls -lh *.pkg.tar.zst

# Update all plugin AUR packages
aur-update-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in aur/owlry-plugin-*/; do
        pkg=$(basename "$dir")
        echo "=== Updating $pkg ==="
        just aur-update-pkg "$pkg"
        echo ""
    done

# Publish all plugin AUR packages
aur-publish-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in aur/owlry-plugin-*/; do
        pkg=$(basename "$dir")
        echo "=== Publishing $pkg ==="
        just aur-publish-pkg "$pkg"
        echo ""
    done

# Publish all meta-packages
aur-publish-meta:
    #!/usr/bin/env bash
    set -euo pipefail
    for pkg in owlry-meta-essentials owlry-meta-tools owlry-meta-widgets owlry-meta-full; do
        echo "=== Publishing $pkg ==="
        just aur-publish-pkg "$pkg"
    done
    echo "All meta-packages published!"

# List all AUR packages with their versions
aur-status:
    #!/usr/bin/env bash
    echo "=== AUR Package Status ==="
    for dir in aur/*/; do
        pkg=$(basename "$dir")
        if [ -f "$dir/PKGBUILD" ]; then
            ver=$(grep '^pkgver=' "$dir/PKGBUILD" | sed 's/pkgver=//')
            if [ -d "$dir/.git" ]; then
                status="✓"
            else
                status="✗ (not initialized)"
            fi
            printf "  %s %-30s %s\n" "$status" "$pkg" "$ver"
        fi
    done

# Update ALL AUR packages (core + plugins + runtimes + meta)
aur-update-all:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Updating core ==="
    just aur-update
    echo ""
    echo "=== Updating plugins ==="
    for dir in aur/owlry-plugin-*/; do
        pkg=$(basename "$dir")
        echo "--- $pkg ---"
        just aur-update-pkg "$pkg"
    done
    echo ""
    echo "=== Updating runtimes ==="
    just aur-update-pkg owlry-lua
    just aur-update-pkg owlry-rune
    echo ""
    echo "=== Updating meta-packages ==="
    for pkg in owlry-meta-essentials owlry-meta-tools owlry-meta-widgets owlry-meta-full; do
        echo "--- $pkg ---"
        (cd "aur/$pkg" && makepkg --printsrcinfo > .SRCINFO)
    done
    echo ""
    echo "All AUR packages updated. Run 'just aur-publish-all' to publish."

# Publish ALL AUR packages
aur-publish-all:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Publishing core ==="
    just aur-publish
    echo ""
    echo "=== Publishing plugins ==="
    for dir in aur/owlry-plugin-*/; do
        pkg=$(basename "$dir")
        echo "--- $pkg ---"
        just aur-publish-pkg "$pkg"
    done
    echo ""
    echo "=== Publishing runtimes ==="
    just aur-publish-pkg owlry-lua
    just aur-publish-pkg owlry-rune
    echo ""
    echo "=== Publishing meta-packages ==="
    for pkg in owlry-meta-essentials owlry-meta-tools owlry-meta-widgets owlry-meta-full; do
        echo "--- $pkg ---"
        just aur-publish-pkg "$pkg"
    done
    echo ""
    echo "All AUR packages published!"

# Full release workflow for core only (bump + tag + aur)
release-core new_version: (bump new_version)
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
    echo "Core release v{{new_version}} prepared!"
    echo "Review AUR changes, then run 'just aur-publish'"

# Full release workflow for everything (core + plugins + runtimes)
# Usage: just release-all 0.5.0 0.3.0
# First arg is core version, second is plugins/runtimes version
release-all core_version plugin_version:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "=== Bumping versions ==="
    just bump {{core_version}}
    just bump-all {{plugin_version}}

    echo ""
    echo "=== Pushing to origin ==="
    git push

    echo ""
    echo "=== Creating tag ==="
    just tag

    echo "Waiting for tag to propagate..."
    sleep 2

    echo ""
    echo "=== Updating all AUR packages ==="
    just aur-update-all

    echo ""
    echo "=========================================="
    echo "Release prepared!"
    echo "  Core: v{{core_version}}"
    echo "  Plugins/Runtimes: v{{plugin_version}}"
    echo ""
    echo "Review changes with 'just aur-status'"
    echo "Then publish with 'just aur-publish-all'"
    echo "=========================================="
