# Owlry Architecture Split — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split owlry into a client/daemon architecture with two repos, enabling independent release cadence and clean separation between UI and backend.

**Architecture:** `owlry-core` daemon manages plugins, providers, frecency, and config over a Unix socket IPC. `owlry` is a thin GTK4 client that connects to the daemon for search/launch. Plugins move to a separate `owlry-plugins` repo with independent versioning.

**Tech Stack:** Rust 1.90+, GTK4 0.10, abi_stable 0.11, serde_json (IPC protocol), tokio or std Unix sockets

**Spec:** `docs/superpowers/specs/2026-03-26-architecture-split-design.md`

---

## File Structure Overview

### New files to create

**In `crates/owlry-core/`:**
- `Cargo.toml` — new crate (lib + bin)
- `src/lib.rs` — library root, re-exports
- `src/main.rs` — daemon entry point
- `src/server.rs` — Unix socket IPC server
- `src/ipc.rs` — shared IPC message types (request/response enums)

**In `crates/owlry/`:**
- `src/client.rs` — IPC client connecting to daemon

**In `owlry-plugins/`:**
- `Cargo.toml` — new workspace root
- `justfile` — plugin-specific build/release automation
- `README.md`

**In `systemd/`:**
- `owlry-core.service` — systemd user service
- `owlry-core.socket` — socket activation unit

### Files to move

**From `crates/owlry/src/` → `crates/owlry-core/src/`:**
- `config/mod.rs`
- `data/mod.rs`, `data/frecency.rs`
- `filter.rs`
- `providers/mod.rs`, `providers/application.rs`, `providers/command.rs`, `providers/native_provider.rs`, `providers/lua_provider.rs`
- `plugins/` (entire directory — mod.rs, native_loader.rs, runtime_loader.rs, loader.rs, manifest.rs, registry.rs, commands.rs, error.rs, runtime.rs, api/)
- `notify.rs`
- `paths.rs`

**From `crates/owlry-plugin-*` → `owlry-plugins/crates/owlry-plugin-*`:**
- All 14 plugin crates
- `owlry-lua`, `owlry-rune` runtime crates

**From `aur/` → `owlry-plugins/aur/`:**
- All `owlry-plugin-*` directories
- `owlry-lua`, `owlry-rune` directories

### Files to modify significantly

- `crates/owlry/Cargo.toml` — remove backend deps, add owlry-core dep
- `crates/owlry/src/main.rs` — remove plugin subcommand handling (moves to owlry-core)
- `crates/owlry/src/app.rs` — replace direct provider/plugin calls with IPC client
- `crates/owlry/src/cli.rs` — remove `--providers`, add `--profile`, restructure
- `crates/owlry/src/ui/main_window.rs` — use IPC client instead of direct ProviderManager
- `Cargo.toml` (root) — remove plugin/runtime workspace members
- `justfile` — split into core justfile + plugins justfile
- `README.md` — update for new architecture
- `CLAUDE.md` — update for new structure
- All plugin `Cargo.toml` files — change owlry-plugin-api from path to git dep

---

## Phase 0: Dependency Refresh

### Task 1: Update all external dependencies to latest stable

**Files:**
- Modify: `Cargo.toml` (root)
- Modify: `crates/owlry/Cargo.toml`
- Modify: `crates/owlry-plugin-api/Cargo.toml`
- Modify: all `crates/owlry-plugin-*/Cargo.toml`
- Modify: `crates/owlry-lua/Cargo.toml`
- Modify: `crates/owlry-rune/Cargo.toml`

- [ ] **Step 1: Check current outdated dependencies**

Run: `cd /home/cnachtigall/ssd/git/archive/owlibou/owlry && cargo outdated -R 2>/dev/null || cargo update --dry-run`

Review output and identify which dependencies have newer versions available.

- [ ] **Step 2: Update Cargo.lock with latest compatible versions**

Run: `cargo update`

This updates within existing semver constraints.

- [ ] **Step 3: Review and bump version constraints in Cargo.toml files**

For each crate, check if major version bumps are available for key dependencies:
- `gtk4`: check latest 0.x
- `abi_stable`: check latest
- `reqwest`: check latest
- `mlua`: check latest
- `rune`: check latest
- `clap`: check latest 4.x
- `serde`: check latest 1.x
- `chrono`: check latest 0.4.x

Update version constraints in each `Cargo.toml` that uses them.

- [ ] **Step 4: Build and verify**

Run: `cargo build --workspace`
Run: `cargo test --workspace`
Run: `cargo clippy --workspace`

