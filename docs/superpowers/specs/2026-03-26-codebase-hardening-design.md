# Codebase Hardening: owlry + owlry-plugins

**Date:** 2026-03-26
**Scope:** 15 fixes across 2 repositories, organized in 5 severity tiers
**Approach:** Severity-ordered tiers, one commit per tier, core repo first

---

## Tier 1: Critical / Soundness (owlry core)

### 1a. Replace `static mut HOST_API` with `OnceLock`

**File:** `crates/owlry-plugin-api/src/lib.rs`
**Problem:** `static mut` is unsound — concurrent reads during initialization are UB.
**Fix:** Replace with `std::sync::OnceLock<&'static HostAPI>`. `init_host_api()` calls `HOST_API.set(api)`, `host_api()` calls `HOST_API.get().copied()`. No public API changes — convenience wrappers (`notify()`, `log_info()`, etc.) keep working. No ABI impact since `HOST_API` is internal.

### 1b. Add IPC message size limit

**File:** `crates/owlry-core/src/server.rs`
**Problem:** `BufReader::lines()` reads unbounded lines. A malicious/buggy client can OOM the daemon.
**Fix:** Replace the `lines()` iterator with a manual `read_line()` loop enforcing a 1 MB max. Lines exceeding the limit get an error response and the connection is dropped. Constant: `const MAX_REQUEST_SIZE: usize = 1_048_576`.

### 1c. Handle mutex poisoning gracefully

**File:** `crates/owlry-core/src/server.rs`
**Problem:** All `lock().unwrap()` calls panic on poisoned mutex, crashing handler threads.
**Fix:** Replace with `lock().unwrap_or_else(|e| e.into_inner())`. The ProviderManager and FrecencyStore don't have invariants that require abort-on-poison.

---

## Tier 2: Security (owlry core)

### 2a. Set socket permissions after bind

**File:** `crates/owlry-core/src/server.rs`
**Problem:** Socket inherits process umask, may be readable by other local users.
**Fix:** After `UnixListener::bind()`, call `std::fs::set_permissions(socket_path, Permissions::from_mode(0o600))`. Uses `std::os::unix::fs::PermissionsExt`.

### 2b. Log signal handler failure

**File:** `crates/owlry-core/src/main.rs`
**Problem:** `ctrlc::set_handler(...).ok()` silently swallows errors. Failed handler means no socket cleanup on SIGINT.
**Fix:** Replace `.ok()` with `if let Err(e) = ... { warn!("...") }`.

### 2c. Add client read timeout

**File:** `crates/owlry-core/src/server.rs`
**Problem:** A client that connects but never sends data blocks a thread forever.
**Fix:** Set `stream.set_read_timeout(Some(Duration::from_secs(30)))` on accepted connections before entering the read loop.

---

## Tier 3: Robustness / Quality (owlry core)

### 3a. Log malformed JSON requests

**File:** `crates/owlry-core/src/server.rs`
**Problem:** JSON parse errors only sent as response to client, not visible in daemon logs.
**Fix:** Add `warn!("Malformed request from client: {}", e)` before sending the error response.

### 3b. Replace Mutex with RwLock for concurrent reads

**File:** `crates/owlry-core/src/server.rs`
**Problem:** `Mutex<ProviderManager>` blocks all concurrent queries even though they're read-only.
**Fix:** Replace both `Arc<Mutex<ProviderManager>>` and `Arc<Mutex<FrecencyStore>>` with `Arc<RwLock<...>>`.

Lock usage per request type:

| Request | ProviderManager | FrecencyStore |
|---------|----------------|---------------|
| Query | `read()` | `read()` |
| Launch | — | `write()` |
| Providers | `read()` | — |
| Refresh | `write()` | — |
| Toggle | — | — |
| Submenu | `read()` | — |
| PluginAction | `read()` | — |

Poisoning recovery: `.unwrap_or_else(|e| e.into_inner())` applies to RwLock the same way.

---

## Tier 4: Critical fixes (owlry-plugins)

### 4a. Fix `Box::leak` memory leak in converter

