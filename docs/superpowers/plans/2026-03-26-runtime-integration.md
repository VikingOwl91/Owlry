# Script Runtime Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable the owlry-core daemon to discover and load Lua/Rune user plugins from `~/.config/owlry/plugins/`, with automatic hot-reload on file changes.

**Architecture:** Fix ABI mismatches between core and runtimes, wire `LoadedRuntime` into `ProviderManager::new_with_config()`, add filesystem watcher for automatic plugin reload. Runtimes are external `.so` libraries loaded from `/usr/lib/owlry/runtimes/`.

**Tech Stack:** Rust 1.90+, notify 7, notify-debouncer-mini 0.5, libloading 0.8

**Repos:**
- Core: `/home/cnachtigall/ssd/git/archive/owlibou/owlry`
- Plugins (docs only): `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins`

---

## Task 1: Fix Lua RuntimeInfo ABI and vtable init signature

**Files:**
- Modify: `crates/owlry-lua/src/lib.rs:42-74,260-279,322-336`
- Modify: `crates/owlry-rune/src/lib.rs:42-46,73-84,90-95,97-146,215-229`
- Modify: `crates/owlry-core/src/plugins/runtime_loader.rs:55-68,84-146,267-277`

### 1a. Shrink Lua RuntimeInfo to 2 fields

- [ ] **Step 1: Update RuntimeInfo struct and runtime_info() in owlry-lua**

In `crates/owlry-lua/src/lib.rs`:

Remove the `LUA_RUNTIME_API_VERSION` constant (line 43).

Replace the `RuntimeInfo` struct (lines 67-74):

```rust
/// Runtime info returned by the runtime
#[repr(C)]
pub struct RuntimeInfo {
    pub name: RString,
    pub version: RString,
}
```

Replace `runtime_info()` (lines 260-268):

```rust
extern "C" fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        name: RString::from("Lua"),
        version: RString::from(env!("CARGO_PKG_VERSION")),
    }
}
```

Remove unused constants `RUNTIME_ID` and `RUNTIME_DESCRIPTION` (lines 37, 40) if no longer referenced.

### 1b. Add owlry_version parameter to vtable init

- [ ] **Step 2: Update ScriptRuntimeVTable in core**

In `crates/owlry-core/src/plugins/runtime_loader.rs`, change the `init` field (line 59):

```rust
pub struct ScriptRuntimeVTable {
    pub info: extern "C" fn() -> RuntimeInfo,
    pub init: extern "C" fn(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle,
    pub providers: extern "C" fn(handle: RuntimeHandle) -> RVec<ScriptProviderInfo>,
    pub refresh: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem>,
    pub query: extern "C" fn(
        handle: RuntimeHandle,
        provider_id: RStr<'_>,
        query: RStr<'_>,
    ) -> RVec<PluginItem>,
    pub drop: extern "C" fn(handle: RuntimeHandle),
}
```

- [ ] **Step 3: Update LoadedRuntime to pass version**

In `crates/owlry-core/src/plugins/runtime_loader.rs`, update `load_lua`, `load_rune`, and `load_from_path` to accept and pass the version:

```rust
impl LoadedRuntime {
    pub fn load_lua(plugins_dir: &Path, owlry_version: &str) -> PluginResult<Self> {
        Self::load_from_path(
            "Lua",
            &PathBuf::from(SYSTEM_RUNTIMES_DIR).join("liblua.so"),
            b"owlry_lua_runtime_vtable",
            plugins_dir,
            owlry_version,
        )
    }

    fn load_from_path(
        name: &'static str,
        library_path: &Path,
        vtable_symbol: &[u8],
        plugins_dir: &Path,
        owlry_version: &str,
    ) -> PluginResult<Self> {
        // ... existing library loading code ...

        // Initialize the runtime with version
        let plugins_dir_str = plugins_dir.to_string_lossy();
        let handle = (vtable.init)(
            RStr::from_str(&plugins_dir_str),
            RStr::from_str(owlry_version),
        );

        // ... rest unchanged ...
    }
}

impl LoadedRuntime {
    pub fn load_rune(plugins_dir: &Path, owlry_version: &str) -> PluginResult<Self> {
        Self::load_from_path(
            "Rune",
            &PathBuf::from(SYSTEM_RUNTIMES_DIR).join("librune.so"),
            b"owlry_rune_runtime_vtable",
            plugins_dir,
            owlry_version,
        )
    }
}
```

