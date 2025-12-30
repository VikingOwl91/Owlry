# Plugin Development Guide

This guide covers creating plugins for Owlry. There are three ways to extend Owlry:

1. **Native plugins** (Rust) — Best performance, ABI-stable interface
2. **Lua plugins** — Easy scripting, requires `owlry-lua` runtime
3. **Rune plugins** — Safe scripting with Rust-like syntax, requires `owlry-rune` runtime

---

## Quick Start

### Native Plugin (Rust)

```bash
# Create a new plugin crate
cargo new --lib owlry-plugin-myplugin
cd owlry-plugin-myplugin
```

Edit `Cargo.toml`:
```toml
[package]
name = "owlry-plugin-myplugin"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
owlry-plugin-api = { git = "https://somegit.dev/Owlibou/owlry" }
abi_stable = "0.11"
```

Edit `src/lib.rs`:
```rust
use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo,
    ProviderKind, API_VERSION,
};

extern "C" fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: RString::from("myplugin"),
        name: RString::from("My Plugin"),
        version: RString::from(env!("CARGO_PKG_VERSION")),
        description: RString::from("A custom plugin"),
        api_version: API_VERSION,
    }
}

extern "C" fn plugin_providers() -> RVec<ProviderInfo> {
    vec![ProviderInfo {
        id: RString::from("myplugin"),
        name: RString::from("My Plugin"),
        prefix: ROption::RSome(RString::from(":my")),
        icon: RString::from("application-x-executable"),
        provider_type: ProviderKind::Static,
        type_id: RString::from("myplugin"),
    }].into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    ProviderHandle::null()
}

extern "C" fn provider_refresh(_handle: ProviderHandle) -> RVec<PluginItem> {
    vec![
        PluginItem::new("item-1", "Hello World", "echo 'Hello!'")
            .with_description("A greeting")
            .with_icon("face-smile"),
    ].into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    RVec::new()
}

extern "C" fn provider_drop(_handle: ProviderHandle) {}

owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}
```

Build and install:
```bash
cargo build --release
sudo cp target/release/libowlry_plugin_myplugin.so /usr/lib/owlry/plugins/
```

### Lua Plugin

```bash
# Requires owlry-lua runtime
yay -S owlry-lua

# Create plugin directory
mkdir -p ~/.config/owlry/plugins/my-lua-plugin
```

Create `~/.config/owlry/plugins/my-lua-plugin/plugin.toml`:
```toml
[plugin]
id = "my-lua-plugin"
name = "My Lua Plugin"
version = "0.1.0"
description = "A custom Lua plugin"
entry_point = "init.lua"

[[providers]]
id = "myluaprovider"
name = "My Lua Provider"
prefix = ":mylua"
icon = "application-x-executable"
type = "static"
type_id = "mylua"
```

Create `~/.config/owlry/plugins/my-lua-plugin/init.lua`:
```lua
local owlry = require("owlry")

-- Called once at startup for static providers
function refresh()
    return {
        owlry.item("item-1", "Hello from Lua", "echo 'Hello Lua!'")
            :description("A Lua greeting")
            :icon("face-smile"),
    }
end

-- Called per-keystroke for dynamic providers
function query(q)
    return {}
end
```

---

## Native Plugin API

### Plugin VTable

Every native plugin must export a function that returns a vtable:

```rust
#[repr(C)]
pub struct PluginVTable {
    pub info: extern "C" fn() -> PluginInfo,
    pub providers: extern "C" fn() -> RVec<ProviderInfo>,
    pub provider_init: extern "C" fn(provider_id: RStr<'_>) -> ProviderHandle,
    pub provider_refresh: extern "C" fn(handle: ProviderHandle) -> RVec<PluginItem>,
    pub provider_query: extern "C" fn(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem>,
    pub provider_drop: extern "C" fn(handle: ProviderHandle),
}
```

Use the `owlry_plugin!` macro to generate the export:

```rust
owlry_plugin! {
    info: my_info_fn,
    providers: my_providers_fn,
    init: my_init_fn,
    refresh: my_refresh_fn,
    query: my_query_fn,
    drop: my_drop_fn,
}
```

### PluginInfo