All must pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.lock Cargo.toml crates/*/Cargo.toml
git commit -m "chore: update all dependencies to latest stable"
```

---

## Phase 1: Extract `owlry-core` as Library Crate

### Task 2: Create `owlry-core` crate scaffold

**Files:**
- Create: `crates/owlry-core/Cargo.toml`
- Create: `crates/owlry-core/src/lib.rs`
- Modify: `Cargo.toml` (root workspace — add member)

- [ ] **Step 1: Create `crates/owlry-core/Cargo.toml`**

```toml
[package]
name = "owlry-core"
version = "0.5.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Core daemon for the Owlry application launcher"

[lib]
name = "owlry_core"
path = "src/lib.rs"

[dependencies]
owlry-plugin-api = { path = "../owlry-plugin-api" }

# Provider system
fuzzy-matcher = "0.3"
freedesktop-desktop-entry = "0.7"

# Plugin loading
libloading = "0.8"
semver = "1"

# Data & config
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"

# Logging & notifications
log = "0.4"
env_logger = "0.11"
notify-rust = "4"

# Optional: embedded Lua runtime
mlua = { version = "0.10", features = ["luajit", "vendored", "serialize"], optional = true }
meval = { version = "0.2", optional = true }
reqwest = { version = "0.12", features = ["blocking", "json"], optional = true }

[features]
default = []
lua = ["mlua", "meval", "reqwest"]
dev-logging = []
```

Note: Exact dependency versions should match what was updated in Task 1. The versions shown here are the current ones — adjust to whatever Task 1 settled on.

- [ ] **Step 2: Create placeholder `crates/owlry-core/src/lib.rs`**

```rust
pub mod config;
pub mod data;
pub mod filter;
pub mod notify;
pub mod paths;
pub mod plugins;
pub mod providers;
```

- [ ] **Step 3: Add `owlry-core` to workspace members**

In root `Cargo.toml`, add `"crates/owlry-core"` to the `members` list.

- [ ] **Step 4: Verify workspace resolves**

Run: `cargo check -p owlry-core`

Expected: Fails with missing modules (expected — we haven't moved files yet).

- [ ] **Step 5: Commit scaffold**

```bash
git add crates/owlry-core/Cargo.toml crates/owlry-core/src/lib.rs Cargo.toml
git commit -m "feat(owlry-core): scaffold new core crate"
```

---

### Task 3: Move backend modules from `owlry` to `owlry-core`

**Files:**
- Move: `crates/owlry/src/config/` → `crates/owlry-core/src/config/`
- Move: `crates/owlry/src/data/` → `crates/owlry-core/src/data/`
- Move: `crates/owlry/src/filter.rs` → `crates/owlry-core/src/filter.rs`
- Move: `crates/owlry/src/providers/mod.rs` → `crates/owlry-core/src/providers/mod.rs`
- Move: `crates/owlry/src/providers/application.rs` → `crates/owlry-core/src/providers/application.rs`
- Move: `crates/owlry/src/providers/command.rs` → `crates/owlry-core/src/providers/command.rs`
- Move: `crates/owlry/src/providers/native_provider.rs` → `crates/owlry-core/src/providers/native_provider.rs`
- Move: `crates/owlry/src/providers/lua_provider.rs` → `crates/owlry-core/src/providers/lua_provider.rs`
- Move: `crates/owlry/src/plugins/` → `crates/owlry-core/src/plugins/`
- Move: `crates/owlry/src/notify.rs` → `crates/owlry-core/src/notify.rs`
- Move: `crates/owlry/src/paths.rs` → `crates/owlry-core/src/paths.rs`
- Keep in `crates/owlry/src/`: `main.rs`, `app.rs`, `cli.rs`, `theme.rs`, `ui/`, `providers/dmenu.rs`

- [ ] **Step 1: Move all backend modules**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry

# Config, data, filter, notify, paths
cp -r crates/owlry/src/config crates/owlry-core/src/
cp -r crates/owlry/src/data crates/owlry-core/src/
cp crates/owlry/src/filter.rs crates/owlry-core/src/
cp crates/owlry/src/notify.rs crates/owlry-core/src/
cp crates/owlry/src/paths.rs crates/owlry-core/src/

# Providers (except dmenu.rs — stays in owlry)
mkdir -p crates/owlry-core/src/providers
cp crates/owlry/src/providers/mod.rs crates/owlry-core/src/providers/
cp crates/owlry/src/providers/application.rs crates/owlry-core/src/providers/
cp crates/owlry/src/providers/command.rs crates/owlry-core/src/providers/
cp crates/owlry/src/providers/native_provider.rs crates/owlry-core/src/providers/
cp crates/owlry/src/providers/lua_provider.rs crates/owlry-core/src/providers/

# Plugins (entire directory)
cp -r crates/owlry/src/plugins crates/owlry-core/src/
```

- [ ] **Step 2: Verify `owlry-core` compiles as library**

Run: `cargo check -p owlry-core`

Fix any import path issues. The moved files may reference `crate::` paths that need updating. Common fixes:
- `crate::config::Config` stays as-is (now within owlry-core)
- `crate::paths::` stays as-is
- `crate::providers::` stays as-is
- `crate::data::` stays as-is

- [ ] **Step 3: Commit moved files (copy phase)**

```bash
git add crates/owlry-core/src/
git commit -m "feat(owlry-core): move backend modules from owlry"
```

---

### Task 4: Wire `owlry` to depend on `owlry-core` as library

**Files:**
- Modify: `crates/owlry/Cargo.toml` — add owlry-core dep, remove moved deps
- Modify: `crates/owlry/src/app.rs` — import from owlry_core instead of local modules
- Modify: `crates/owlry/src/main.rs` — import from owlry_core
- Modify: `crates/owlry/src/theme.rs` — import config types from owlry_core
- Modify: `crates/owlry/src/ui/main_window.rs` — import from owlry_core
- Modify: `crates/owlry/src/ui/result_row.rs` — import from owlry_core
- Modify: `crates/owlry/src/ui/submenu.rs` — import from owlry_core (if needed)
- Delete from `crates/owlry/src/`: `config/`, `data/`, `filter.rs`, `notify.rs`, `paths.rs`, `plugins/`, `providers/mod.rs`, `providers/application.rs`, `providers/command.rs`, `providers/native_provider.rs`, `providers/lua_provider.rs`
- Keep: `crates/owlry/src/providers/dmenu.rs` (restructure into standalone module)

- [ ] **Step 1: Add `owlry-core` dependency to `owlry`**

In `crates/owlry/Cargo.toml`, add:
```toml
owlry-core = { path = "../owlry-core" }
```

Remove dependencies that are now only used by owlry-core (keep only what owlry itself uses directly):
- Keep: `gtk4`, `gtk4-layer-shell`, `clap`, `log`, `env_logger`, `serde`, `toml`, `dirs`
- Remove: `fuzzy-matcher`, `freedesktop-desktop-entry`, `libloading`, `semver`, `notify-rust`
- Remove optional lua-related deps if they moved entirely to owlry-core

- [ ] **Step 2: Update all `use` statements in owlry source files**

Replace local module imports with owlry-core imports throughout `crates/owlry/src/`:

```rust
// Before (local):
use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::{ProviderFilter, ParsedQuery};
use crate::providers::{ProviderManager, LaunchItem, ProviderType, Provider};
use crate::plugins::native_loader::NativePluginLoader;
use crate::notify;
use crate::paths;

// After (from owlry-core):
use owlry_core::config::Config;
use owlry_core::data::FrecencyStore;
use owlry_core::filter::{ProviderFilter, ParsedQuery};
use owlry_core::providers::{ProviderManager, LaunchItem, ProviderType, Provider};
use owlry_core::plugins::native_loader::NativePluginLoader;
use owlry_core::notify;
use owlry_core::paths;
```

Apply this across: `app.rs`, `main.rs`, `theme.rs`, `ui/main_window.rs`, `ui/result_row.rs`, `ui/submenu.rs`.

- [ ] **Step 3: Restructure dmenu as standalone provider in owlry**

`crates/owlry/src/providers/dmenu.rs` stays but needs its own module root. Create `crates/owlry/src/providers/mod.rs` that only contains the dmenu re-export:

```rust
pub mod dmenu;
pub use dmenu::DmenuProvider;
```

The `DmenuProvider` imports `LaunchItem` and `Provider` from `owlry_core::providers`.

- [ ] **Step 4: Delete moved source files from owlry**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
rm -rf crates/owlry/src/config
rm -rf crates/owlry/src/data
rm -rf crates/owlry/src/plugins
rm crates/owlry/src/filter.rs
rm crates/owlry/src/notify.rs
rm crates/owlry/src/paths.rs
rm crates/owlry/src/providers/application.rs
rm crates/owlry/src/providers/command.rs
rm crates/owlry/src/providers/native_provider.rs
rm crates/owlry/src/providers/lua_provider.rs
```

- [ ] **Step 5: Build and verify the full workspace**

Run: `cargo build --workspace`
Run: `cargo test --workspace`

The launcher should work exactly as before — same binary, just using owlry-core as a library internally.

- [ ] **Step 6: Smoke test**

Run: `cargo run -p owlry -- -m dmenu <<< $'option1\noption2\noption3'`

Verify dmenu mode works. Then run the full launcher if possible:

Run: `cargo run -p owlry`

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: wire owlry to use owlry-core as library dependency"
```

---

### Task 5: Update justfile for owlry-core

**Files:**
- Modify: `justfile`

- [ ] **Step 1: Add owlry-core build targets**

Add these targets to the justfile:

```just
# Build core daemon only
build-daemon:
    cargo build -p owlry-core

# Build core daemon release
release-daemon:
    cargo build -p owlry-core --release

# Run core daemon
run-daemon *ARGS:
    cargo run -p owlry-core -- {{ARGS}}
```

- [ ] **Step 2: Update existing targets**

Update `build-core` to clarify it builds the UI binary:

```just
# Build UI binary only
build-ui:
    cargo build -p owlry
```

Update `install-local` to install both binaries:

```just
install-local:
    cargo build -p owlry --release
    cargo build -p owlry-core --release
    sudo install -Dm755 target/release/owlry /usr/bin/owlry
    sudo install -Dm755 target/release/owlry-core /usr/bin/owlry-core
    # ... rest of plugin install unchanged
```

- [ ] **Step 3: Add bump target for owlry-core**

Add `bump-core` and update `bump-all` to include owlry-core.

- [ ] **Step 4: Verify justfile**

Run: `just build`
Run: `just check`

- [ ] **Step 5: Commit**

```bash
git add justfile
git commit -m "chore: update justfile for owlry-core crate"
```

---

## Phase 2: Add IPC Layer

### Task 6: Define shared IPC message types

**Files:**
- Create: `crates/owlry-core/src/ipc.rs`
- Modify: `crates/owlry-core/src/lib.rs` — add `pub mod ipc;`

- [ ] **Step 1: Write tests for IPC message serialization**

Create `crates/owlry-core/tests/ipc_test.rs`:

```rust
use owlry_core::ipc::{Request, Response, ResultItem, ProviderDesc};

#[test]
fn test_query_request_roundtrip() {
    let req = Request::Query {
        text: "fire".into(),
        modes: Some(vec!["app".into(), "cmd".into()]),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_query_request_without_modes() {
    let req = Request::Query {
        text: "fire".into(),
        modes: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(!json.contains("modes"));
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_launch_request_roundtrip() {
    let req = Request::Launch {
        item_id: "firefox.desktop".into(),
        provider: "app".into(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_results_response_roundtrip() {
    let resp = Response::Results {
        items: vec![ResultItem {
            id: "firefox.desktop".into(),
            title: "Firefox".into(),
            description: "Web Browser".into(),
            icon: "firefox".into(),
            provider: "app".into(),
            score: 95,
            command: Some("firefox".into()),
            tags: vec![],
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, parsed);
}

#[test]
fn test_providers_response() {
    let resp = Response::Providers {
        list: vec![ProviderDesc {
            id: "app".into(),
            name: "Applications".into(),
            prefix: Some(":app".into()),
            icon: "application-x-executable".into(),
            position: "normal".into(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, parsed);
}

#[test]
fn test_error_response() {
    let resp = Response::Error {
        message: "plugin not found".into(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, parsed);
}

#[test]
fn test_toggle_request() {
    let req = Request::Toggle;
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p owlry-core --test ipc_test`

Expected: compilation error — `ipc` module doesn't exist yet.

- [ ] **Step 3: Implement IPC message types**

Create `crates/owlry-core/src/ipc.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Query {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        modes: Option<Vec<String>>,
    },
    Launch {
        item_id: String,
        provider: String,
    },
    Providers,
    Refresh {
        provider: String,
    },
    Toggle,
    Submenu {
        plugin_id: String,
        data: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Results {
        items: Vec<ResultItem>,
    },
    Providers {
        list: Vec<ProviderDesc>,
    },
    SubmenuItems {
        items: Vec<ResultItem>,
    },
    Ack,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub provider: String,
    pub score: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDesc {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub icon: String,
    pub position: String,
}
```

Add `pub mod ipc;` to `crates/owlry-core/src/lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p owlry-core --test ipc_test`

Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/ipc.rs crates/owlry-core/src/lib.rs crates/owlry-core/tests/
git commit -m "feat(owlry-core): define IPC message types with serde"
```

---

### Task 7: Implement IPC server in `owlry-core`

**Files:**
- Create: `crates/owlry-core/src/server.rs`
- Modify: `crates/owlry-core/src/lib.rs` — add `pub mod server;`
- Modify: `crates/owlry-core/Cargo.toml` — may need to add deps

- [ ] **Step 1: Write test for server socket creation and message handling**

Create `crates/owlry-core/tests/server_test.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use owlry_core::ipc::{Request, Response};
use owlry_core::server::Server;

fn temp_socket_path() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("owlry-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test.sock")
}

#[test]
fn test_server_accepts_connection_and_responds_to_providers() {
    let sock = temp_socket_path();
    let sock2 = sock.clone();

    let handle = thread::spawn(move || {
        let server = Server::new(&sock2).unwrap();
        // Accept one connection, handle one request, then stop
        server.handle_one_for_testing().unwrap();
    });

    // Give server time to bind
    thread::sleep(Duration::from_millis(100));

    let mut stream = UnixStream::connect(&sock).unwrap();
    let req = serde_json::to_string(&Request::Providers).unwrap();
    writeln!(stream, "{}", req).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: Response = serde_json::from_str(line.trim()).unwrap();

    match resp {
        Response::Providers { list } => {
            // Server without plugins returns at least empty list
            assert!(list.is_empty() || !list.is_empty());
        }
        other => panic!("Expected Providers response, got {:?}", other),
    }

    handle.join().unwrap();
    std::fs::remove_dir_all(sock.parent().unwrap()).ok();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p owlry-core --test server_test`

Expected: compilation error — `server` module doesn't exist.

- [ ] **Step 3: Implement the IPC server**

Create `crates/owlry-core/src/server.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use log::{error, info, warn};

use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::ipc::{Request, Response, ResultItem, ProviderDesc};
use crate::providers::{LaunchItem, ProviderManager, ProviderType};

pub struct Server {
    listener: UnixListener,
    socket_path: PathBuf,
    provider_manager: Arc<Mutex<ProviderManager>>,
    frecency: Arc<Mutex<FrecencyStore>>,
    config: Arc<Config>,
}

impl Server {
    pub fn new(socket_path: &Path) -> std::io::Result<Self> {
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        let config = Config::load_or_default();

        let provider_manager = ProviderManager::new(&config);
        let frecency = FrecencyStore::new();

        info!("owlry-core listening on {}", socket_path.display());

        Ok(Self {
            listener,
            socket_path: socket_path.to_path_buf(),
            provider_manager: Arc::new(Mutex::new(provider_manager)),
            frecency: Arc::new(Mutex::new(frecency)),
            config: Arc::new(config),
        })
    }

    pub fn run(&self) -> std::io::Result<()> {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let pm = Arc::clone(&self.provider_manager);
                    let frec = Arc::clone(&self.frecency);
                    let config = Arc::clone(&self.config);
                    std::thread::spawn(move || {
                        if let Err(e) = Self::handle_client(stream, &pm, &frec, &config) {
                            warn!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
        Ok(())
    }

    fn handle_client(
        stream: UnixStream,
        provider_manager: &Arc<Mutex<ProviderManager>>,
        frecency: &Arc<Mutex<FrecencyStore>>,
        config: &Arc<Config>,
    ) -> std::io::Result<()> {
        let reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let request: Request = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let resp = Response::Error {
                        message: format!("Invalid request: {}", e),
                    };
                    Self::send_response(&mut writer, &resp)?;
                    continue;
                }
            };

            let response = Self::handle_request(request, provider_manager, frecency, config);
            Self::send_response(&mut writer, &response)?;
        }
        Ok(())
    }

    fn handle_request(
        request: Request,
        provider_manager: &Arc<Mutex<ProviderManager>>,
        frecency: &Arc<Mutex<FrecencyStore>>,
        config: &Arc<Config>,
    ) -> Response {
        match request {
            Request::Query { text, modes } => {
                let pm = provider_manager.lock().unwrap();
                let frec = frecency.lock().unwrap();

                let filter = match &modes {
                    Some(m) => ProviderFilter::from_mode_strings(m),
                    None => ProviderFilter::all(),
                };

                let max_results = config.general.max_results.unwrap_or(50) as usize;
                let results = pm.search_with_frecency(
                    &text,
                    max_results,
                    &filter,
                    &frec,
                    config.general.frecency_weight.unwrap_or(0.3),
                    None,
                );

                Response::Results {
                    items: results
                        .into_iter()
                        .map(|(item, score)| ResultItem {
                            id: item.id.clone(),
                            title: item.name.clone(),
                            description: item.description.clone().unwrap_or_default(),
                            icon: item.icon.clone().unwrap_or_default(),
                            provider: format!("{:?}", item.provider),
                            score,
                            command: item.command.clone(),
                            tags: item.tags.clone(),
                        })
                        .collect(),
                }
            }
            Request::Launch { item_id, provider: _ } => {
                let mut frec = frecency.lock().unwrap();
                frec.record_launch(&item_id);
                Response::Ack
            }
            Request::Providers => {
                let pm = provider_manager.lock().unwrap();
                let providers = pm.available_providers();
                Response::Providers {
                    list: providers
                        .into_iter()
                        .map(|p| ProviderDesc {
                            id: p.id,
                            name: p.name,
                            prefix: p.prefix,
                            icon: p.icon,
                            position: p.position,
                        })
                        .collect(),
                }
            }
            Request::Refresh { provider } => {
                let mut pm = provider_manager.lock().unwrap();
                pm.refresh_provider(&provider);
                Response::Ack
            }
            Request::Toggle => {
                // Toggle is handled by the UI client — daemon just acknowledges
                Response::Ack
            }
            Request::Submenu { plugin_id, data } => {
                let pm = provider_manager.lock().unwrap();
                match pm.query_submenu_actions(&plugin_id, &data, "") {
                    Some((_title, items)) => Response::SubmenuItems {
                        items: items
                            .into_iter()
                            .map(|item| ResultItem {
                                id: item.id.clone(),
                                title: item.name.clone(),
                                description: item.description.clone().unwrap_or_default(),
                                icon: item.icon.clone().unwrap_or_default(),
                                provider: format!("{:?}", item.provider),
                                score: 0,
                                command: item.command.clone(),
                                tags: item.tags.clone(),
                            })
                            .collect(),
                    },
                    None => Response::Error {
                        message: format!("No submenu for plugin '{}'", plugin_id),
                    },
                }
            }
        }
    }

    fn send_response(writer: &mut UnixStream, response: &Response) -> std::io::Result<()> {
        let json = serde_json::to_string(response).unwrap();
        writeln!(writer, "{}", json)?;
        writer.flush()
    }

    /// For testing: accept one connection, handle one request, then return
    pub fn handle_one_for_testing(&self) -> std::io::Result<()> {
        let (stream, _) = self.listener.accept()?;
        let reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;

        if let Some(Ok(line)) = reader.lines().next() {
            if let Ok(request) = serde_json::from_str::<Request>(&line) {
                let response = Self::handle_request(
                    request,
                    &self.provider_manager,
                    &self.frecency,
                    &self.config,
                );
                Self::send_response(&mut writer, &response)?;
            }
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        std::fs::remove_file(&self.socket_path).ok();
    }
}
```

**Dependency:** This task depends on Task 8 (daemon-friendly API surface). Implement Task 8 first, then come back to this task. The server calls `ProviderManager::new()`, `ProviderFilter::from_mode_strings()`, `pm.available_providers()`, and `pm.refresh_provider()` — all defined in Task 8.

Add `pub mod server;` to `crates/owlry-core/src/lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p owlry-core --test server_test`

Fix any compilation issues. The test may need adjustments based on `ProviderManager::new()` signature.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/server.rs crates/owlry-core/src/lib.rs crates/owlry-core/tests/
git commit -m "feat(owlry-core): implement IPC server over Unix socket"
```

---

### Task 8: Add daemon-friendly API surface to ProviderManager

**Files:**
- Modify: `crates/owlry-core/src/providers/mod.rs` — add methods needed by server
- Modify: `crates/owlry-core/src/filter.rs` — add `from_mode_strings()` constructor

The server (Task 7) needs several methods that the current `ProviderManager` doesn't have. This task adds them.

- [ ] **Step 1: Add `ProviderFilter::from_mode_strings`**

In `crates/owlry-core/src/filter.rs`, add:

```rust
impl ProviderFilter {
    /// Create a filter from a list of mode name strings (e.g., ["app", "cmd", "calc"])
    pub fn from_mode_strings(modes: &[String]) -> Self {
        let mut filter = Self::none();
        for mode in modes {
            if let Some(pt) = Self::provider_type_from_str(mode) {
                filter.enable(pt);
            }
        }
        filter
    }

    /// Create a filter that accepts all providers
    pub fn all() -> Self {
        Self::new(None, None, None)
    }

    /// Create a filter that accepts no providers (empty enabled set)
    fn none() -> Self {
        // Start with default filter, then clear all enabled providers.
        // Exact implementation depends on ProviderFilter internals —
        // if it uses a HashSet<ProviderType>, start with an empty set.
        // If it uses bool flags, set all to false.
        let mut f = Self::default();
        // Clear all enabled providers
        f
    }

    fn provider_type_from_str(s: &str) -> Option<ProviderType> {
        match s {
            "app" => Some(ProviderType::Application),
            "cmd" => Some(ProviderType::Command),
            "dmenu" => Some(ProviderType::Dmenu),
            other => Some(ProviderType::Plugin(other.to_string())),
        }
    }
}
```

Exact implementation depends on current `ProviderFilter` internals — adapt to match.

- [ ] **Step 2: Add `ProviderManager::new(&Config)` constructor**

Currently `ProviderManager` is created via `with_native_plugins()`. Add a constructor that initializes from config, loads plugins, and sets up providers:

```rust
impl ProviderManager {
    pub fn new(config: &Config) -> Self {
        // Load native plugins — move the logic from OwlryApp::load_native_plugins()
        // in app.rs into this method. It currently:
        // 1. Creates NativePluginLoader::new()
        // 2. Calls loader.discover()
        // 3. Iterates loader.into_plugins() creating NativeProvider instances
        // 4. Filters by config.plugins.disabled_plugins if set
        let native_providers = Self::load_native_plugins(config);
        let mut pm = Self::with_native_plugins(native_providers);
        pm.refresh_all();
        pm
    }

    fn load_native_plugins(config: &Config) -> Vec<NativeProvider> {
        use crate::plugins::native_loader::NativePluginLoader;
        let mut loader = NativePluginLoader::new();
        if let Err(e) = loader.discover() {
            log::warn!("Plugin discovery error: {}", e);
        }
        let plugins = loader.into_plugins();
        let mut providers = Vec::new();
        for plugin in plugins {
            for info in (plugin.vtable().providers)() {
                let provider = NativeProvider::new(plugin.clone(), info);
                // Skip disabled plugins
                if let Some(disabled) = config.plugins.as_ref().and_then(|p| p.disabled_plugins.as_ref()) {
                    if disabled.contains(&provider.type_id().to_string()) {
                        continue;
                    }
                }
                providers.push(provider);
            }
        }
        providers
    }
}
```

- [ ] **Step 3: Add `available_providers()` and `refresh_provider()` methods**

```rust
impl ProviderManager {
    pub fn available_providers(&self) -> Vec<ProviderDescriptor> {
        // Iterate over core providers + native providers and collect metadata.
        // Core providers: Application, Command (always present).
        // Native providers: iterate self.native_providers (or equivalent field).
        let mut descs = vec![
            ProviderDescriptor {
                id: "app".into(),
                name: "Applications".into(),
                prefix: Some(":app".into()),
                icon: "application-x-executable".into(),
                position: "normal".into(),
            },
            ProviderDescriptor {
                id: "cmd".into(),
                name: "Commands".into(),
                prefix: Some(":cmd".into()),
                icon: "utilities-terminal".into(),
                position: "normal".into(),
            },
        ];
        // Add native plugin providers from self's internal list
        // Map NativeProvider info to ProviderDescriptor
        descs
    }

    pub fn refresh_provider(&mut self, provider_id: &str) {
        // Find the provider matching provider_id and call its refresh() method.
        // For core providers, call the Provider trait's refresh().
        // For native providers, find by type_id and call refresh().
    }
}

pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub icon: String,
    pub position: String,
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo check -p owlry-core`
Run: `cargo test -p owlry-core`

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/providers/mod.rs crates/owlry-core/src/filter.rs
git commit -m "feat(owlry-core): add daemon-friendly API to ProviderManager and ProviderFilter"
```

---

### Task 9: Create daemon binary entry point

**Files:**
- Create: `crates/owlry-core/src/main.rs`
- Modify: `crates/owlry-core/Cargo.toml` — add `[[bin]]` target

- [ ] **Step 1: Add binary target to Cargo.toml**

In `crates/owlry-core/Cargo.toml`, add:

```toml
[[bin]]
name = "owlry-core"
path = "src/main.rs"
```

- [ ] **Step 2: Implement daemon entry point**

Create `crates/owlry-core/src/main.rs`:

```rust
use std::path::PathBuf;

use log::info;

use owlry_core::paths;
use owlry_core::server::Server;

fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    runtime_dir.join("owlry").join("owlry.sock")
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let sock = socket_path();
    info!("Starting owlry-core daemon...");

    let server = match Server::new(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start owlry-core: {}", e);
            std::process::exit(1);
        }
    };

    // Handle SIGTERM for graceful shutdown
    let sock_cleanup = sock.clone();
    ctrlc::handle(move || {
        std::fs::remove_file(&sock_cleanup).ok();
        std::process::exit(0);
    })
    .ok();

    if let Err(e) = server.run() {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}
```

Note: May need `ctrlc` crate or use `signal_hook` for signal handling. Alternatively, handle cleanup in `Drop`.

- [ ] **Step 3: Add signal handling dependency if needed**

If using `ctrlc` crate, add to `crates/owlry-core/Cargo.toml`:

```toml
ctrlc = "3"
```

Or use standard library signal handling with `libc`.

- [ ] **Step 4: Build daemon binary**

Run: `cargo build -p owlry-core`
Run: `./target/debug/owlry-core &` (verify it starts and creates socket)
Run: `ls $XDG_RUNTIME_DIR/owlry/owlry.sock` (verify socket exists)
Run: `kill %1` (stop daemon)

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/main.rs crates/owlry-core/Cargo.toml
git commit -m "feat(owlry-core): add daemon binary entry point"
```

---

### Task 10: Implement IPC client in `owlry`

**Files:**
- Create: `crates/owlry/src/client.rs`

- [ ] **Step 1: Write client tests**

Create `crates/owlry/tests/client_test.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use owlry::client::CoreClient;

fn temp_socket_path() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("owlry-client-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test.sock")
}

#[test]
fn test_client_connects_and_queries() {
    let sock = temp_socket_path();
    let sock2 = sock.clone();

    // Mock server: accept connection, read query, respond with results
    let handle = thread::spawn(move || {
        let listener = UnixListener::bind(&sock2).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        for line in reader.lines() {
            let line = line.unwrap();
            // Respond with empty results
            let resp = r#"{"type":"results","items":[]}"#;
            writeln!(writer, "{}", resp).unwrap();
            writer.flush().unwrap();
            break;
        }
    });

    thread::sleep(Duration::from_millis(100));

    let client = CoreClient::connect(&sock).unwrap();
    let results = client.query("test", None).unwrap();
    assert!(results.is_empty());

    handle.join().unwrap();
    std::fs::remove_dir_all(sock.parent().unwrap()).ok();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p owlry --test client_test`

Expected: compilation error.

- [ ] **Step 3: Implement the IPC client**

Create `crates/owlry/src/client.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use owlry_core::ipc::{ProviderDesc, Request, Response, ResultItem};

pub struct CoreClient {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl CoreClient {
    pub fn connect(socket_path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    pub fn connect_or_start() -> std::io::Result<Self> {
        let sock = Self::socket_path();

        // Try direct connection first
        if let Ok(client) = Self::connect(&sock) {
            return Ok(client);
        }

        // Try starting via systemd
        let _ = Command::new("systemctl")
            .args(["--user", "start", "owlry-core"])
            .status();

        // Retry with backoff
        for delay_ms in [100, 200, 400] {
            std::thread::sleep(Duration::from_millis(delay_ms));
            if let Ok(client) = Self::connect(&sock) {
                return Ok(client);
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Could not connect to owlry-core daemon. Is it running?",
        ))
    }

    pub fn socket_path() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        runtime_dir.join("owlry").join("owlry.sock")
    }

    pub fn query(&mut self, text: &str, modes: Option<Vec<String>>) -> std::io::Result<Vec<ResultItem>> {
        let request = Request::Query {
            text: text.into(),
            modes,
        };
        self.send(&request)?;

        match self.receive()? {
            Response::Results { items } => Ok(items),
            Response::Error { message } => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                message,
            )),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unexpected response: {:?}", other),
            )),
        }
    }

    pub fn launch(&mut self, item_id: &str, provider: &str) -> std::io::Result<()> {
        let request = Request::Launch {
            item_id: item_id.into(),
            provider: provider.into(),
        };
        self.send(&request)?;
        self.receive()?;
        Ok(())
    }

    pub fn providers(&mut self) -> std::io::Result<Vec<ProviderDesc>> {
        self.send(&Request::Providers)?;
        match self.receive()? {
            Response::Providers { list } => Ok(list),
            Response::Error { message } => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                message,
            )),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            )),
        }
    }

    pub fn toggle(&mut self) -> std::io::Result<()> {
        self.send(&Request::Toggle)?;
        self.receive()?;
        Ok(())
    }

    pub fn submenu(&mut self, plugin_id: &str, data: &str) -> std::io::Result<Vec<ResultItem>> {
        let request = Request::Submenu {
            plugin_id: plugin_id.into(),
            data: data.into(),
        };
        self.send(&request)?;
        match self.receive()? {
            Response::SubmenuItems { items } => Ok(items),
            Response::Error { message } => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                message,
            )),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            )),
        }
    }

    fn send(&mut self, request: &Request) -> std::io::Result<()> {
        let json = serde_json::to_string(request).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()
    }

    fn receive(&mut self) -> std::io::Result<Response> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        serde_json::from_str(line.trim()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p owlry --test client_test`

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry/src/client.rs crates/owlry/tests/
git commit -m "feat(owlry): implement IPC client for daemon communication"
```

---

### Task 11: Wire UI to use IPC client

**Files:**
- Modify: `crates/owlry/src/app.rs` — replace direct plugin/provider calls with client
- Modify: `crates/owlry/src/ui/main_window.rs` — use client for search/launch
- Modify: `crates/owlry/src/main.rs` — add client initialization

This is the largest single task. The UI currently calls `ProviderManager` directly. It needs to go through the IPC client instead, except in dmenu mode.

- [ ] **Step 1: Refactor `app.rs` — remove direct plugin loading**

In `on_activate()`, replace:
- `load_native_plugins()` call → removed (daemon loads plugins)
- `ProviderManager::with_native_plugins()` → `CoreClient::connect_or_start()`
- `load_lua_plugins()` → removed (daemon loads)

The app creates a `CoreClient` and passes it to `MainWindow`.

- [ ] **Step 2: Refactor `MainWindow` to accept `CoreClient`**

Replace `ProviderManager` and `FrecencyStore` fields with `CoreClient`:
- Search handler: calls `client.query(text, modes)` instead of `pm.search_with_frecency()`
- Launch handler: calls `client.launch(id, provider)` instead of direct command execution
- Provider tabs: calls `client.providers()` on init
- Submenu: calls `client.submenu(plugin_id, data)`

The `CoreClient` is wrapped in `Rc<RefCell<CoreClient>>` for GTK signal handlers.

- [ ] **Step 3: Keep dmenu mode as direct (no client)**

In `app.rs`, if `args.mode` contains only `dmenu`:
- Skip client connection entirely
- Use `DmenuProvider` directly as before
- This path doesn't change

- [ ] **Step 4: Build and test**

Run: `cargo build -p owlry`

Start daemon in one terminal: `cargo run -p owlry-core`
Test UI in another: `cargo run -p owlry`
Test dmenu: `echo "a\nb\nc" | cargo run -p owlry -- -m dmenu`

- [ ] **Step 5: Commit**

```bash
git add crates/owlry/src/
git commit -m "refactor(owlry): wire UI to use IPC client instead of direct provider calls"
```

---

### Task 12: Implement toggle behavior

**Files:**
- Modify: `crates/owlry/src/main.rs` — check for existing instance
- Modify: `crates/owlry-core/src/server.rs` — track client visibility state

- [ ] **Step 1: Add instance detection**

In `owlry` `main.rs`, before creating the GTK Application:
1. Try connecting to daemon
2. If another UI instance is already connected, send `Toggle` → daemon tells the other instance to close
3. If no other instance, proceed normally

Implementation options:
- Lock file at `$XDG_RUNTIME_DIR/owlry/owlry-ui.lock`
- Or: daemon tracks connected UI clients and responds to Toggle with visibility state

- [ ] **Step 2: Implement using lock file approach**

```rust
use std::fs;
use std::os::unix::fs::OpenOptionsExt;

fn try_acquire_lock() -> Option<fs::File> {
    let lock_path = CoreClient::socket_path()
        .parent()
        .unwrap()
        .join("owlry-ui.lock");

    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&lock_path)
        .ok()
        .and_then(|f| {
            use std::os::unix::io::AsRawFd;
            let fd = f.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret == 0 { Some(f) } else { None }
        })
}
```

If lock acquisition fails, another instance is running → send toggle signal and exit.

- [ ] **Step 3: Test toggle**

Run daemon: `cargo run -p owlry-core &`
Run UI: `cargo run -p owlry &`
Run UI again: `cargo run -p owlry` → should close the first instance

- [ ] **Step 4: Commit**

```bash
git add crates/owlry/src/main.rs
git commit -m "feat(owlry): implement toggle behavior for repeated invocations"
```

---

### Task 13: Add config profiles and update CLI

**Files:**
- Modify: `crates/owlry/src/cli.rs` — remove `--providers`, add `--profile`
- Modify: `crates/owlry-core/src/config/mod.rs` — add `ProfileConfig`

- [ ] **Step 1: Add profile config structure**

In `crates/owlry-core/src/config/mod.rs`, add:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProfileConfig {
    pub modes: Vec<String>,
}

// Add to Config struct:
// pub profiles: HashMap<String, ProfileConfig>,
```

- [ ] **Step 2: Update CLI args**

In `crates/owlry/src/cli.rs`:
- Remove `--providers` / `-p` flag
- Add `--profile` flag: `Option<String>`
- Keep `--mode` / `-m` flag
- Repurpose `-p` as short for `--prompt` (dmenu prompt text)

```rust
#[derive(Debug)]
pub struct CliArgs {
    pub mode: Option<Vec<String>>,
    pub profile: Option<String>,
    pub prompt: Option<String>,  // was --prompt, now also -p
    pub command: Option<Command>,
}
```

- [ ] **Step 3: Resolve profile to modes**

In `app.rs` or `main.rs`, resolve the profile:

```rust
fn resolve_modes(args: &CliArgs, config: &Config) -> Option<Vec<String>> {
    if let Some(modes) = &args.mode {
        return Some(modes.clone());
    }
    if let Some(profile_name) = &args.profile {
        if let Some(profile) = config.profiles.get(profile_name) {
            return Some(profile.modes.clone());
        }
        eprintln!("Unknown profile: {}", profile_name);
        std::process::exit(1);
    }
    None // All providers
}
```

- [ ] **Step 4: Test profile resolution**

Add config with profiles, verify `owlry --profile default` uses correct modes.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry/src/cli.rs crates/owlry-core/src/config/mod.rs crates/owlry/src/app.rs
git commit -m "feat: add config profiles, remove --providers flag"
```

---

### Task 14: Create systemd service files

**Files:**
- Create: `systemd/owlry-core.service`
- Create: `systemd/owlry-core.socket`

- [ ] **Step 1: Create service unit**

Create `systemd/owlry-core.service`:

```ini
[Unit]
Description=Owlry application launcher daemon
Documentation=https://somegit.dev/Owlibou/owlry
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/owlry-core
Restart=on-failure
RestartSec=3
Environment=RUST_LOG=warn

[Install]
WantedBy=default.target
```

- [ ] **Step 2: Create socket unit**

Create `systemd/owlry-core.socket`:

```ini
[Unit]
Description=Owlry launcher socket

[Socket]
ListenStream=%t/owlry/owlry.sock
DirectoryMode=0700

[Install]
WantedBy=sockets.target
```

- [ ] **Step 3: Commit**

```bash
git add systemd/
git commit -m "feat: add systemd user service and socket units for owlry-core"
```

---

### Task 15: Update README and justfile for Phase 2

**Files:**
- Modify: `README.md`
- Modify: `justfile`

- [ ] **Step 1: Update README**

Rewrite README to reflect the new architecture:
- Explain client/daemon split
- Update installation instructions (install both binaries)
- Update usage examples (daemon startup, UI invocation, profiles)
- Update dmenu section (unchanged behavior, but clarify it bypasses daemon)
- Remove `--providers` references
- Add systemd setup instructions
- Add profile configuration examples
- Keep plugin usage section (will be updated in Phase 3)

- [ ] **Step 2: Update justfile**

Ensure all build/run/install/release targets reflect two binaries. Key changes:
- `just run` → runs the UI
- `just run-daemon` → runs the daemon
- `just install-local` → installs both binaries + systemd units
- `just release` → builds both in release mode
- Bump targets updated for owlry-core

- [ ] **Step 3: Verify**

Run: `just build`
Run: `just check`

- [ ] **Step 4: Commit**

```bash
git add README.md justfile
git commit -m "docs: update README and justfile for client/daemon architecture"
```

---

## Phase 3: Split Repos

### Task 16: Create `owlry-plugins` workspace

**Files:**
- Create: `owlry-plugins/Cargo.toml`
- Create: `owlry-plugins/justfile`
- Create: `owlry-plugins/README.md`
- Create: `owlry-plugins/.gitignore`

- [ ] **Step 1: Create plugins workspace directory**

```bash
mkdir -p /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
```

- [ ] **Step 2: Initialize git repo**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git init
git remote add origin gitea@somegit.dev:Owlibou/owlry-plugins.git
```

- [ ] **Step 3: Create workspace Cargo.toml**

Create `owlry-plugins/Cargo.toml`:

```toml
[workspace]
members = [
    "crates/owlry-plugin-calculator",
    "crates/owlry-plugin-clipboard",
    "crates/owlry-plugin-emoji",
    "crates/owlry-plugin-bookmarks",
    "crates/owlry-plugin-ssh",
    "crates/owlry-plugin-scripts",
    "crates/owlry-plugin-system",
    "crates/owlry-plugin-websearch",
    "crates/owlry-plugin-filesearch",
    "crates/owlry-plugin-weather",
    "crates/owlry-plugin-media",
    "crates/owlry-plugin-pomodoro",
    "crates/owlry-plugin-systemd",
    "crates/owlry-lua",
    "crates/owlry-rune",
]
resolver = "2"

[workspace.package]
edition = "2024"
rust-version = "1.90"
license = "GPL-3.0-or-later"
repository = "https://somegit.dev/Owlibou/owlry-plugins"

[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
opt-level = "z"
```

- [ ] **Step 4: Create .gitignore**

Create `owlry-plugins/.gitignore`:

```
/target
**/*.rs.bk
Cargo.lock
```

Note: `Cargo.lock` is not committed for library/plugin workspaces — only for binaries.

- [ ] **Step 5: Create README.md**

Create `owlry-plugins/README.md` with plugin listing, build instructions, and link to plugin development docs.

- [ ] **Step 6: Create justfile**

Create `owlry-plugins/justfile`:

```just
default:
    @just --list

