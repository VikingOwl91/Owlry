# Owlry build and release automation

# Default recipe
default:
    @just --list

# Build debug
build:
    cargo build

# Build release
release:
    cargo build --release

# Run in debug mode
run *ARGS:
    cargo run -- {{ARGS}}

# Run tests
test:
    cargo test

# Check code
check:
    cargo check
    cargo clippy

# Format code
fmt:
    cargo fmt

# Clean build artifacts
clean:
    cargo clean

# === Release Management ===

# AUR package directory
aur_dir := "/home/cnachtigall/data/git/aur/owlry"

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
    echo "Creating tag v{{version}}"
    git tag -a "v{{version}}" -m "Release v{{version}}"
    git push origin "v{{version}}"
    echo "Tag v{{version}} pushed"

# Update AUR package
aur-update:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_dir}}"

    echo "Updating PKGBUILD to version {{version}}"
    sed -i 's/^pkgver=.*/pkgver={{version}}/' PKGBUILD
    sed -i 's/^pkgrel=.*/pkgrel=1/' PKGBUILD

    # Update checksums
    echo "Updating checksums..."
    updpkgsums

    # Generate .SRCINFO
    echo "Generating .SRCINFO..."
    makepkg --printsrcinfo > .SRCINFO

    # Show diff
    git diff

    echo ""
    echo "AUR package updated. Review changes above."
    echo "Run 'just aur-publish' to commit and push."

# Publish AUR package
aur-publish:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_dir}}"

    git add PKGBUILD .SRCINFO
    git commit -m "Update to v{{version}}"
    git push

    echo "AUR package v{{version}} published!"

# Test AUR package build locally
aur-test:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{aur_dir}}"

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