**File:** `owlry-plugins/crates/owlry-plugin-converter/src/units.rs`
**Problem:** `Box::leak(code.into_boxed_str())` leaks memory on every keystroke for currency queries.
**Fix:** Currency codes are already `&'static str` in `CURRENCY_ALIASES`. Change `resolve_currency_code()` return type from `Option<String>` to `Option<&'static str>` so it returns the static str directly. This eliminates the `Box::leak`. Callers in `units.rs` (`find_unit`, `convert_currency`, `convert_currency_common`) and `currency.rs` (`is_currency_alias`) must be updated to work with `&'static str` — mostly removing `.to_string()` calls or adding them at the boundary where `String` is needed (e.g., HashMap lookups that need owned keys).

### 4b. Fix bookmarks temp file race condition

**File:** `owlry-plugins/crates/owlry-plugin-bookmarks/src/lib.rs`
**Problem:** Predictable `/tmp/owlry_places_temp.sqlite` path — concurrent instances clobber, symlink attacks possible.
**Fix:** Append PID and monotonic counter to filename: `owlry_places_{pid}.sqlite`. Uses `std::process::id()`. Each profile copy gets its own name via index. Cleanup on exit remains the same.

### 4c. Fix bookmarks background refresh never updating state

**File:** `owlry-plugins/crates/owlry-plugin-bookmarks/src/lib.rs`
**Problem:** Background thread loads items and saves cache but never writes back to `self.items`. Current session keeps stale data.
**Fix:** Replace `items: Vec<PluginItem>` with `items: Arc<Mutex<Vec<PluginItem>>>`. Background thread writes to the shared vec after completing. `provider_refresh` reads from it. The `loading` AtomicBool already prevents concurrent loads.

---

## Tier 5: Quality fixes (owlry-plugins)

### 5a. SSH plugin: read terminal from config

**File:** `owlry-plugins/crates/owlry-plugin-ssh/src/lib.rs`
**Problem:** Hardcoded `kitty` as terminal fallback. Core already detects terminals.
**Fix:** Read `terminal` from `[plugins.ssh]` in `~/.config/owlry/config.toml`. Fall back to `$TERMINAL` env var, then `xdg-terminal-exec`. Same config pattern as weather/pomodoro plugins.

### 5b. WebSearch plugin: read engine from config

**File:** `owlry-plugins/crates/owlry-plugin-websearch/src/lib.rs`
**Problem:** TODO comment for config reading, never implemented. Engine is always duckduckgo.
**Fix:** Read `engine` from `[plugins.websearch]` in config.toml. Supports named engines (`google`, `duckduckgo`, etc.) or custom URL templates with `{query}`. Falls back to duckduckgo.

### 5c. Emoji plugin: build items once at init

**File:** `owlry-plugins/crates/owlry-plugin-emoji/src/lib.rs`
**Problem:** `load_emojis()` clears and rebuilds ~370 items on every `refresh()` call.
**Fix:** Call `load_emojis()` in `EmojiState::new()`. `provider_refresh` returns `self.items.clone()` without rebuilding.

### 5d. Calculator/Converter: safer shell commands

**Files:** `owlry-plugin-calculator/src/lib.rs`, `owlry-plugin-converter/src/lib.rs`
**Problem:** `sh -c 'echo -n "..."'` pattern with double-quote interpolation. Theoretically breakable by unexpected result formatting.
**Fix:** Use `printf '%s' '...' | wl-copy` with single-quote escaping (`replace('\'', "'\\''")`) — the same safe pattern already used by bookmarks and clipboard plugins.

---

## Out of scope

These were identified but deferred:

- **Hardcoded emoji list** — replacing with a crate/data file is a feature, not a fix
- **Plugin vtable-level tests** — valuable but a separate testing initiative
- **IPC protocol versioning** — protocol change, not a bug fix
- **Plugin sandbox enforcement** — large feature, not a point fix
- **Desktop Exec field sanitization** — deep rabbit hole, needs separate design
- **Config validation** — separate concern, deserves its own pass
