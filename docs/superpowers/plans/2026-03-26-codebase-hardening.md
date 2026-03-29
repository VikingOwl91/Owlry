# Codebase Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 15 soundness, security, robustness, and quality issues across owlry core and owlry-plugins repos.

**Architecture:** Point fixes organized into 5 severity tiers. Each tier is one commit. Core repo (owlry) tiers 1-3 first, then plugins repo (owlry-plugins) tiers 4-5. No new features, no refactoring beyond what each fix requires.

**Tech Stack:** Rust 1.90+, abi_stable 0.11, toml 0.8, dirs 5.0

**Repos:**
- Core: `/home/cnachtigall/ssd/git/archive/owlibou/owlry`
- Plugins: `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins`

---

## Task 1: Tier 1 — Critical / Soundness (owlry core)

**Files:**
- Modify: `crates/owlry-plugin-api/src/lib.rs:297-320`
- Modify: `crates/owlry-core/src/server.rs:1-6,91-123,127-215`

### 1a. Replace `static mut HOST_API` with `OnceLock`

- [ ] **Step 1: Replace the static mut and init function**

In `crates/owlry-plugin-api/src/lib.rs`, replace lines 297-320:

```rust
// Old:
// static mut HOST_API: Option<&'static HostAPI> = None;
//
// pub unsafe fn init_host_api(api: &'static HostAPI) {
//     unsafe {
//         HOST_API = Some(api);
//     }
// }
//
// pub fn host_api() -> Option<&'static HostAPI> {
//     unsafe { HOST_API }
// }

// New:
use std::sync::OnceLock;

static HOST_API: OnceLock<&'static HostAPI> = OnceLock::new();

/// Initialize the host API (called by the host)
///
/// # Safety
/// Must only be called once by the host before any plugins use the API
pub unsafe fn init_host_api(api: &'static HostAPI) {
    let _ = HOST_API.set(api);
}

/// Get the host API
///
/// Returns None if the host hasn't initialized the API yet
pub fn host_api() -> Option<&'static HostAPI> {
    HOST_API.get().copied()
}
```

Note: `init_host_api` keeps its `unsafe` signature for API compatibility even though `OnceLock::set` is safe. The `unsafe` documents the caller contract.

- [ ] **Step 2: Verify the plugin-api crate compiles**

Run: `cargo check -p owlry-plugin-api`
Expected: success, no warnings about `static mut`

### 1b. Add IPC message size limit

- [ ] **Step 3: Add size-limited read loop in server.rs**

In `crates/owlry-core/src/server.rs`, add the constant near the top of the file (after the imports):

```rust
/// Maximum size of a single IPC request line (1 MB)
const MAX_REQUEST_SIZE: usize = 1_048_576;
```

Replace the `handle_client` method (lines 91-123). Change the `for line in reader.lines()` loop to a manual `read_line` loop with size checking:

```rust
fn handle_client(
    stream: UnixStream,
    pm: Arc<Mutex<ProviderManager>>,
    frecency: Arc<Mutex<FrecencyStore>>,
    config: Arc<Config>,
) -> io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut reader = reader;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break; // EOF
        }

        if line.len() > MAX_REQUEST_SIZE {
            let resp = Response::Error {
                message: "request too large".to_string(),
            };
            write_response(&mut writer, &resp)?;
            break; // Drop connection
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Request = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = Response::Error {
                    message: format!("invalid request JSON: {}", e),
                };
                write_response(&mut writer, &resp)?;
                continue;
            }
        };

        let response = Self::handle_request(&request, &pm, &frecency, &config);
        write_response(&mut writer, &response)?;
    }

    Ok(())
}
```

### 1c. Handle mutex poisoning gracefully

- [ ] **Step 4: Replace all lock().unwrap() in handle_request**

In `crates/owlry-core/src/server.rs`, in the `handle_request` method, replace every occurrence of `.lock().unwrap()` with `.lock().unwrap_or_else(|e| e.into_inner())`. There are instances in the `Query`, `Launch`, `Providers`, `Refresh`, `Submenu`, and `PluginAction` arms.