- [ ] **Step 4: Update Lua runtime_init to accept version**

In `crates/owlry-lua/src/lib.rs`, update `runtime_init` (line 270) and the vtable:

```rust
extern "C" fn runtime_init(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle {
    let plugins_dir = PathBuf::from(plugins_dir.as_str());
    let mut state = Box::new(LuaRuntimeState::new(plugins_dir));

    state.discover_and_load(owlry_version.as_str());

    RuntimeHandle::from_box(state)
}
```

Update the `LuaRuntimeVTable` struct `init` field to match:
```rust
pub init: extern "C" fn(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle,
```

- [ ] **Step 5: Update Rune runtime_init to accept version**

In `crates/owlry-rune/src/lib.rs`, update `runtime_init` (line 97) and the vtable:

```rust
extern "C" fn runtime_init(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle {
    let _ = env_logger::try_init();

    let plugins_dir = PathBuf::from(plugins_dir.as_str());
    let _version = owlry_version.as_str();
    log::info!(
        "Initializing Rune runtime with plugins from: {}",
        plugins_dir.display()
    );

    // ... rest unchanged — Rune doesn't currently do version checking ...
```

Update the `RuneRuntimeVTable` struct `init` field:
```rust
pub init: extern "C" fn(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle,
```

- [ ] **Step 6: Build all three crates**

Run: `cargo check -p owlry-core && cargo check -p owlry-lua && cargo check -p owlry-rune`
Expected: all pass

- [ ] **Step 7: Run tests**

Run: `cargo test -p owlry-core && cargo test -p owlry-lua && cargo test -p owlry-rune`
Expected: all pass

- [ ] **Step 8: Commit**

```bash
git add crates/owlry-core/src/plugins/runtime_loader.rs \
  crates/owlry-lua/src/lib.rs \
  crates/owlry-rune/src/lib.rs
git commit -m "fix: align runtime ABI — shrink Lua RuntimeInfo, pass owlry_version to init"
```

---

## Task 2: Change default entry points to `main` and add alias

**Files:**
- Modify: `crates/owlry-lua/src/manifest.rs:52-54`
- Modify: `crates/owlry-rune/src/manifest.rs:36-38,29`

- [ ] **Step 1: Update Lua manifest default entry**

In `crates/owlry-lua/src/manifest.rs`, change `default_entry()` (line 52):

```rust
fn default_entry() -> String {
    "main.lua".to_string()
}
```

Add `serde(alias)` to the `entry` field in `PluginInfo` (line 45):

```rust
    #[serde(default = "default_entry", alias = "entry_point")]
    pub entry: String,
```

- [ ] **Step 2: Update Rune manifest default entry**

In `crates/owlry-rune/src/manifest.rs`, change `default_entry()` (line 36):

```rust
fn default_entry() -> String {
    "main.rn".to_string()
}
```

Add `serde(alias)` to the `entry` field in `PluginInfo` (line 29):

```rust
    #[serde(default = "default_entry", alias = "entry_point")]
    pub entry: String,
```

- [ ] **Step 3: Update tests that reference init.lua/init.rn**

In `crates/owlry-lua/src/manifest.rs` test `test_parse_minimal_manifest`:
```rust
assert_eq!(manifest.plugin.entry, "main.lua");
```

In `crates/owlry-lua/src/loader.rs` test `create_test_plugin`:
```rust
fs::write(plugin_dir.join("main.lua"), "-- empty plugin").unwrap();
```

In `crates/owlry-rune/src/manifest.rs` test `test_parse_minimal_manifest`:
```rust
assert_eq!(manifest.plugin.entry, "main.rn");
```

- [ ] **Step 4: Build and test**

Run: `cargo test -p owlry-lua && cargo test -p owlry-rune`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-lua/src/manifest.rs crates/owlry-lua/src/loader.rs \
  crates/owlry-rune/src/manifest.rs