build:
    cargo build --workspace

release:
    cargo build --workspace --release

check:
    cargo check --workspace
    cargo clippy --workspace

test:
    cargo test --workspace

fmt:
    cargo fmt --all

# Build a specific plugin
plugin name:
    cargo build -p owlry-plugin-{{name}} --release

# Build all plugins
plugins:
    cargo build --workspace --release

# Show all crate versions
show-versions:
    @for dir in crates/owlry-plugin-* crates/owlry-lua crates/owlry-rune; do \
        name=$(basename $dir); \
        version=$(grep '^version' $dir/Cargo.toml | head -1 | cut -d'"' -f2); \
        printf "%-35s %s\n" "$name" "$version"; \
    done

# Bump a specific crate version
bump-crate crate new_version:
    @cd crates/{{crate}} && \
    sed -i 's/^version = ".*"/version = "{{new_version}}"/' Cargo.toml
    @echo "Bumped {{crate}} to {{new_version}}"

# Bump all plugin crates
bump-all new_version:
    @for dir in crates/owlry-plugin-* crates/owlry-lua crates/owlry-rune; do \
        sed -i 's/^version = ".*"/version = "{{new_version}}"/' $dir/Cargo.toml; \
    done
    @echo "Bumped all crates to {{new_version}}"

# Install all plugins locally
install-local:
    just plugins
    @for f in target/release/libowlry_plugin_*.so; do \
        sudo install -Dm755 "$f" /usr/lib/owlry/plugins/$(basename "$f"); \
    done
    @for f in target/release/libowlry_lua.so target/release/libowlry_rune.so; do \
        [ -f "$f" ] && sudo install -Dm755 "$f" /usr/lib/owlry/runtimes/$(basename "$f"); \
    done
    @echo "Installed all plugins and runtimes"

