# Owlry build and release automation

# Default recipe
default:
    @just --list

# Build debug (all workspace members)
build:
    cargo build --workspace

# Build UI binary only
build-ui:
    cargo build -p owlry

# Build core daemon only
build-daemon:
    cargo build -p owlry-core

# Build core daemon release
release-daemon:
    cargo build -p owlry-core --release

# Run core daemon
run-daemon *ARGS:
    cargo run -p owlry-core -- {{ARGS}}

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

# Install locally (core + runtimes)
install-local:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Building release..."
    # Build UI without embedded Lua (smaller binary)
    cargo build -p owlry --release --no-default-features
    # Build core daemon
    cargo build -p owlry-core --release
    # Build runtimes
    cargo build -p owlry-lua -p owlry-rune --release

    echo "Creating directories..."
    sudo mkdir -p /usr/lib/owlry/plugins
    sudo mkdir -p /usr/lib/owlry/runtimes

    echo "Installing binaries..."
    sudo install -Dm755 target/release/owlry /usr/bin/owlry
    sudo install -Dm755 target/release/owlry-core /usr/bin/owlry-core

    echo "Installing runtimes..."
    if [ -f "target/release/libowlry_lua.so" ]; then
        sudo install -Dm755 target/release/libowlry_lua.so /usr/lib/owlry/runtimes/liblua.so
        echo "  → liblua.so"
    fi
    if [ -f "target/release/libowlry_rune.so" ]; then
        sudo install -Dm755 target/release/libowlry_rune.so /usr/lib/owlry/runtimes/librune.so
        echo "  → librune.so"
    fi

    echo "Installing systemd service files..."
    if [ -f "systemd/owlry-core.service" ]; then
        sudo install -Dm644 systemd/owlry-core.service /usr/lib/systemd/user/owlry-core.service
        echo "  → owlry-core.service"
    fi
    if [ -f "systemd/owlry-core.socket" ]; then
        sudo install -Dm644 systemd/owlry-core.socket /usr/lib/systemd/user/owlry-core.socket
        echo "  → owlry-core.socket"
    fi

    echo ""
    echo "Installation complete!"
    echo "  - /usr/bin/owlry (UI)"
    echo "  - /usr/bin/owlry-core (daemon)"
    echo "  - $(ls /usr/lib/owlry/runtimes/*.so 2>/dev/null | wc -l) runtimes"
    echo "  - systemd: owlry-core.service, owlry-core.socket"
    echo ""
    echo "To start the daemon:"
    echo "  systemctl --user enable --now owlry-core.service"
    echo "  OR add 'exec-once = owlry-core' to your compositor config"
    echo ""
    echo "Note: Install plugins separately from the owlry-plugins repo."

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

# Bump a specific crate version (usage: just bump-crate owlry-core 0.2.0)
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

# Bump all crates (core UI + daemon + plugin-api + runtimes) to same version
bump-all new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    for toml in crates/*/Cargo.toml; do
        crate=$(basename $(dirname "$toml"))
        old=$(grep '^version' "$toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        if [ "$old" != "{{new_version}}" ]; then
            echo "Bumping $crate from $old to {{new_version}}"
            sed -i 's/^version = ".*"/version = "{{new_version}}"/' "$toml"
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

# Update AUR package (core UI)
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

# Publish AUR package (core UI)
aur-publish:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_core_dir}}"

    git add PKGBUILD .SRCINFO
    git commit -m "Update to v{{version}}"
    git push

    echo "AUR package v{{version}} published!"

# Test AUR package build locally (core UI)
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

# Update a specific AUR package (usage: just aur-update-pkg owlry-core)
aur-update-pkg pkg:
    #!/usr/bin/env bash
    set -euo pipefail
    aur_dir="aur/{{pkg}}"

    if [ ! -d "$aur_dir" ]; then
        echo "Error: $aur_dir not found"
        exit 1
    fi

    url="https://somegit.dev/Owlibou/owlry"

    # Determine crate version
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

    # Update checksums
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

# Update ALL AUR packages (core + daemon + runtimes + meta)
aur-update-all:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Updating core UI ==="
    just aur-update
    echo ""
    echo "=== Updating core daemon ==="
    just aur-update-pkg owlry-core
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
    echo "=== Publishing core UI ==="
    just aur-publish
    echo ""
    echo "=== Publishing core daemon ==="
    just aur-publish-pkg owlry-core
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