git commit -m "feat: change default entry points to main.lua/main.rn, add entry_point alias"
```

---

## Task 3: Wire runtime loading into ProviderManager

**Files:**
- Modify: `crates/owlry-core/src/providers/mod.rs:106-119,173-224`
- Modify: `crates/owlry-core/src/plugins/runtime_loader.rs:13` (remove allow dead_code)

- [ ] **Step 1: Add runtimes field to ProviderManager**

In `crates/owlry-core/src/providers/mod.rs`, add import and field:

```rust
use crate::plugins::runtime_loader::LoadedRuntime;
```

Add to the `ProviderManager` struct (after `matcher` field):

```rust
pub struct ProviderManager {
    providers: Vec<Box<dyn Provider>>,
    static_native_providers: Vec<NativeProvider>,
    dynamic_providers: Vec<NativeProvider>,
    widget_providers: Vec<NativeProvider>,
    matcher: SkimMatcherV2,
    /// Loaded script runtimes (Lua, Rune) — must stay alive to keep Library handles
    runtimes: Vec<LoadedRuntime>,
    /// Type IDs of providers that came from script runtimes (for hot-reload removal)
    runtime_type_ids: std::collections::HashSet<String>,
}
```

Update `ProviderManager::new()` to initialize the new fields:

```rust
let mut manager = Self {
    providers: core_providers,
    static_native_providers: Vec::new(),
    dynamic_providers: Vec::new(),
    widget_providers: Vec::new(),
    matcher: SkimMatcherV2::default(),
    runtimes: Vec::new(),
    runtime_type_ids: std::collections::HashSet::new(),
};
```

- [ ] **Step 2: Add runtime loading to new_with_config**

In `ProviderManager::new_with_config()`, after the native plugin loading block (after line 221) and before `Self::new(core_providers, native_providers)` (line 223), add runtime loading:

```rust
        // Load script runtimes (Lua, Rune) for user plugins
        let mut runtime_providers: Vec<Box<dyn Provider>> = Vec::new();
        let mut runtimes: Vec<LoadedRuntime> = Vec::new();
        let mut runtime_type_ids = std::collections::HashSet::new();
        let owlry_version = env!("CARGO_PKG_VERSION");

        if let Some(plugins_dir) = crate::paths::plugins_dir() {
            // Try Lua runtime
            match LoadedRuntime::load_lua(&plugins_dir, owlry_version) {
                Ok(rt) => {
                    info!("Loaded Lua runtime with {} provider(s)", rt.providers().len());
                    for provider in rt.create_providers() {
                        let type_id = format!("{}", provider.provider_type());
                        runtime_type_ids.insert(type_id);
                        runtime_providers.push(provider);
                    }
                    runtimes.push(rt);
                }
                Err(e) => {
                    info!("Lua runtime not available: {}", e);
                }
            }

            // Try Rune runtime
            match LoadedRuntime::load_rune(&plugins_dir, owlry_version) {
                Ok(rt) => {
                    info!("Loaded Rune runtime with {} provider(s)", rt.providers().len());
                    for provider in rt.create_providers() {
                        let type_id = format!("{}", provider.provider_type());
                        runtime_type_ids.insert(type_id);
                        runtime_providers.push(provider);
                    }
                    runtimes.push(rt);
                }
                Err(e) => {
                    info!("Rune runtime not available: {}", e);
                }
            }
        }

        let mut manager = Self::new(core_providers, native_providers);
        manager.runtimes = runtimes;
        manager.runtime_type_ids = runtime_type_ids;

        // Add runtime providers to the core providers list
        for provider in runtime_providers {
            info!("Registered runtime provider: {}", provider.name());
            manager.providers.push(provider);
        }

        // Refresh runtime providers
        for provider in &mut manager.providers {
            // Only refresh the ones we just added (runtime providers)
            // They need an initial refresh to populate items
        }
        manager.refresh_all();

        manager
```

Note: This replaces the current `Self::new(core_providers, native_providers)` return. The `refresh_all()` at the end of `new()` will be called, plus we call it again — but that's fine since refresh is idempotent. Actually, `new()` already calls `refresh_all()`, so we should remove the duplicate. Let me adjust:

The cleaner approach is to construct the manager via `Self::new()` which calls `refresh_all()`, then set the runtime fields and add providers, then call `refresh_all()` once more for the newly added runtime providers. Or better — add runtime providers to `core_providers` before calling `new()`:

```rust
        // Merge runtime providers into core providers
        let mut all_core_providers = core_providers;
        for provider in runtime_providers {
            info!("Registered runtime provider: {}", provider.name());
            all_core_providers.push(provider);
        }

        let mut manager = Self::new(all_core_providers, native_providers);
        manager.runtimes = runtimes;
        manager.runtime_type_ids = runtime_type_ids;
        manager