# AUR operations
aur-update-pkg pkg:
    @cd aur/{{pkg}} && updpkgsums

aur-publish-pkg pkg:
    @cd aur/{{pkg}} && makepkg --printsrcinfo > .SRCINFO && \
    git add PKGBUILD .SRCINFO && \
    git commit -m "Update {{pkg}}" && \
    git push

aur-update-plugins:
    @for dir in aur/owlry-plugin-*; do \
        pkg=$(basename $dir); \
        echo "Updating $pkg..."; \
        cd $dir && updpkgsums && cd ../..; \
    done
```

- [ ] **Step 7: Commit**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add Cargo.toml justfile README.md .gitignore
git commit -m "feat: scaffold owlry-plugins workspace"
```

---

### Task 17: Move plugin crates to `owlry-plugins`

**Files:**
- Move: `owlry/crates/owlry-plugin-*` → `owlry-plugins/crates/owlry-plugin-*`
- Move: `owlry/crates/owlry-lua` → `owlry-plugins/crates/owlry-lua`
- Move: `owlry/crates/owlry-rune` → `owlry-plugins/crates/owlry-rune`
- Modify: each moved crate's `Cargo.toml` — change owlry-plugin-api dep

- [ ] **Step 1: Copy plugin crates**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou
mkdir -p owlry-plugins/crates

