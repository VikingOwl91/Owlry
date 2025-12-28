# Owlry - Claude Code Instructions

## Release Workflow

Always use `just` for releases and AUR deployment:

```bash
# Bump version (updates Cargo.toml + Cargo.lock, commits)
just bump 0.x.y

# Push and create tag
git push && just tag

# Update AUR package
just aur-update

# Review changes, then publish
just aur-publish
```

Do NOT manually edit Cargo.toml for version bumps - use `just bump`.

## Available just recipes

- `just build` / `just release` - Build debug/release
- `just check` - Run cargo check + clippy
- `just test` - Run tests
- `just bump <version>` - Bump version
- `just tag` - Create and push git tag
- `just aur-update` - Update PKGBUILD checksums
- `just aur-publish` - Commit and push to AUR
- `just aur-test` - Test PKGBUILD locally