```

This way `new()` handles the single `refresh_all()` call.

- [ ] **Step 3: Remove allow(dead_code) from runtime_loader**

In `crates/owlry-core/src/plugins/runtime_loader.rs`, remove `#![allow(dead_code)]` (line 13).

Fix any resulting dead code warnings by removing unused `#[allow(dead_code)]` attributes on individual items that are now actually used, or adding targeted `#[allow(dead_code)]` only on truly unused items.

- [ ] **Step 4: Build and test**

Run: `cargo check -p owlry-core && cargo test -p owlry-core`
Expected: all pass. May see info logs about runtimes loading (if installed on the build machine).

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/providers/mod.rs \
  crates/owlry-core/src/plugins/runtime_loader.rs
git commit -m "feat: wire script runtime loading into daemon ProviderManager"
```

---

## Task 4: Filesystem watcher for hot-reload

**Files:**
- Create: `crates/owlry-core/src/plugins/watcher.rs`
- Modify: `crates/owlry-core/src/plugins/mod.rs:23-28` (add module)
- Modify: `crates/owlry-core/src/providers/mod.rs` (add reload method)
- Modify: `crates/owlry-core/src/server.rs:59-78` (start watcher)
- Modify: `crates/owlry-core/Cargo.toml` (add deps)

- [ ] **Step 1: Add dependencies**

In `crates/owlry-core/Cargo.toml`, add to `[dependencies]`:

```toml
# Filesystem watching for plugin hot-reload
notify = "7"
notify-debouncer-mini = "0.5"
```

- [ ] **Step 2: Add reload_runtimes method to ProviderManager**

In `crates/owlry-core/src/providers/mod.rs`, add a method:

```rust
    /// Reload all script runtime providers (called by filesystem watcher)
    pub fn reload_runtimes(&mut self) {
        // Remove old runtime providers from the core providers list
        self.providers.retain(|p| {
            let type_str = format!("{}", p.provider_type());
            !self.runtime_type_ids.contains(&type_str)
        });

        // Drop old runtimes
        self.runtimes.clear();
        self.runtime_type_ids.clear();

        let owlry_version = env!("CARGO_PKG_VERSION");
        let plugins_dir = match crate::paths::plugins_dir() {
            Some(d) => d,
            None => return,
        };

        // Reload Lua runtime
        match LoadedRuntime::load_lua(&plugins_dir, owlry_version) {
            Ok(rt) => {
                info!("Reloaded Lua runtime with {} provider(s)", rt.providers().len());
                for provider in rt.create_providers() {
                    let type_id = format!("{}", provider.provider_type());
                    self.runtime_type_ids.insert(type_id);
                    self.providers.push(provider);
                }
                self.runtimes.push(rt);
            }
            Err(e) => {
                info!("Lua runtime not available on reload: {}", e);
            }
        }

        // Reload Rune runtime
        match LoadedRuntime::load_rune(&plugins_dir, owlry_version) {
            Ok(rt) => {
                info!("Reloaded Rune runtime with {} provider(s)", rt.providers().len());
                for provider in rt.create_providers() {
                    let type_id = format!("{}", provider.provider_type());
                    self.runtime_type_ids.insert(type_id);
                    self.providers.push(provider);
                }
                self.runtimes.push(rt);
            }
            Err(e) => {
                info!("Rune runtime not available on reload: {}", e);
            }
        }

        // Refresh the newly added providers
        for provider in &mut self.providers {
            provider.refresh();
        }

        info!("Runtime reload complete");
    }
```

- [ ] **Step 3: Create the watcher module**

Create `crates/owlry-core/src/plugins/watcher.rs`:

```rust
//! Filesystem watcher for user plugin hot-reload
//!
//! Watches `~/.config/owlry/plugins/` for changes and triggers
//! runtime reload when plugin files are modified.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use log::{info, warn};
use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};

use crate::providers::ProviderManager;