# Copy all plugin crates
for dir in owlry/crates/owlry-plugin-*; do
    cp -r "$dir" owlry-plugins/crates/
done

# Copy runtimes
cp -r owlry/crates/owlry-lua owlry-plugins/crates/
cp -r owlry/crates/owlry-rune owlry-plugins/crates/
```

- [ ] **Step 2: Update owlry-plugin-api dependency in all plugin Cargo.toml files**

For each plugin and runtime crate, change:

```toml
# Before (path dependency):
owlry-plugin-api = { path = "../owlry-plugin-api" }

# After (git dependency):
owlry-plugin-api = { git = "https://somegit.dev/Owlibou/owlry.git", tag = "plugin-api-v0.5.0" }
```

Run this for all crates:

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
for toml in crates/*/Cargo.toml; do
    sed -i 's|owlry-plugin-api = { path = "../owlry-plugin-api" }|owlry-plugin-api = { git = "https://somegit.dev/Owlibou/owlry.git", tag = "plugin-api-v0.5.0" }|' "$toml"
done
```

Note: The tag `plugin-api-v0.5.0` must exist in the owlry repo. Create it after Phase 2 is pushed. During development, use `branch = "main"` instead:

```toml
owlry-plugin-api = { git = "https://somegit.dev/Owlibou/owlry.git", branch = "main" }
```