For example, the Query arm changes from:
```rust
let pm_guard = pm.lock().unwrap();
let frecency_guard = frecency.lock().unwrap();
```
to:
```rust
let pm_guard = pm.lock().unwrap_or_else(|e| e.into_inner());
let frecency_guard = frecency.lock().unwrap_or_else(|e| e.into_inner());
```

Apply this pattern to all `.lock().unwrap()` calls in `handle_request`.

- [ ] **Step 5: Build and test the core crate**

Run: `cargo check -p owlry-core && cargo test -p owlry-core`
Expected: all checks pass, all existing tests pass

- [ ] **Step 6: Commit Tier 1**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git add crates/owlry-plugin-api/src/lib.rs crates/owlry-core/src/server.rs
git commit -m "fix: soundness — OnceLock for HOST_API, IPC size limits, mutex poisoning recovery"
```

---

## Task 2: Tier 2 — Security (owlry core)

**Files:**
- Modify: `crates/owlry-core/src/server.rs:1-6,29-36,91-123`
- Modify: `crates/owlry-core/src/main.rs:26-32`

### 2a. Set socket permissions after bind

- [ ] **Step 1: Add permission setting in Server::bind**

In `crates/owlry-core/src/server.rs`, add the import at the top:

```rust
use std::os::unix::fs::PermissionsExt;
```

In `Server::bind()`, after the `UnixListener::bind(socket_path)?;` line, add:

```rust
std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
```

### 2b. Log signal handler failure

- [ ] **Step 2: Replace .ok() with warning log in main.rs**

In `crates/owlry-core/src/main.rs`, add `use log::warn;` to the imports, then replace lines 26-32:

```rust
// Old:
// ctrlc::set_handler(move || {
//     let _ = std::fs::remove_file(&sock_cleanup);
//     std::process::exit(0);
// })
// .ok();

