# Built-in Providers Migration — Design Spec

## Goal

Move calculator, converter, and system from external `.so` plugins (owlry-plugins repo) to native providers compiled into `owlry-core`. Remove 3 plugin AUR packages (transitional), 4 meta AUR packages (already deleted). Update READMEs for both repos.

## Architecture

The 3 plugins currently use the FFI plugin API (`PluginVTable`, `PluginItem`, etc.) and are loaded as `.so` files by `NativePluginLoader`. As built-in providers, they become native Rust modules inside `owlry-core/src/providers/` implementing the existing `Provider` trait — same as `ApplicationProvider` and `CommandProvider`.

No changes to the plugin system itself. External plugins continue to work via `.so` loading.

## Components

### New modules in owlry-core

- `providers/calculator.rs` — port of owlry-plugin-calculator (231 lines, depends on `meval`)
- `providers/converter/mod.rs` — port of owlry-plugin-converter entry point
- `providers/converter/parser.rs` — query parsing (235 lines, no new deps)
- `providers/converter/units.rs` — unit definitions + conversion (944 lines, no new deps)
- `providers/converter/currency.rs` — ECB rate fetching (313 lines, depends on `reqwest` blocking + `dirs` + `serde`)
- `providers/system.rs` — port of owlry-plugin-system (257 lines, no new deps)

### New owlry-core dependencies

- `meval` — math expression evaluation (currently optional behind `lua` feature, make required)
- `reqwest` with `blocking` feature — ECB currency rate fetching (currently optional behind `lua`, make required)
- `dirs` — already a dependency
- `serde`/`serde_json` — already dependencies

### Modified files

- `owlry-core/src/providers/mod.rs` — register the 3 new providers in `ProviderManager`, honor config toggles, classify calculator+converter as dynamic providers
- `owlry-core/Cargo.toml` — move `meval` and `reqwest` from optional to required
- `owlry-core/src/config/mod.rs` — add `converter` config toggle (calculator and system already exist)

### Provider classification

- Calculator → dynamic (queried per-keystroke via `query()`)
- Converter → dynamic (queried per-keystroke via `query()`)
- System → static (populated at `refresh()`, returns fixed list of actions)

## Provider Type IDs

Built-in providers use `ProviderType::Plugin(String)` with fixed IDs to maintain backward compatibility with the UI highlighting and filter system:

- Calculator: `ProviderType::Plugin("calc".into())`
- Converter: `ProviderType::Plugin("conv".into())`
- System: `ProviderType::Plugin("sys".into())`

This ensures the UI's highlighting logic (`matches!(id.as_str(), "calc" | "conv")`) and CSS badge classes (`.owlry-badge-calc`, `.owlry-badge-sys`) continue to work without changes.

## Config

Existing toggles in `[providers]`:

```toml
[providers]
calculator = true   # already exists
system = true       # already exists
converter = true    # new — add with default true
```

When a toggle is false, the provider is not registered in `ProviderManager` at startup.

## Currency Conversion

The converter's currency feature uses `reqwest` (blocking) to fetch ECB exchange rates with a 24-hour file cache at `~/.cache/owlry/ecb_rates.json`. If the HTTP fetch fails (no network, timeout), currency conversion silently returns no results — unit conversion still works. This matches current plugin behavior.

## AUR Changes

### Main repo (owlry)

- `aur/owlry-core/PKGBUILD` — bump version
- Remove `aur/owlry-meta-*` directories (4 dirs, already deleted from AUR)

### Plugins repo (owlry-plugins)

- Remove crates: `owlry-plugin-calculator`, `owlry-plugin-converter`, `owlry-plugin-system`
- Remove AUR dirs: `aur/owlry-plugin-calculator`, `aur/owlry-plugin-converter`, `aur/owlry-plugin-system` from tracked files
- Push transitional PKGBUILDs to the 3 AUR repos:

```bash
pkgname=owlry-plugin-calculator  # (and converter, system)
pkgver=<last_version>
pkgrel=99
pkgdesc="Transitional package — calculator is now built into owlry-core"
arch=('any')
depends=('owlry-core>=<new_version>')
replaces=('owlry-plugin-calculator')
# No source, no build, no package body
```

### Conflict prevention

When owlry-core gains built-in calculator/converter/system, users who have the old `.so` plugins installed will have both the built-in provider AND the `.so` plugin active — duplicate results. The daemon should detect this: if a built-in provider ID matches a loaded native plugin ID, skip the native plugin. Add this check in `ProviderManager` when registering native plugins.

## README Updates

### Main repo README

- Package table: remove separate plugin entries for calculator, converter, system — note them as built-in to owlry-core
- Remove meta package section entirely
- Update install examples (no need to install calculator/converter/system separately)

### Plugins repo README

- Remove calculator, converter, system from plugin listing
- Add note that these 3 are built into owlry-core

## Testing

- Port existing plugin tests directly — they test provider logic, not FFI wrappers
- `cargo test -p owlry-core --lib` covers all 3 new providers
- Add conflict detection test (built-in provider ID vs native plugin ID)
- Manual verification: `= 5+3` (calc), `20F` (conv), `20 euro to dollar` (currency), system actions