- [ ] **Step 3: Update workspace.package.repository in root Cargo.toml**

Already set to `https://somegit.dev/Owlibou/owlry-plugins` in Task 16.

- [ ] **Step 4: Build plugins workspace**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
cargo build --workspace
```

Fix any compilation issues from the dependency change.

- [ ] **Step 5: Commit**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add crates/
git commit -m "feat: move all plugin and runtime crates from owlry"
```

---

### Task 18: Move AUR packages and docs to `owlry-plugins`

**Files:**
- Move: `owlry/aur/owlry-plugin-*` → `owlry-plugins/aur/`
- Move: `owlry/aur/owlry-lua` → `owlry-plugins/aur/`
- Move: `owlry/aur/owlry-rune` → `owlry-plugins/aur/`
- Move: `owlry/docs/PLUGIN_DEVELOPMENT.md` → `owlry-plugins/docs/`
- Move: `owlry/docs/PLUGINS.md` → `owlry-plugins/docs/`

- [ ] **Step 1: Copy AUR packages**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou
mkdir -p owlry-plugins/aur

for dir in owlry/aur/owlry-plugin-*; do
    cp -r "$dir" owlry-plugins/aur/
done
cp -r owlry/aur/owlry-lua owlry-plugins/aur/
cp -r owlry/aur/owlry-rune owlry-plugins/aur/
```

- [ ] **Step 2: Update PKGBUILD source URLs**

All plugin PKGBUILDs currently point at the owlry repo. Update them to point at owlry-plugins:

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
for pkgbuild in aur/*/PKGBUILD; do
    # Update source URL to point to owlry-plugins repo
    sed -i 's|somegit.dev/Owlibou/owlry/|somegit.dev/Owlibou/owlry-plugins/|g' "$pkgbuild"
done
```