// New:
if let Err(e) = ctrlc::set_handler(move || {
    let _ = std::fs::remove_file(&sock_cleanup);
    std::process::exit(0);
}) {
    warn!("Failed to set signal handler: {}", e);
}
```

### 2c. Add client read timeout

- [ ] **Step 3: Set read timeout on accepted connections**

In `crates/owlry-core/src/server.rs`, add `use std::time::Duration;` to the imports.

In the `handle_client` method, at the very top (before the `BufReader` creation), add:

```rust
stream.set_read_timeout(Some(Duration::from_secs(30)))?;
```

This means the `stream` passed to `handle_client` needs to be mutable, or we set it on the clone. Since `set_read_timeout` takes `&self` (not `&mut self`), we can call it directly:

```rust
fn handle_client(
    stream: UnixStream,
    pm: Arc<...>,
    frecency: Arc<...>,
    config: Arc<Config>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let reader = BufReader::new(stream.try_clone()?);
    // ... rest unchanged
```

- [ ] **Step 4: Build and test**

Run: `cargo check -p owlry-core && cargo test -p owlry-core`
Expected: all checks pass, all existing tests pass

- [ ] **Step 5: Commit Tier 2**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git add crates/owlry-core/src/server.rs crates/owlry-core/src/main.rs
git commit -m "fix: security — socket perms 0600, signal handler logging, client read timeout"
```

---

## Task 3: Tier 3 — Robustness / Quality (owlry core)

**Files:**
- Modify: `crates/owlry-core/src/server.rs:1-6,17-23,53-73,91-215`

### 3a. Log malformed JSON requests

- [ ] **Step 1: Add warn! for JSON parse errors**

In `crates/owlry-core/src/server.rs`, in the `handle_client` method, in the JSON parse error arm, add a warning log before the error response:

```rust
Err(e) => {
    warn!("Malformed request from client: {}", e);
    let resp = Response::Error {
        message: format!("invalid request JSON: {}", e),
    };
    write_response(&mut writer, &resp)?;
    continue;
}
```

### 3b. Replace Mutex with RwLock

- [ ] **Step 2: Change Server struct and imports**

In `crates/owlry-core/src/server.rs`, change the import from `Mutex` to `RwLock`:

```rust
use std::sync::{Arc, RwLock};
```

Change the `Server` struct fields:

```rust
pub struct Server {
    listener: UnixListener,
    socket_path: PathBuf,
    provider_manager: Arc<RwLock<ProviderManager>>,
    frecency: Arc<RwLock<FrecencyStore>>,
    config: Arc<Config>,
}
```

- [ ] **Step 3: Update Server::bind**

In `Server::bind()`, change `Arc::new(Mutex::new(...))` to `Arc::new(RwLock::new(...))`:

```rust
Ok(Self {
    listener,
    socket_path: socket_path.to_path_buf(),
    provider_manager: Arc::new(RwLock::new(provider_manager)),
    frecency: Arc::new(RwLock::new(frecency)),
    config: Arc::new(config),
})
```

- [ ] **Step 4: Update handle_client and handle_request signatures**

Change `handle_client` parameter types:

```rust
fn handle_client(
    stream: UnixStream,
    pm: Arc<RwLock<ProviderManager>>,
    frecency: Arc<RwLock<FrecencyStore>>,
    config: Arc<Config>,
) -> io::Result<()> {
```

Change `handle_request` parameter types:

```rust
fn handle_request(
    request: &Request,
    pm: &Arc<RwLock<ProviderManager>>,
    frecency: &Arc<RwLock<FrecencyStore>>,
    config: &Arc<Config>,
) -> Response {
```

Also update `handle_one_for_testing` if it passes these types through.

- [ ] **Step 5: Update lock calls per request type**

In `handle_request`, change each lock call according to the read/write mapping:

**Query** (read PM, read frecency):
```rust
Request::Query { text, modes } => {
    let filter = match modes {
        Some(m) => ProviderFilter::from_mode_strings(m),
        None => ProviderFilter::all(),
    };
    let max = config.general.max_results;
    let weight = config.providers.frecency_weight;

    let pm_guard = pm.read().unwrap_or_else(|e| e.into_inner());
    let frecency_guard = frecency.read().unwrap_or_else(|e| e.into_inner());
    let results = pm_guard.search_with_frecency(
        text, max, &filter, &frecency_guard, weight, None,
    );

    Response::Results {
        items: results
            .into_iter()
            .map(|(item, score)| launch_item_to_result(item, score))
            .collect(),
    }
}
```

**Launch** (write frecency):
```rust
Request::Launch { item_id, provider: _ } => {
    let mut frecency_guard = frecency.write().unwrap_or_else(|e| e.into_inner());
    frecency_guard.record_launch(item_id);
    Response::Ack
}
```

**Providers** (read PM):
```rust
Request::Providers => {
    let pm_guard = pm.read().unwrap_or_else(|e| e.into_inner());
    let descs = pm_guard.available_providers();
    Response::Providers {
        list: descs.into_iter().map(descriptor_to_desc).collect(),
    }
}
```

**Refresh** (write PM):
```rust
Request::Refresh { provider } => {
    let mut pm_guard = pm.write().unwrap_or_else(|e| e.into_inner());
    pm_guard.refresh_provider(provider);
    Response::Ack
}
```

**Toggle** (no locks):
```rust
Request::Toggle => Response::Ack,
```

**Submenu** (read PM):
```rust
Request::Submenu { plugin_id, data } => {
    let pm_guard = pm.read().unwrap_or_else(|e| e.into_inner());
    match pm_guard.query_submenu_actions(plugin_id, data, plugin_id) {
        Some((_name, actions)) => Response::SubmenuItems {
            items: actions
                .into_iter()
                .map(|item| launch_item_to_result(item, 0))
                .collect(),
        },
        None => Response::Error {
            message: format!("no submenu actions for plugin '{}'", plugin_id),
        },
    }
}
```

**PluginAction** (read PM):
```rust
Request::PluginAction { command } => {
    let pm_guard = pm.read().unwrap_or_else(|e| e.into_inner());
    if pm_guard.execute_plugin_action(command) {
        Response::Ack
    } else {
        Response::Error {
            message: format!("no plugin handled action '{}'", command),
        }
    }
}
```

- [ ] **Step 6: Build and test**

Run: `cargo check -p owlry-core && cargo test -p owlry-core`
Expected: all checks pass, all existing tests pass

- [ ] **Step 7: Commit Tier 3**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry
git add crates/owlry-core/src/server.rs
git commit -m "fix: robustness — RwLock for concurrent reads, log malformed JSON requests"
```

---

## Task 4: Tier 4 — Critical fixes (owlry-plugins)

**Files:**
- Modify: `crates/owlry-plugin-converter/src/currency.rs:88-113,244-265`
- Modify: `crates/owlry-plugin-converter/src/units.rs:90-101,160-213`
- Modify: `crates/owlry-plugin-bookmarks/src/lib.rs:40-45,228-260,317-353`

All paths relative to `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins`.

### 4a. Fix Box::leak memory leak in converter

- [ ] **Step 1: Change resolve_currency_code return type**

In `crates/owlry-plugin-converter/src/currency.rs`, change the `resolve_currency_code` function (line 88) from returning `Option<String>` to `Option<&'static str>`:

```rust
pub fn resolve_currency_code(alias: &str) -> Option<&'static str> {
    let lower = alias.to_lowercase();

    // Check aliases
    for ca in CURRENCY_ALIASES {
        if ca.aliases.contains(&lower.as_str()) {
            return Some(ca.code);
        }
    }

    // Check if it's a raw 3-letter ISO code we know about
    let upper = alias.to_uppercase();
    if upper.len() == 3 {
        if upper == "EUR" {
            return Some("EUR");
        }
        // Check if we have rates for it — return the matching alias code
        if let Some(rates) = get_rates()
            && rates.rates.contains_key(&upper)
        {
            // Find a matching CURRENCY_ALIASES entry for this code
            for ca in CURRENCY_ALIASES {
                if ca.code == upper {
                    return Some(ca.code);
                }
            }
            // Not in our aliases but valid in ECB rates — we can't return
            // a &'static str for an arbitrary code, so skip
        }
    }

    None
}
```

Note: For ISO codes that are in ECB rates but NOT in `CURRENCY_ALIASES`, we lose the ability to resolve them. This is acceptable because:
1. `CURRENCY_ALIASES` already covers the 15 most common currencies
2. The alternative (Box::leak) was leaking memory on every keystroke

- [ ] **Step 2: Update is_currency_alias**

No change needed — it already just calls `resolve_currency_code(alias).is_some()`.

- [ ] **Step 3: Update find_unit in units.rs**

In `crates/owlry-plugin-converter/src/units.rs`, replace lines 90-101:

```rust
pub fn find_unit(alias: &str) -> Option<&'static str> {
    let lower = alias.to_lowercase();
    if let Some(&i) = ALIAS_MAP.get(&lower) {
        return Some(UNITS[i].symbol);
    }
    // Check currency — resolve_currency_code now returns &'static str directly
    currency::resolve_currency_code(&lower)
}
```

- [ ] **Step 4: Update convert_currency in units.rs**

In `crates/owlry-plugin-converter/src/units.rs`, update `convert_currency` (line 160). The `from_code` and `to_code` are now `&'static str`. HashMap lookups with `rates.rates.get(code)` work because `HashMap<String, f64>::get` accepts `&str` via `Borrow`:

```rust
fn convert_currency(value: f64, from: &str, to: &str) -> Option<ConversionResult> {
    let rates = currency::get_rates()?;
    let from_code = currency::resolve_currency_code(from)?;
    let to_code = currency::resolve_currency_code(to)?;

    let from_rate = if from_code == "EUR" {
        1.0
    } else {
        *rates.rates.get(from_code)?
    };
    let to_rate = if to_code == "EUR" {
        1.0
    } else {
        *rates.rates.get(to_code)?
    };

    let result = value / from_rate * to_rate;
    Some(format_currency_result(result, to_code))
}
```

- [ ] **Step 5: Update convert_currency_common in units.rs**

In `crates/owlry-plugin-converter/src/units.rs`, update `convert_currency_common` (line 180). Change `from_code` handling:

```rust
fn convert_currency_common(value: f64, from: &str) -> Vec<ConversionResult> {
    let rates = match currency::get_rates() {
        Some(r) => r,
        None => return vec![],
    };
    let from_code = match currency::resolve_currency_code(from) {
        Some(c) => c,
        None => return vec![],
    };

    let targets = COMMON_TARGETS.get(&Category::Currency).unwrap();
    let from_rate = if from_code == "EUR" {
        1.0
    } else {
        match rates.rates.get(from_code) {
            Some(&r) => r,
            None => return vec![],
        }
    };

    targets
        .iter()
        .filter(|&&sym| sym != from_code)
        .filter_map(|&sym| {
            let to_rate = if sym == "EUR" {
                1.0
            } else {
                *rates.rates.get(sym)?
            };
            let result = value / from_rate * to_rate;
            Some(format_currency_result(result, sym))
        })
        .take(5)
        .collect()
}
```

- [ ] **Step 6: Update currency tests**

In `crates/owlry-plugin-converter/src/currency.rs`, update test assertions to use `&str` instead of `String`:

```rust
#[test]
fn test_resolve_currency_code_iso() {
    assert_eq!(resolve_currency_code("usd"), Some("USD"));
    assert_eq!(resolve_currency_code("EUR"), Some("EUR"));
}

#[test]
fn test_resolve_currency_code_name() {
    assert_eq!(resolve_currency_code("dollar"), Some("USD"));
    assert_eq!(resolve_currency_code("euro"), Some("EUR"));
    assert_eq!(resolve_currency_code("pounds"), Some("GBP"));
}

#[test]
fn test_resolve_currency_code_symbol() {
    assert_eq!(resolve_currency_code("$"), Some("USD"));
    assert_eq!(resolve_currency_code("€"), Some("EUR"));
    assert_eq!(resolve_currency_code("£"), Some("GBP"));
}

#[test]
fn test_resolve_currency_unknown() {
    assert_eq!(resolve_currency_code("xyz"), None);
}
```

### 4b. Fix bookmarks temp file race condition

- [ ] **Step 7: Use PID-based temp filenames**

In `crates/owlry-plugin-bookmarks/src/lib.rs`, replace the `read_firefox_bookmarks` method. Change lines 318-319 and the corresponding favicons temp path:

```rust
fn read_firefox_bookmarks(places_path: &PathBuf, items: &mut Vec<PluginItem>) {
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let temp_db = temp_dir.join(format!("owlry_places_{}.sqlite", pid));

    // Copy database to temp location to avoid locking issues
    if fs::copy(places_path, &temp_db).is_err() {
        return;
    }

    // Also copy WAL file if it exists
    let wal_path = places_path.with_extension("sqlite-wal");
    if wal_path.exists() {
        let temp_wal = temp_db.with_extension("sqlite-wal");
        let _ = fs::copy(&wal_path, &temp_wal);
    }

    // Copy favicons database if available
    let favicons_path = Self::firefox_favicons_path(places_path);
    let temp_favicons = temp_dir.join(format!("owlry_favicons_{}.sqlite", pid));
    if let Some(ref fp) = favicons_path {
        let _ = fs::copy(fp, &temp_favicons);
        let fav_wal = fp.with_extension("sqlite-wal");
        if fav_wal.exists() {
            let _ = fs::copy(&fav_wal, temp_favicons.with_extension("sqlite-wal"));
        }
    }

    let cache_dir = Self::ensure_favicon_cache_dir();

    // Read bookmarks from places.sqlite
    let bookmarks = Self::fetch_firefox_bookmarks(&temp_db, &temp_favicons, cache_dir.as_ref());

    // Clean up temp files
    let _ = fs::remove_file(&temp_db);
    let _ = fs::remove_file(temp_db.with_extension("sqlite-wal"));
    let _ = fs::remove_file(&temp_favicons);
    let _ = fs::remove_file(temp_favicons.with_extension("sqlite-wal"));

    // ... rest of method unchanged (the for loop adding items)
```

### 4c. Fix bookmarks background refresh never updating state

- [ ] **Step 8: Change BookmarksState to use Arc<Mutex<Vec<PluginItem>>>**

In `crates/owlry-plugin-bookmarks/src/lib.rs`, add `use std::sync::Mutex;` to imports (it's already importing `Arc` and `AtomicBool`).

Change the struct:

```rust
struct BookmarksState {
    /// Cached bookmark items (shared with background thread)
    items: Arc<Mutex<Vec<PluginItem>>>,
    /// Flag to prevent concurrent background loads
    loading: Arc<AtomicBool>,
}

impl BookmarksState {
    fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
            loading: Arc::new(AtomicBool::new(false)),
        }
    }
```

- [ ] **Step 9: Update load_bookmarks to write through Arc<Mutex>**

Update the `load_bookmarks` method:

```rust
fn load_bookmarks(&self) {
    // Fast path: load from cache immediately if items are empty
    {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        if items.is_empty() {
            *items = Self::load_cached_bookmarks();
        }
    }

    // Don't start another background load if one is already running
    if self.loading.swap(true, Ordering::SeqCst) {
        return;
    }

    // Spawn background thread to refresh bookmarks
    let loading = self.loading.clone();
    let items_ref = self.items.clone();
    thread::spawn(move || {
        let mut new_items = Vec::new();

        // Load Chrome/Chromium bookmarks (fast - just JSON parsing)
        for path in Self::chromium_bookmark_paths() {
            if path.exists() {
                Self::read_chrome_bookmarks_static(&path, &mut new_items);
            }
        }

        // Load Firefox bookmarks with favicons (synchronous with rusqlite)
        for path in Self::firefox_places_paths() {
            Self::read_firefox_bookmarks(&path, &mut new_items);
        }

        // Save to cache for next startup
        Self::save_cached_bookmarks(&new_items);

        // Update shared state so next refresh returns fresh data
        if let Ok(mut items) = items_ref.lock() {
            *items = new_items;
        }

        loading.store(false, Ordering::SeqCst);
    });
}
```

Note: `load_bookmarks` now takes `&self` instead of `&mut self`.

- [ ] **Step 10: Update provider_refresh to read from Arc<Mutex>**

Update the `provider_refresh` function:

```rust
extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<BookmarksState>
    let state = unsafe { &*(handle.ptr as *const BookmarksState) };

    // Load bookmarks
    state.load_bookmarks();

    // Return items
    let items = state.items.lock().unwrap_or_else(|e| e.into_inner());
    items.to_vec().into()
}
```

Note: Uses `&*` (shared ref) instead of `&mut *` since `load_bookmarks` now takes `&self`.

- [ ] **Step 11: Build and test plugins**

Run: `cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins && cargo check && cargo test`
Expected: all checks pass, all existing tests pass

- [ ] **Step 12: Commit Tier 4**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add crates/owlry-plugin-converter/src/currency.rs crates/owlry-plugin-converter/src/units.rs crates/owlry-plugin-bookmarks/src/lib.rs
git commit -m "fix: critical — eliminate Box::leak in converter, secure temp files, fix background refresh"
```

---

## Task 5: Tier 5 — Quality fixes (owlry-plugins)

**Files:**
- Modify: `crates/owlry-plugin-ssh/Cargo.toml`
- Modify: `crates/owlry-plugin-ssh/src/lib.rs:17-48`
- Modify: `crates/owlry-plugin-websearch/Cargo.toml`
- Modify: `crates/owlry-plugin-websearch/src/lib.rs:46-76,174-177`
- Modify: `crates/owlry-plugin-emoji/src/lib.rs:34-37,463-481`
- Modify: `crates/owlry-plugin-calculator/src/lib.rs:139`
- Modify: `crates/owlry-plugin-converter/src/lib.rs:95`

All paths relative to `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins`.

### 5a. SSH plugin: read terminal from config

- [ ] **Step 1: Add toml dependency to SSH plugin**

In `crates/owlry-plugin-ssh/Cargo.toml`, add:

```toml
# TOML config parsing
toml = "0.8"
```

- [ ] **Step 2: Add config loading and update SshState::new**

In `crates/owlry-plugin-ssh/src/lib.rs`, add `use std::fs;` to imports, remove the `DEFAULT_TERMINAL` constant, and update `SshState::new`:

```rust
impl SshState {
    fn new() -> Self {
        let terminal = Self::load_terminal_from_config();

        Self {
            items: Vec::new(),
            terminal_command: terminal,
        }
    }

    fn load_terminal_from_config() -> String {
        // Try [plugins.ssh] in config.toml
        let config_path = dirs::config_dir().map(|d| d.join("owlry").join("config.toml"));
        if let Some(content) = config_path.and_then(|p| fs::read_to_string(p).ok())
            && let Ok(toml) = content.parse::<toml::Table>()
        {
            if let Some(plugins) = toml.get("plugins").and_then(|v| v.as_table())
                && let Some(ssh) = plugins.get("ssh").and_then(|v| v.as_table())
                && let Some(terminal) = ssh.get("terminal").and_then(|v| v.as_str())
            {
                return terminal.to_string();
            }
        }

        // Fall back to $TERMINAL env var
        if let Ok(terminal) = std::env::var("TERMINAL") {
            return terminal;
        }

        // Last resort
        "xdg-terminal-exec".to_string()
    }
```

### 5b. WebSearch plugin: read engine from config

- [ ] **Step 3: Add dependencies to websearch plugin**

In `crates/owlry-plugin-websearch/Cargo.toml`, add:

```toml
# TOML config parsing
toml = "0.8"

# XDG directories for config
dirs = "5.0"
```

- [ ] **Step 4: Add config loading and update provider_init**

In `crates/owlry-plugin-websearch/src/lib.rs`, add `use std::fs;` to imports. Add a config loading function and update `provider_init`:

```rust
fn load_engine_from_config() -> String {
    let config_path = dirs::config_dir().map(|d| d.join("owlry").join("config.toml"));
    if let Some(content) = config_path.and_then(|p| fs::read_to_string(p).ok())
        && let Ok(toml) = content.parse::<toml::Table>()
    {
        if let Some(plugins) = toml.get("plugins").and_then(|v| v.as_table())
            && let Some(websearch) = plugins.get("websearch").and_then(|v| v.as_table())
            && let Some(engine) = websearch.get("engine").and_then(|v| v.as_str())
        {
            return engine.to_string();
        }
    }
    DEFAULT_ENGINE.to_string()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let engine = load_engine_from_config();
    let state = Box::new(WebSearchState::with_engine(&engine));
    ProviderHandle::from_box(state)
}
```

Remove the TODO comment from the old `provider_init`.

### 5c. Emoji plugin: build items once at init

- [ ] **Step 5: Move load_emojis to constructor**

In `crates/owlry-plugin-emoji/src/lib.rs`, change `EmojiState::new` to call `load_emojis`:

```rust
impl EmojiState {
    fn new() -> Self {
        let mut state = Self { items: Vec::new() };
        state.load_emojis();
        state
    }
```

Update `provider_refresh` to just return the cached items without reloading:

```rust
extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<EmojiState>
    let state = unsafe { &*(handle.ptr as *const EmojiState) };

    // Return cached items (loaded once at init)
    state.items.to_vec().into()
}
```

Note: Uses `&*` (shared ref) since we're only reading.

### 5d. Calculator/Converter: safer shell commands

- [ ] **Step 6: Fix calculator command**

In `crates/owlry-plugin-calculator/src/lib.rs`, in `evaluate_expression` (around line 139), replace:

```rust
// Old:
format!("sh -c 'echo -n \"{}\" | wl-copy'", result_str)

// New:
format!("printf '%s' '{}' | wl-copy", result_str.replace('\'', "'\\''"))
```

- [ ] **Step 7: Fix converter command**

In `crates/owlry-plugin-converter/src/lib.rs`, in `provider_query` (around line 95), replace:

```rust
// Old:
format!("sh -c 'echo -n \"{}\" | wl-copy'", r.raw_value)

// New:
format!("printf '%s' '{}' | wl-copy", r.raw_value.replace('\'', "'\\''"))
```

- [ ] **Step 8: Build and test all plugins**

Run: `cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins && cargo check && cargo test`
Expected: all checks pass, all existing tests pass

- [ ] **Step 9: Commit Tier 5**

```bash
cd /home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins
git add crates/owlry-plugin-ssh/Cargo.toml crates/owlry-plugin-ssh/src/lib.rs \
  crates/owlry-plugin-websearch/Cargo.toml crates/owlry-plugin-websearch/src/lib.rs \
  crates/owlry-plugin-emoji/src/lib.rs \
  crates/owlry-plugin-calculator/src/lib.rs \
  crates/owlry-plugin-converter/src/lib.rs
git commit -m "fix: quality — config-based terminal/engine, emoji init perf, safer shell commands"
```
