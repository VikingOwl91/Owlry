# Script Runtime Integration for owlry-core Daemon

**Date:** 2026-03-26
**Scope:** Wire up Lua/Rune script runtime loading in the daemon, fix ABI mismatch, add filesystem-watching hot-reload, update plugin documentation
**Repos:** owlry (core), owlry-plugins (docs only)

---

## Problem

The daemon (`owlry-core`) only loads native plugins from `/usr/lib/owlry/plugins/`. User script plugins in `~/.config/owlry/plugins/` are never discovered because `ProviderManager::new_with_config()` never calls the `LoadedRuntime` infrastructure that already exists in `runtime_loader.rs`. Both Lua and Rune runtimes are installed at `/usr/lib/owlry/runtimes/` and functional, but never invoked.

Additionally, the Lua runtime's `RuntimeInfo` struct has 5 fields while the core expects 2, causing a SIGSEGV on cleanup.

---

## 1. Fix Lua RuntimeInfo ABI mismatch

**File:** `owlry/crates/owlry-lua/src/lib.rs`

Shrink Lua's `RuntimeInfo` from 5 fields to 2, matching core and Rune:

```rust
// Before (5 fields — ABI mismatch with core):
pub struct RuntimeInfo {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub description: RString,
    pub api_version: u32,
}

// After (2 fields — matches core/Rune):
pub struct RuntimeInfo {
    pub name: RString,
    pub version: RString,
}
```

Update `runtime_info()` to return only 2 fields. Remove the `LUA_RUNTIME_API_VERSION` constant and `LuaRuntimeVTable` (use the core's `ScriptRuntimeVTable` layout — both already match). The extra metadata (`id`, `description`) was never consumed by the core.

### Vtable `init` signature change

Change the `init` function in the vtable to accept the owlry version as a second parameter:

```rust
// Before:
pub init: extern "C" fn(plugins_dir: RStr<'_>) -> RuntimeHandle,

// After:
pub init: extern "C" fn(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle,
```

This applies to:
- `owlry-core/src/plugins/runtime_loader.rs` — `ScriptRuntimeVTable.init`
- `owlry-lua/src/lib.rs` — `LuaRuntimeVTable.init` and `runtime_init()` implementation
- `owlry-rune/src/lib.rs` — `RuneRuntimeVTable.init` and `runtime_init()` implementation

The core passes its version (`env!("CARGO_PKG_VERSION")` from `owlry-core`) when calling `(vtable.init)(plugins_dir, version)`. Runtimes forward it to `discover_and_load()` instead of hardcoding a version string. This keeps compatibility checks future-proof — no code changes needed on version bumps.

---

## 2. Change default entry points to `main`

**Files:**
- `owlry/crates/owlry-lua/src/manifest.rs` — change `default_entry()` from `"init.lua"` to `"main.lua"`
- `owlry/crates/owlry-rune/src/manifest.rs` — change `default_entry()` from `"init.rn"` to `"main.rn"`

Add `#[serde(alias = "entry_point")]` to the `entry` field in both manifests so existing `plugin.toml` files using `entry_point` continue to work.

---

## 3. Wire runtime loading into ProviderManager

**File:** `owlry/crates/owlry-core/src/providers/mod.rs`

In `ProviderManager::new_with_config()`, after native plugin loading:

1. Get user plugins directory from `paths::plugins_dir()`
2. Get owlry version: `env!("CARGO_PKG_VERSION")`
3. Try `LoadedRuntime::load_lua(&plugins_dir, version)` — log at `info!` if unavailable, not error
4. Try `LoadedRuntime::load_rune(&plugins_dir, version)` — same
5. Call `create_providers()` on each loaded runtime
6. Feed runtime providers into existing categorization (static/dynamic/widget)

`LoadedRuntime::load_lua`, `load_rune`, and `load_from_path` all gain an `owlry_version: &str` parameter, which is passed to `(vtable.init)(plugins_dir, owlry_version)`.

Store `LoadedRuntime` instances on `ProviderManager` in a new field `runtimes: Vec<LoadedRuntime>`. These must stay alive for the daemon's lifetime (they own the `Library` handle via `Arc`).

Remove `#![allow(dead_code)]` from `runtime_loader.rs` since it's now used.

---

## 4. Filesystem watcher for automatic hot-reload

**New file:** `owlry/crates/owlry-core/src/plugins/watcher.rs`
**Modified:** `owlry/crates/owlry-core/src/providers/mod.rs`, `Cargo.toml`

### Dependencies

Add to `owlry-core/Cargo.toml`:
```toml
notify = "7"
notify-debouncer-mini = "0.5"
```

### Watcher design

After initializing runtimes, spawn a background watcher thread:

1. Watch `~/.config/owlry/plugins/` recursively using `notify-debouncer-mini` with 500ms debounce
2. On debounced event (any file create/modify/delete):
   - Acquire write lock on `ProviderManager`
   - Remove all runtime-backed providers from the provider vecs
   - Drop old `LoadedRuntime` instances
   - Re-load runtimes from `/usr/lib/owlry/runtimes/` with fresh plugin discovery
   - Add new runtime providers to provider vecs
   - Refresh the new providers
   - Release write lock

### Provider tracking

`ProviderManager` needs to distinguish runtime providers from native/core providers for selective removal during reload. Options:

- **Tag-based:** Runtime providers already use `ProviderType::Plugin(type_id)`. Keep a `HashSet<String>` of type_ids that came from runtimes. On reload, remove providers whose type_id is in the set.
- **Separate storage:** Store runtime providers in their own vec, separate from native providers. Query merges results from both.

**Chosen: Tag-based.** Simpler — runtime type_ids are tracked in a `runtime_type_ids: HashSet<String>` on `ProviderManager`. Reload clears the set, removes matching providers, then re-adds.

### Thread communication

The watcher thread needs access to `Arc<RwLock<ProviderManager>>`. The `Server` already holds this Arc. Pass a clone to the watcher thread at startup. The watcher acquires `write()` only during reload (~10ms), so read contention is minimal.

### Watcher lifecycle

- Started in `Server::run()` (or `Server::bind()`) before the accept loop
- Runs until the daemon exits (watcher thread is detached or joined on drop)
- Errors in the watcher (e.g., inotify limit exceeded) are logged and the watcher stops — daemon continues without hot-reload

---

## 5. Plugin development documentation

**File:** `owlry-plugins/docs/PLUGIN_DEVELOPMENT.md`

Cover:
- **Plugin directory structure** — `~/.config/owlry/plugins/<name>/plugin.toml` + `main.lua`/`main.rn`
- **Manifest reference** — all `plugin.toml` fields (`id`, `name`, `version`, `description`, `entry`/`entry_point`, `owlry_version`, `[[providers]]` section, `[permissions]` section)
- **Lua plugin guide** — `owlry.provider.register()` API with `refresh` and `query` callbacks, item table format (`id`, `name`, `command`, `description`, `icon`, `terminal`, `tags`)
- **Rune plugin guide** — `pub fn refresh()` and `pub fn query(q)` signatures, `Item::new()` builder, `use owlry::Item`
- **Hot-reload** — changes are picked up automatically, no daemon restart needed
- **Examples** — complete working examples for both Lua and Rune

---

## Out of scope

- Config-gated runtime loading (runtimes self-skip if `.so` not installed)
- Per-plugin selective reload (full runtime reload is fast enough)
- Plugin registry/installation (already exists in the CLI)
- Sandbox enforcement (separate concern, deferred from hardening spec)