Review each PKGBUILD to ensure build commands are correct (they should still work since the workspace structure under `crates/` is the same).

- [ ] **Step 3: Copy plugin docs**

```bash
mkdir -p owlry-plugins/docs
cp owlry/docs/PLUGIN_DEVELOPMENT.md owlry-plugins/docs/
cp owlry/docs/PLUGINS.md owlry-plugins/docs/
```

- [ ] **Step 4: Commit**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add aur/ docs/
git commit -m "feat: move AUR packages and plugin docs from owlry"
```

---

### Task 19: Clean up core repo after plugin extraction

**Files:**
- Modify: `owlry/Cargo.toml` — remove plugin/runtime workspace members
- Delete: `owlry/crates/owlry-plugin-*` (all 14 plugin crates)
- Delete: `owlry/crates/owlry-lua`, `owlry/crates/owlry-rune`
- Delete: `owlry/aur/owlry-plugin-*`, `owlry/aur/owlry-lua`, `owlry/aur/owlry-rune`
- Delete: `owlry/docs/PLUGIN_DEVELOPMENT.md`, `owlry/docs/PLUGINS.md`
- Modify: `owlry/justfile` — remove plugin build/bump/AUR targets

- [ ] **Step 1: Update workspace members**

In `owlry/Cargo.toml`, reduce members to:

```toml
[workspace]
members = [
    "crates/owlry",
    "crates/owlry-core",
    "crates/owlry-plugin-api",
]
```

- [ ] **Step 2: Delete moved crates**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
rm -rf crates/owlry-plugin-*
rm -rf crates/owlry-lua
rm -rf crates/owlry-rune
```