```rust
pub struct PluginInfo {
    pub id: RString,           // Unique ID (e.g., "calculator")
    pub name: RString,         // Display name
    pub version: RString,      // Semantic version
    pub description: RString,  // Short description
    pub api_version: u32,      // Must match API_VERSION
}
```

### ProviderInfo

```rust
pub struct ProviderInfo {
    pub id: RString,                    // Provider ID within plugin
    pub name: RString,                  // Display name
    pub prefix: ROption<RString>,       // Activation prefix (e.g., ":calc")
    pub icon: RString,                  // Default icon name
    pub provider_type: ProviderKind,    // Static or Dynamic
    pub type_id: RString,               // Short ID for badges
}

pub enum ProviderKind {
    Static,   // Items loaded at startup via refresh()
    Dynamic,  // Items computed per-query via query()
}
```

### PluginItem

```rust
pub struct PluginItem {
    pub id: RString,                      // Unique item ID
    pub name: RString,                    // Display name
    pub description: ROption<RString>,    // Optional description
    pub icon: ROption<RString>,           // Optional icon
    pub command: RString,                 // Command to execute
    pub terminal: bool,                   // Run in terminal?
    pub keywords: RVec<RString>,          // Search keywords
    pub score_boost: i32,                 // Frecency boost
}

// Builder pattern
let item = PluginItem::new("id", "Name", "command")
    .with_description("Description")
    .with_icon("icon-name")
    .with_terminal(true)
    .with_keywords(vec!["tag1".to_string(), "tag2".to_string()])
    .with_score_boost(100);
```

### ProviderHandle

For stateful providers, use `ProviderHandle` to store state:

```rust
struct MyState {
    items: Vec<PluginItem>,
    cache: HashMap<String, String>,
}

extern "C" fn provider_init(_: RStr<'_>) -> ProviderHandle {
    let state = Box::new(MyState {
        items: Vec::new(),
        cache: HashMap::new(),
    });
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let state = unsafe { &mut *(handle.ptr as *mut MyState) };
    state.items = load_items();
    state.items.clone().into()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        unsafe { handle.drop_as::<MyState>(); }
    }
}
```

### Host API

Plugins can use host-provided functions:

```rust
use owlry_plugin_api::{notify, notify_with_icon, log_info, log_warn, log_error};

// Send notifications
notify("Title", "Body text");
notify_with_icon("Title", "Body", "dialog-information");

// Logging
log_info("Plugin loaded successfully");
log_warn("Cache miss, fetching data");
log_error("Failed to connect to API");
```

### Submenu Support

Plugins can provide submenus for detailed actions:

```rust
// Return an item that opens a submenu
PluginItem::new(
    "service-docker",
    "Docker",
    "SUBMENU:systemd:docker.service",  // Special command format
)

// Handle submenu query (query starts with "?SUBMENU:")
extern "C" fn provider_query(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    let q = query.as_str();

    if let Some(data) = q.strip_prefix("?SUBMENU:") {
        // Return submenu actions
        return vec![
            PluginItem::new("start", "Start", format!("systemctl start {}", data)),
            PluginItem::new("stop", "Stop", format!("systemctl stop {}", data)),
        ].into();
    }

    RVec::new()
}
```

---

## Lua Plugin API

### Plugin Manifest (plugin.toml)

```toml
[plugin]
id = "my-plugin"
name = "My Plugin"
version = "1.0.0"
description = "Plugin description"
entry_point = "init.lua"
owlry_version = ">=0.4.0"  # Optional version constraint

[permissions]
fs = ["read"]       # File system access
http = true         # HTTP requests
process = true      # Spawn processes

[[providers]]
id = "provider1"
name = "Provider Name"
prefix = ":prefix"
icon = "icon-name"
type = "static"     # or "dynamic"
type_id = "shortid"
```

### Lua API

```lua
local owlry = require("owlry")

-- Create items
local item = owlry.item(id, name, command)
    :description("Description")
    :icon("icon-name")
    :terminal(false)
    :keywords({"tag1", "tag2"})

-- Notifications
owlry.notify("Title", "Body")
owlry.notify_icon("Title", "Body", "icon-name")

-- Logging
owlry.log.info("Message")
owlry.log.warn("Warning")
owlry.log.error("Error")

-- File operations (requires fs permission)
local content = owlry.fs.read("/path/to/file")
local files = owlry.fs.list("/path/to/dir")
local exists = owlry.fs.exists("/path")

-- HTTP requests (requires http permission)
local response = owlry.http.get("https://api.example.com/data")
local json = owlry.json.decode(response)

-- Process execution (requires process permission)
local output = owlry.process.run("ls", {"-la"})

-- Cache (persistent across sessions)
owlry.cache.set("key", value, ttl_seconds)
local value = owlry.cache.get("key")
```