/// Start watching the user plugins directory for changes.
///
/// Spawns a background thread that monitors the directory and triggers
/// a full runtime reload on any file change. Returns immediately.
///
/// If the plugins directory doesn't exist or the watcher fails to start,
/// logs a warning and returns without spawning a thread.
pub fn start_watching(pm: Arc<RwLock<ProviderManager>>) {
    let plugins_dir = match crate::paths::plugins_dir() {
        Some(d) => d,
        None => {
            info!("No plugins directory configured, skipping file watcher");
            return;
        }
    };

    if !plugins_dir.exists() {
        // Create the directory so the watcher has something to watch
        if std::fs::create_dir_all(&plugins_dir).is_err() {
            warn!("Failed to create plugins directory: {}", plugins_dir.display());
            return;
        }
    }

    thread::spawn(move || {
        if let Err(e) = watch_loop(&plugins_dir, &pm) {
            warn!("Plugin watcher stopped: {}", e);
        }
    });

    info!("Plugin file watcher started for {}", plugins_dir.display());
}

fn watch_loop(
    plugins_dir: &PathBuf,
    pm: &Arc<RwLock<ProviderManager>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;

    debouncer
        .watcher()
        .watch(plugins_dir.as_ref(), notify::RecursiveMode::Recursive)?;

    info!("Watching {} for plugin changes", plugins_dir.display());

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Check if any event is relevant (not just access/metadata)
                let has_relevant_change = events.iter().any(|e| {
                    matches!(e.kind, DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous)
                });

                if has_relevant_change {
                    info!("Plugin file change detected, reloading runtimes...");
                    let mut pm_guard = pm.write().unwrap_or_else(|e| e.into_inner());
                    pm_guard.reload_runtimes();
                }
            }
            Ok(Err(errors)) => {
                for e in errors {
                    warn!("File watcher error: {}", e);
                }
            }
            Err(e) => {
                // Channel closed — watcher was dropped
                return Err(Box::new(e));
            }
        }
    }
}
```

- [ ] **Step 4: Register the watcher module**

In `crates/owlry-core/src/plugins/mod.rs`, add after line 28 (`pub mod runtime_loader;`):

```rust
pub mod watcher;
```

- [ ] **Step 5: Start watcher in Server::run**

In `crates/owlry-core/src/server.rs`, in the `run()` method, before the accept loop, add:

```rust
pub fn run(&self) -> io::Result<()> {
    // Start filesystem watcher for user plugin hot-reload
    crate::plugins::watcher::start_watching(Arc::clone(&self.provider_manager));

    info!("Server entering accept loop");
    for stream in self.listener.incoming() {
```

- [ ] **Step 6: Build and test**

Run: `cargo check -p owlry-core && cargo test -p owlry-core`
Expected: all pass

- [ ] **Step 7: Manual smoke test**

```bash
# Start the daemon
RUST_LOG=info cargo run -p owlry-core

# In another terminal, create a test plugin
mkdir -p ~/.config/owlry/plugins/hotreload-test
cat > ~/.config/owlry/plugins/hotreload-test/plugin.toml << 'EOF'
[plugin]
id = "hotreload-test"
name = "Hot Reload Test"
version = "0.1.0"
EOF
cat > ~/.config/owlry/plugins/hotreload-test/main.lua << 'EOF'
owlry.provider.register({
    name = "hotreload-test",
    refresh = function()
        return {{ id = "hr1", name = "Hot Reload Works!", command = "echo yes" }}
    end,
})
EOF

# Watch daemon logs — should see "Plugin file change detected, reloading runtimes..."
# Clean up after testing
rm -rf ~/.config/owlry/plugins/hotreload-test
```

- [ ] **Step 8: Commit**

```bash
git add crates/owlry-core/Cargo.toml \
  crates/owlry-core/src/plugins/watcher.rs \
  crates/owlry-core/src/plugins/mod.rs \
  crates/owlry-core/src/providers/mod.rs \
  crates/owlry-core/src/server.rs
git commit -m "feat: add filesystem watcher for automatic user plugin hot-reload"
```

---

## Task 5: Update plugin development documentation

**Files:**
- Modify: `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins/docs/PLUGIN_DEVELOPMENT.md`

- [ ] **Step 1: Update Lua plugin section**

In `docs/PLUGIN_DEVELOPMENT.md`, update the Lua Quick Start section (around line 101):

Change `entry_point = "init.lua"` to `entry = "main.lua"` in the manifest example.

Replace the Lua code example with the `owlry.provider.register()` API:

```lua
owlry.provider.register({
    name = "myluaprovider",
    display_name = "My Lua Provider",
    type_id = "mylua",
    default_icon = "application-x-executable",
    prefix = ":mylua",
    refresh = function()
        return {
            { id = "item-1", name = "Hello from Lua", command = "echo 'Hello Lua!'" },
        }
    end,
})
```

Remove `local owlry = require("owlry")` — the `owlry` table is pre-registered globally.

- [ ] **Step 2: Update Rune plugin section**

Update the Rune manifest example to use `entry = "main.rn"` instead of `entry_point = "main.rn"`.

- [ ] **Step 3: Update manifest reference**

In the Lua Plugin API manifest section (around line 325), change `entry_point` to `entry` and add a note:

```toml
[plugin]
id = "my-plugin"
name = "My Plugin"
version = "1.0.0"
description = "Plugin description"
entry = "main.lua"            # Default: main.lua (Lua) / main.rn (Rune)
                               # Alias: entry_point also accepted
owlry_version = ">=1.0.0"     # Optional version constraint
```

- [ ] **Step 4: Add hot-reload documentation**

Add a new section after "Best Practices" (before "Publishing to AUR"):

```markdown
## Hot Reload

User plugins in `~/.config/owlry/plugins/` are automatically reloaded when files change.
The daemon watches the plugins directory and reloads all script runtimes when any file
is created, modified, or deleted. No daemon restart is needed.

**What triggers a reload:**
- Creating a new plugin directory with `plugin.toml`
- Editing a plugin's script files (`main.lua`, `main.rn`, etc.)
- Editing a plugin's `plugin.toml`
- Deleting a plugin directory

**What does NOT trigger a reload:**
- Changes to native plugins (`.so` files) — these require a daemon restart
- Changes to runtime libraries in `/usr/lib/owlry/runtimes/` — daemon restart needed

**Reload behavior:**
- All script runtimes (Lua, Rune) are fully reloaded
- Existing search results may briefly show stale data during reload
- Errors in plugins are logged but don't affect other plugins
```

- [ ] **Step 5: Update Lua provider functions section**

Replace the bare `refresh()`/`query()` examples (around line 390) with the register API:

```lua
-- Static provider: called once at startup and on reload
owlry.provider.register({
    name = "my-provider",
    display_name = "My Provider",
    prefix = ":my",
    refresh = function()
        return {
            { id = "id1", name = "Item 1", command = "command1" },
            { id = "id2", name = "Item 2", command = "command2" },
        }
    end,
})

-- Dynamic provider: called on each keystroke
owlry.provider.register({
    name = "my-search",
    display_name = "My Search",
    prefix = "?my",
    query = function(q)
        if q == "" then return {} end
        return {
            { id = "result", name = "Result for: " .. q, command = "echo " .. q },
        }
    end,
})
```

- [ ] **Step 6: Commit**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add docs/PLUGIN_DEVELOPMENT.md
git commit -m "docs: update plugin development guide for main.lua/rn defaults, register API, hot-reload"
```

---

## Task 6: Update hello-test plugin and clean up

**Files:**
- Modify: `~/.config/owlry/plugins/hello-test/plugin.toml`
- Modify: `~/.config/owlry/plugins/hello-test/init.lua` → rename to `main.lua`

This is a local-only task, not committed to either repo.

- [ ] **Step 1: Update hello-test plugin**

```bash
# Rename entry point
mv ~/.config/owlry/plugins/hello-test/init.lua ~/.config/owlry/plugins/hello-test/main.lua

# Update manifest to use entry field
cat > ~/.config/owlry/plugins/hello-test/plugin.toml << 'EOF'
[plugin]
id = "hello-test"
name = "Hello Test"
version = "0.1.0"
description = "Minimal test plugin for verifying Lua runtime loading"
EOF
```

- [ ] **Step 2: End-to-end verification**

```bash
# Rebuild and restart daemon
cargo build -p owlry-core
RUST_LOG=info cargo run -p owlry-core

# Expected log output should include:
# - "Loaded Lua runtime with 1 provider(s)" (hello-test)
# - "Loaded Rune runtime with 1 provider(s)" (hyprshutdown)
# - "Plugin file watcher started for ..."
```