- [ ] **Step 3: Delete moved AUR packages**

```bash
rm -rf aur/owlry-plugin-*
rm -rf aur/owlry-lua
rm -rf aur/owlry-rune
```

- [ ] **Step 4: Delete moved docs**

```bash
rm -f docs/PLUGIN_DEVELOPMENT.md docs/PLUGINS.md
```

- [ ] **Step 5: Clean up justfile**

Remove plugin-specific targets from the justfile:
- Remove: `plugin`, `plugins`, `bump-plugins`, `bump-crate` (for plugins)
- Remove: plugin AUR targets (`aur-update-plugins`, `aur-publish-plugins`, etc.)
- Keep: core build, bump, AUR targets for owlry, owlry-core, owlry-plugin-api

- [ ] **Step 6: Add owlry-core AUR PKGBUILD**

Create `aur/owlry-core/PKGBUILD`:

```bash
# Maintainer: ...
pkgname=owlry-core
pkgver=0.5.0
pkgrel=1
pkgdesc='Core daemon for the Owlry application launcher'
arch=('x86_64')
url='https://somegit.dev/Owlibou/owlry'
license=('GPL-3.0-or-later')
depends=('gcc-libs')
makedepends=('cargo' 'git')
source=("$pkgname-$pkgver::git+https://somegit.dev/Owlibou/owlry.git#tag=v$pkgver")
sha256sums=('SKIP')

build() {
    cd "$pkgname-$pkgver"
    cargo build -p owlry-core --frozen --release
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/owlry-core" "$pkgdir/usr/bin/owlry-core"
    install -Dm644 "systemd/owlry-core.service" "$pkgdir/usr/lib/systemd/user/owlry-core.service"
    install -Dm644 "systemd/owlry-core.socket" "$pkgdir/usr/lib/systemd/user/owlry-core.socket"

    # Create plugin directories
    install -dm755 "$pkgdir/usr/lib/owlry/plugins"
    install -dm755 "$pkgdir/usr/lib/owlry/runtimes"
}
```

- [ ] **Step 7: Update meta-package PKGBUILDs**

Update `aur/owlry-meta-*/PKGBUILD` to add `owlry-core` as a dependency. The meta-essentials should depend on both `owlry` and `owlry-core`.

- [ ] **Step 8: Build and verify core workspace**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
cargo build --workspace
cargo test --workspace
```

- [ ] **Step 9: Commit**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git add -A
git commit -m "refactor: remove plugins and runtimes from core repo (moved to owlry-plugins)"
```

---

### Task 20: Update CLAUDE.md for new structure

**Files:**
- Modify: `owlry/CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md**

Rewrite CLAUDE.md to reflect:
- New workspace structure (3 crates: owlry, owlry-core, owlry-plugin-api)
- Daemon architecture and IPC
- Build commands for daemon and UI
- Updated justfile targets
- Reference to owlry-plugins repo for plugin development
- Updated AUR packaging info
- New CLI flags (--profile, removal of --providers)
- Systemd integration

- [ ] **Step 2: Create CLAUDE.md in owlry-plugins**

Create `owlry-plugins/CLAUDE.md` with:
- Plugin workspace structure
- Build commands
- Plugin API dependency info
- AUR packaging workflow
- How to add new plugins

- [ ] **Step 3: Commit both**

```bash
# Core repo
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for new architecture"

# Plugins repo
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add CLAUDE.md
git commit -m "docs: add CLAUDE.md for plugins workspace"
```

---

## Phase 4: Polish & Verify

### Task 21: End-to-end verification

**Files:** None (testing only)

- [ ] **Step 1: Build both workspaces**

```bash
# Core
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
cargo build --workspace
cargo test --workspace
cargo clippy --workspace

# Plugins
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
cargo build --workspace
cargo test --workspace
cargo clippy --workspace
```

- [ ] **Step 2: Install locally and test**

```bash
# Install core
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
just install-local

# Install plugins
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
just install-local
```

- [ ] **Step 3: Test daemon lifecycle**

```bash
# Start daemon
owlry-core &
ls $XDG_RUNTIME_DIR/owlry/owlry.sock  # Socket should exist

# Launch UI
owlry                    # Should connect to daemon, show all providers
owlry -m app,cmd         # Should filter to app+cmd
owlry --profile default  # Should use profile from config

# Test toggle
owlry &                  # Open UI
owlry                    # Should close existing UI

# Test dmenu (bypasses daemon)
echo -e "option1\noption2" | owlry -m dmenu -p "Pick:"

# Stop daemon
kill %1
```

- [ ] **Step 4: Test systemd integration**

```bash
# Copy service files
mkdir -p ~/.config/systemd/user
cp /usr/lib/systemd/user/owlry-core.service ~/.config/systemd/user/ 2>/dev/null
cp /usr/lib/systemd/user/owlry-core.socket ~/.config/systemd/user/ 2>/dev/null
systemctl --user daemon-reload

# Test service start
systemctl --user start owlry-core
systemctl --user status owlry-core
owlry  # Should connect to systemd-managed daemon

# Test socket activation
systemctl --user stop owlry-core
systemctl --user start owlry-core.socket
owlry  # Should trigger socket activation

# Cleanup
systemctl --user stop owlry-core owlry-core.socket
```

- [ ] **Step 5: Verify AUR PKGBUILDs build**

```bash
# Test core PKGBUILD
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry/aur/owlry
makepkg -s

cd ../owlry-core
makepkg -s

# Test a plugin PKGBUILD
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins/aur/owlry-plugin-calculator
makepkg -s
```

- [ ] **Step 6: Document any issues found**

If any tests fail, note the issue and fix it before proceeding.

---

### Task 22: Final cleanup and formatting

**Files:**
- All modified files across both repos

- [ ] **Step 1: Format all code**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
cargo fmt --all

cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
cargo fmt --all
```

- [ ] **Step 2: Run final clippy**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
cargo clippy --workspace -- -D warnings

cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 3: Review and clean up any leftover references**

Search for stale references in both repos:

```bash
# In core repo: any references to removed plugin crates?
grep -r "owlry-plugin-" crates/ --include="*.rs" --include="*.toml"

# In plugins repo: any path deps to owlry-plugin-api?
grep -r 'path = "../owlry-plugin-api"' crates/
```

- [ ] **Step 4: Final commits**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git add -A
git commit -m "chore: final cleanup and formatting"

cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add -A
git commit -m "chore: final cleanup and formatting"
```

---

### Task 23: Tag releases

**Files:** None (git operations only)

- [ ] **Step 1: Tag core repo**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git tag -a v0.5.0 -m "feat: client/daemon architecture split"
git tag -a plugin-api-v0.5.0 -m "owlry-plugin-api v0.5.0"
```

- [ ] **Step 2: Push core repo**

```bash
git push origin main --tags
```

- [ ] **Step 3: Update plugins repo to use tagged plugin-api**

Update all plugin `Cargo.toml` files from `branch = "main"` to `tag = "plugin-api-v0.5.0"`:

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
for toml in crates/*/Cargo.toml; do
    sed -i 's|branch = "main"|tag = "plugin-api-v0.5.0"|' "$toml"
done
cargo update
cargo build --workspace
git add -A
git commit -m "chore: pin owlry-plugin-api to tagged release v0.5.0"
```

- [ ] **Step 4: Tag and push plugins repo**

```bash
git tag -a v0.5.0 -m "Initial release: plugins split from owlry core"
git push origin main --tags
```

Note: User confirmed AUR publishing happens later, after verification. Do NOT push to AUR git repos in this task.