### Provider Functions

```lua
-- Static provider: called once at startup
function refresh()
    return {
        owlry.item("id1", "Item 1", "command1"),
        owlry.item("id2", "Item 2", "command2"),
    }
end

-- Dynamic provider: called on each keystroke
function query(q)
    if q == "" then
        return {}
    end

    return {
        owlry.item("result", "Result for: " .. q, "echo " .. q),
    }
end
```

---

## Rune Plugin API

Rune plugins use a Rust-like syntax with memory safety.

### Plugin Manifest

```toml
[plugin]
id = "my-rune-plugin"
name = "My Rune Plugin"
version = "1.0.0"
entry_point = "main.rn"

[[providers]]
id = "runeprovider"
name = "Rune Provider"
type = "static"
```

### Rune API

```rune
use owlry::{Item, log, notify};

pub fn refresh() {
    let items = [];

    items.push(Item::new("id", "Name", "command")
        .description("Description")
        .icon("icon-name"));

    items
}

pub fn query(q) {
    if q.is_empty() {
        return [];
    }

    log::info(`Query: {q}`);

    [Item::new("result", `Result: {q}`, `echo {q}`)]
}
```

---

## Best Practices

### Performance

1. **Static providers**: Do expensive work in `refresh()`, not `items()`
2. **Dynamic providers**: Keep `query()` fast (<50ms)
3. **Cache data**: Use persistent cache for API responses
4. **Lazy loading**: Don't load all items if only a few are needed

### Error Handling

```rust
// Native: Return empty vec on error, log the issue
extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    match load_data() {
        Ok(items) => items.into(),
        Err(e) => {
            log_error(&format!("Failed to load: {}", e));
            RVec::new()
        }
    }
}
```

```lua
-- Lua: Wrap in pcall for safety
function refresh()
    local ok, result = pcall(function()
        return load_items()
    end)

    if not ok then
        owlry.log.error("Failed: " .. result)
        return {}
    end

    return result
end
```

### Icons

Use freedesktop icon names for consistency:
- `application-x-executable` — Generic executable
- `folder` — Directories
- `text-x-generic` — Text files
- `face-smile` — Emoji/reactions
- `system-shutdown` — Power actions
- `network-server` — SSH/network
- `edit-paste` — Clipboard

### Testing

```bash
# Build and test native plugin
cargo build --release -p owlry-plugin-myplugin
cargo test -p owlry-plugin-myplugin

# Install for testing
sudo cp target/release/libowlry_plugin_myplugin.so /usr/lib/owlry/plugins/

# Test with verbose logging
RUST_LOG=debug owlry
```

---

## Publishing to AUR

### PKGBUILD Template

```bash
# Maintainer: Your Name <email@example.com>
pkgname=owlry-plugin-myplugin
pkgver=0.1.0
pkgrel=1
pkgdesc="My custom Owlry plugin"
arch=('x86_64')
url="https://github.com/you/owlry-plugin-myplugin"
license=('GPL-3.0-or-later')
depends=('owlry')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('...')

build() {
    cd "$pkgname-$pkgver"
    cargo build --release
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/lib${pkgname//-/_}.so" \
        "$pkgdir/usr/lib/owlry/plugins/lib${pkgname//-/_}.so"
}
```

---

## Example Plugins

The owlry repository includes 13 native plugins as reference implementations:

| Plugin | Type | Highlights |
|--------|------|------------|
| `owlry-plugin-calculator` | Dynamic | Math parsing, expression evaluation |
| `owlry-plugin-weather` | Static/Widget | HTTP API, JSON parsing, caching |
| `owlry-plugin-systemd` | Static | Submenu actions, service management |
| `owlry-plugin-pomodoro` | Static/Widget | State persistence, notifications |
| `owlry-plugin-clipboard` | Static | External process integration |

Browse the source at `crates/owlry-plugin-*/` for implementation details.
