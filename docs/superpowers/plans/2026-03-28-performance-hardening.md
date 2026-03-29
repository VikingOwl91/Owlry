# Performance Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate UI startup delay and per-keystroke search lag in the Owlry launcher.

**Architecture:** Six targeted fixes across two crates (`owlry` client, `owlry-core` daemon). The two critical fixes move blocking IPC off the GTK main thread and defer the initial query until after the window is visible. The remaining four are low-risk surgical optimizations to config loading, frecency scoring, result list updates, and the periodic refresh timer.

**Tech Stack:** Rust 1.90+, GTK4 0.10 (glib channels), owlry-core IPC

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/owlry/src/backend.rs` | Modify | Add `SearchBackend::search_async()` method using `std::thread` + `glib::MainContext::channel()` |
| `crates/owlry/src/ui/main_window.rs` | Modify | Use async search in debounce handler; defer `update_results` to after `present()`; use `lazy_state.displayed_count` in `scroll_to_row`; remove redundant `results.clone()` |
| `crates/owlry/src/app.rs` | Modify | Move `MainWindow::new()` result population to post-`present()` idle callback |
| `crates/owlry-core/src/config/mod.rs` | Modify | Replace `which` subprocess calls with in-process PATH lookup |
| `crates/owlry-core/src/data/frecency.rs` | Modify | Add `get_score_at()` accepting a pre-sampled timestamp |
| `crates/owlry-core/src/providers/mod.rs` | Modify | Sample `Utc::now()` once before scoring loop; use `get_score_at()` |

---

### Task 1: Replace `which` subprocesses with in-process PATH lookup

The `command_exists()` function in config spawns a `which` subprocess for every terminal candidate. On a cold cache this adds 200-500ms to startup. Replace it with a pure-Rust PATH scan.

**Files:**
- Modify: `crates/owlry-core/src/config/mod.rs:525-532`
- Test: `crates/owlry-core/src/config/mod.rs` (existing inline tests)

- [ ] **Step 1: Write test for the new `command_exists` implementation**

Add at the bottom of the `#[cfg(test)]` block in `config/mod.rs`:

```rust
#[test]
fn command_exists_finds_sh() {
    // /bin/sh exists on every Unix system
    assert!(super::command_exists("sh"));
}

#[test]
fn command_exists_rejects_nonexistent() {
    assert!(!super::command_exists("owlry_nonexistent_binary_abc123"));
}
```

- [ ] **Step 2: Run test to verify it passes with the current `which`-based implementation**

Run: `cargo test -p owlry-core command_exists`

Expected: Both tests PASS (the current implementation works, just slowly).

- [ ] **Step 3: Replace `command_exists` with in-process PATH scan**

Replace lines 525-532 of `crates/owlry-core/src/config/mod.rs`:

```rust
/// Check if a command exists in PATH
fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

With:

```rust
/// Check if a command exists in PATH (in-process, no subprocess spawning)
fn command_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let full = dir.join(cmd);
                full.is_file()
            })
        })
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run tests to verify the new implementation works**

Run: `cargo test -p owlry-core command_exists`

Expected: Both tests PASS.

- [ ] **Step 5: Run full check**

Run: `cargo check -p owlry-core`

Expected: No errors or warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/owlry-core/src/config/mod.rs
git commit -m "perf(config): replace which subprocesses with in-process PATH scan

detect_terminal() was spawning up to 17 'which' subprocesses sequentially
on every startup. Replace with std::env::split_paths + is_file() check.
Eliminates 200-500ms of fork+exec overhead on cold cache."
```

---

### Task 2: Defer initial `update_results("")` to after `window.present()`

`MainWindow::new()` calls `update_results("")` at line 227, which does a synchronous blocking IPC round-trip before the window is even shown. Moving this to a post-`present()` idle callback makes the window appear immediately.

**Files:**
- Modify: `crates/owlry/src/ui/main_window.rs:225-227`
- Modify: `crates/owlry/src/app.rs:108-137`

- [ ] **Step 1: Remove `update_results("")` from `MainWindow::new()`**

In `crates/owlry/src/ui/main_window.rs`, remove line 227 so that lines 225-230 become:

```rust
        main_window.setup_signals();
        main_window.setup_lazy_loading();

        // Ensure search entry has focus when window is shown
        main_window.search_entry.grab_focus();
```

The `update_results("")` call (previously line 227) is removed entirely from the constructor.

- [ ] **Step 2: Add a public method to trigger initial population**

Add this method to `impl MainWindow` (after `update_results`):

```rust
    /// Schedule initial results population via idle callback.
    /// Call this AFTER `window.present()` so the window appears immediately.
    pub fn schedule_initial_results(&self) {
        let backend = self.backend.clone();
        let results_list = self.results_list.clone();
        let config = self.config.clone();
        let filter = self.filter.clone();
        let current_results = self.current_results.clone();
        let lazy_state = self.lazy_state.clone();

        gtk4::glib::idle_add_local_once(move || {
            let cfg = config.borrow();
            let max_results = cfg.general.max_results;
            drop(cfg);

            let results = backend.borrow_mut().search(
                "",
                max_results,
                &filter.borrow(),
                &config.borrow(),
            );

            // Clear existing results
            while let Some(child) = results_list.first_child() {
                results_list.remove(&child);
            }

            let initial_count = INITIAL_RESULTS.min(results.len());
            {
                let mut lazy = lazy_state.borrow_mut();
                lazy.all_results = results.clone();
                lazy.displayed_count = initial_count;
            }

            for item in results.iter().take(initial_count) {
                let row = ResultRow::new(item);
                results_list.append(&row);
            }

            if let Some(first_row) = results_list.row_at_index(0) {
                results_list.select_row(Some(&first_row));
            }

            *current_results.borrow_mut() =
                results.into_iter().take(initial_count).collect();
        });
    }
```

- [ ] **Step 3: Call `schedule_initial_results()` after `present()` in `app.rs`**

In `crates/owlry/src/app.rs`, change lines 108-137 from:

```rust
        let window = MainWindow::new(
            app,
            config.clone(),
            backend.clone(),
            filter.clone(),
            args.prompt.clone(),
        );

        // Set up layer shell for Wayland overlay behavior
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

        // Anchor to all edges for centered overlay effect
        // We'll use margins to control the actual size
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, false);

        // Position from top
        window.set_margin(Edge::Top, 200);

        // Set up icon theme fallbacks
        Self::setup_icon_theme();

        // Load CSS styling with config for theming
        Self::load_css(&config.borrow());

        window.present();
```

To:

```rust
        let window = MainWindow::new(
            app,
            config.clone(),
            backend.clone(),
            filter.clone(),
            args.prompt.clone(),
        );

        // Set up layer shell for Wayland overlay behavior
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

        // Anchor to all edges for centered overlay effect
        // We'll use margins to control the actual size
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, false);

        // Position from top
        window.set_margin(Edge::Top, 200);

        // Set up icon theme fallbacks
        Self::setup_icon_theme();

        // Load CSS styling with config for theming
        Self::load_css(&config.borrow());

        window.present();

        // Populate results AFTER present() so the window appears immediately
        window.schedule_initial_results();
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p owlry`

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry/src/ui/main_window.rs crates/owlry/src/app.rs
git commit -m "perf(ui): defer initial query to after window.present()

update_results('') was called inside MainWindow::new(), blocking the
window from appearing until the daemon responded. Move it to a
glib::idle_add_local_once callback scheduled after present() so the
window renders immediately."
```

---

### Task 3: Move search IPC off the GTK main thread

This is the most impactful fix. Currently the debounce handler calls `backend.borrow_mut().search_with_tag()` synchronously on the GTK main thread, freezing the UI for the entire IPC round-trip. The fix uses `std::thread::spawn` to run the query on a background thread and `glib::MainContext::channel()` to post results back.

**Files:**
- Modify: `crates/owlry/src/backend.rs`
- Modify: `crates/owlry/src/ui/main_window.rs:51,53-77,575-728,1186-1223`

- [ ] **Step 1: Add `Send`-safe query parameters struct to `backend.rs`**

Add at the top of `crates/owlry/src/backend.rs`, after the existing imports:

```rust
use std::sync::{Arc, Mutex};

/// Parameters needed to run a search query on a background thread.
/// Extracted from Rc<RefCell<..>> state so it can cross thread boundaries.
pub struct QueryParams {
    pub query: String,
    pub max_results: usize,
    pub modes: Option<Vec<String>>,
    pub tag_filter: Option<String>,
}

/// Result of an async search, sent back to the main thread.
pub struct QueryResult {
    /// The query string that produced these results (for staleness detection).
    pub query: String,
    pub items: Vec<LaunchItem>,
}
```

- [ ] **Step 2: Add `DaemonHandle` type for thread-safe async queries**

Add below the `QueryResult` struct in `crates/owlry/src/backend.rs`:

```rust
/// Thread-safe handle to the daemon IPC connection.
/// Wraps the CoreClient in Arc<Mutex<>> so it can be sent to a worker thread.
pub struct DaemonHandle {
    client: Arc<Mutex<CoreClient>>,
}

impl DaemonHandle {
    pub fn new(client: CoreClient) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
        }
    }

    /// Run a query on a background thread. Results are sent via the callback
    /// on the glib main context (GTK main thread).
    pub fn query_async<F>(&self, params: QueryParams, on_done: F)
    where
        F: FnOnce(QueryResult) + Send + 'static,
    {
        let client = Arc::clone(&self.client);
        let query_for_result = params.query.clone();

        std::thread::spawn(move || {
            let items = match client.lock() {
                Ok(mut c) => {
                    let effective_query = if let Some(ref tag) = params.tag_filter {
                        format!(":tag:{} {}", tag, params.query)
                    } else {
                        params.query
                    };
                    match c.query(&effective_query, params.modes) {
                        Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                        Err(e) => {
                            warn!("IPC query failed: {}", e);
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to lock daemon client: {}", e);
                    Vec::new()
                }
            };

            on_done(QueryResult {
                query: query_for_result,
                items,
            });
        });
    }
}
```

- [ ] **Step 3: Update `SearchBackend` enum to use `DaemonHandle`**

In `crates/owlry/src/backend.rs`, change the enum definition:

```rust
pub enum SearchBackend {
    /// Async IPC handle to owlry-core daemon
    Daemon(DaemonHandle),
    /// Direct local provider manager (dmenu mode only)
    Local {
        providers: Box<ProviderManager>,
        frecency: FrecencyStore,
    },
}
```

- [ ] **Step 4: Add `build_modes_param` helper and `query_async` method to `SearchBackend`**

Add these methods to `impl SearchBackend`, keeping the existing synchronous methods intact for local-mode use:

```rust
    /// Build the modes parameter for IPC from a ProviderFilter.
    fn build_modes_param(filter: &ProviderFilter) -> Option<Vec<String>> {
        if filter.is_accept_all() {
            None
        } else {
            let modes: Vec<String> = filter
                .enabled_providers()
                .iter()
                .map(|p| p.to_string())
                .collect();
            if modes.is_empty() { None } else { Some(modes) }
        }
    }

    /// Dispatch an async search query (daemon mode only).
    /// For local mode, returns None — caller should use synchronous search.
    pub fn query_async<F>(
        &self,
        query: &str,
        max_results: usize,
        filter: &ProviderFilter,
        config: &Config,
        tag_filter: Option<&str>,
        on_done: F,
    ) -> bool
    where
        F: FnOnce(QueryResult) + Send + 'static,
    {
        match self {
            SearchBackend::Daemon(handle) => {
                let params = QueryParams {
                    query: query.to_string(),
                    max_results,
                    modes: Self::build_modes_param(filter),
                    tag_filter: tag_filter.map(|s| s.to_string()),
                };
                handle.query_async(params, on_done);
                true // async dispatched
            }
            SearchBackend::Local { .. } => false, // caller should use sync path
        }
    }
```

- [ ] **Step 5: Update existing synchronous methods to use `build_modes_param`**

Refactor `search()` and `search_with_tag()` to use the shared helper. In the `Daemon` arms, replace the inline modes computation with `Self::build_modes_param(filter)`. The `Daemon` arm now needs to lock the client:

```rust
    pub fn search(
        &mut self,
        query: &str,
        max_results: usize,
        filter: &ProviderFilter,
        config: &Config,
    ) -> Vec<LaunchItem> {
        match self {
            SearchBackend::Daemon(handle) => {
                let modes_param = Self::build_modes_param(filter);
                match handle.client.lock() {
                    Ok(mut client) => match client.query(query, modes_param) {
                        Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                        Err(e) => {
                            warn!("IPC query failed: {}", e);
                            Vec::new()
                        }
                    },
                    Err(e) => {
                        warn!("Failed to lock daemon client: {}", e);
                        Vec::new()
                    }
                }
            }
            SearchBackend::Local {
                providers,
                frecency,
            } => {
                let frecency_weight = config.providers.frecency_weight;
                let use_frecency = config.providers.frecency;

                if use_frecency {
                    providers
                        .search_with_frecency(
                            query,
                            max_results,
                            filter,
                            frecency,
                            frecency_weight,
                            None,
                        )
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                } else {
                    providers
                        .search_filtered(query, max_results, filter)
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                }
            }
        }
    }
```

Apply the same pattern to `search_with_tag()`, `record_launch()`, `query_submenu_actions()`, `execute_plugin_action()`, `available_provider_ids()` — each `Daemon` arm should use `handle.client.lock()` instead of accessing a bare `CoreClient`. Keep the logic identical, just wrap in `lock()`.

- [ ] **Step 6: Update `app.rs` to construct `DaemonHandle`**

In `crates/owlry/src/app.rs`, change line 72:

```rust
                    SearchBackend::Daemon(client)
```

To:

```rust
                    SearchBackend::Daemon(crate::backend::DaemonHandle::new(client))
```

- [ ] **Step 7: Update debounce handler in `main_window.rs` to use async search**

In `crates/owlry/src/ui/main_window.rs`, replace the debounce closure body (lines 682-724) with an async-aware version. The key change: instead of calling `backend.borrow_mut().search_with_tag()` synchronously and then updating the list, we attempt `query_async()` first and fall back to sync for local mode:

```rust
            let source_id = gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS),
                move || {
                    // Clear the source ID since we're now executing
                    *debounce_source_for_closure.borrow_mut() = None;

                    let cfg = config.borrow();
                    let max_results = cfg.general.max_results;
                    drop(cfg);

                    let query_str = parsed.query.clone();
                    let tag = parsed.tag_filter.clone();

                    // Try async path (daemon mode)
                    let dispatched = {
                        let be = backend.borrow();
                        let f = filter.borrow();
                        let c = config.borrow();

                        // Clone what we need for the callback
                        let results_list_cb = results_list.clone();
                        let current_results_cb = current_results.clone();
                        let lazy_state_cb = lazy_state.clone();

                        be.query_async(
                            &query_str,
                            max_results,
                            &f,
                            &c,
                            tag.as_deref(),
                            move |result| {
                                // This closure runs on a background thread.
                                // Post UI update back to the main thread.
                                gtk4::glib::idle_add_local_once(move || {
                                    // Clear existing results
                                    while let Some(child) = results_list_cb.first_child() {
                                        results_list_cb.remove(&child);
                                    }

                                    let initial_count =
                                        INITIAL_RESULTS.min(result.items.len());
                                    {
                                        let mut lazy = lazy_state_cb.borrow_mut();
                                        lazy.all_results = result.items.clone();
                                        lazy.displayed_count = initial_count;
                                    }

                                    for item in result.items.iter().take(initial_count) {
                                        let row = ResultRow::new(item);
                                        results_list_cb.append(&row);
                                    }

                                    if let Some(first_row) = results_list_cb.row_at_index(0)
                                    {
                                        results_list_cb.select_row(Some(&first_row));
                                    }

                                    *current_results_cb.borrow_mut() = result
                                        .items
                                        .into_iter()
                                        .take(initial_count)
                                        .collect();
                                });
                            },
                        )
                    };

                    if !dispatched {
                        // Local mode (dmenu): synchronous search
                        let results = backend.borrow_mut().search_with_tag(
                            &query_str,
                            max_results,
                            &filter.borrow(),
                            &config.borrow(),
                            tag.as_deref(),
                        );

                        while let Some(child) = results_list.first_child() {
                            results_list.remove(&child);
                        }

                        let initial_count = INITIAL_RESULTS.min(results.len());
                        {
                            let mut lazy = lazy_state.borrow_mut();
                            lazy.all_results = results.clone();
                            lazy.displayed_count = initial_count;
                        }

                        for item in results.iter().take(initial_count) {
                            let row = ResultRow::new(item);
                            results_list.append(&row);
                        }

                        if let Some(first_row) = results_list.row_at_index(0) {
                            results_list.select_row(Some(&first_row));
                        }

                        *current_results.borrow_mut() =
                            results.into_iter().take(initial_count).collect();
                    }
                },
            );
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p owlry`

Expected: No errors. There may be warnings about unused imports in `backend.rs` — fix those if they appear.

- [ ] **Step 9: Run existing tests**

Run: `cargo test -p owlry`

Expected: All existing tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/owlry/src/backend.rs crates/owlry/src/ui/main_window.rs crates/owlry/src/app.rs
git commit -m "perf(ui): move search IPC off the GTK main thread

Search queries in daemon mode now run on a background thread via
DaemonHandle::query_async(). Results are posted back to the main
thread via glib::idle_add_local_once. The GTK event loop is never
blocked by IPC, eliminating perceived input lag.

Local mode (dmenu) continues to use synchronous search since it
has no IPC overhead."
```

---

### Task 4: Sample `Utc::now()` once per search instead of per-item

`FrecencyStore::get_score()` calls `Utc::now()` inside `calculate_frecency()` on every item. With hundreds of items per query, that's hundreds of unnecessary syscalls. Sample the timestamp once and pass it in.

**Files:**
- Modify: `crates/owlry-core/src/data/frecency.rs:125-153`
- Modify: `crates/owlry-core/src/providers/mod.rs:636,685`

- [ ] **Step 1: Write test for the new `get_score_at` method**

Add to the `#[cfg(test)]` block in `crates/owlry-core/src/data/frecency.rs`:

```rust
    #[test]
    fn get_score_at_matches_get_score() {
        let mut store = FrecencyStore {
            data: FrecencyData {
                version: 1,
                entries: HashMap::new(),
            },
            path: PathBuf::from("/dev/null"),
            dirty: false,
        };
        store.data.entries.insert(
            "test".to_string(),
            FrecencyEntry {
                launch_count: 5,
                last_launch: Utc::now(),
            },
        );

        let now = Utc::now();
        let score_at = store.get_score_at("test", now);
        let score = store.get_score("test");

        // Both should be very close (same timestamp, within rounding)
        assert!((score_at - score).abs() < 1.0);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p owlry-core get_score_at`

Expected: FAIL — `get_score_at` method does not exist yet.

- [ ] **Step 3: Add `get_score_at` and `calculate_frecency_at` methods**

In `crates/owlry-core/src/data/frecency.rs`, add after the existing `get_score` method (line 132):

```rust
    /// Calculate frecency score using a pre-sampled timestamp.
    /// Use this in hot loops to avoid repeated Utc::now() syscalls.
    pub fn get_score_at(&self, item_id: &str, now: DateTime<Utc>) -> f64 {
        match self.data.entries.get(item_id) {
            Some(entry) => Self::calculate_frecency_at(entry.launch_count, entry.last_launch, now),
            None => 0.0,
        }
    }
```

Then add after `calculate_frecency` (line 154):

```rust
    /// Calculate frecency using a caller-provided timestamp.
    fn calculate_frecency_at(launch_count: u32, last_launch: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
        let age = now.signed_duration_since(last_launch);
        let age_days = age.num_hours() as f64 / 24.0;

        let recency_weight = if age_days < 1.0 {
            100.0
        } else if age_days < 7.0 {
            70.0
        } else if age_days < 30.0 {
            50.0
        } else if age_days < 90.0 {
            30.0
        } else {
            10.0
        };

        launch_count as f64 * recency_weight
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p owlry-core get_score_at`

Expected: PASS.

- [ ] **Step 5: Update `search_with_frecency` to sample time once**

In `crates/owlry-core/src/providers/mod.rs`, add `use chrono::Utc;` to the imports at the top of the file if not already present.

Then in `search_with_frecency`, just before the `score_item` closure definition (around line 650), add:

```rust
        let now = Utc::now();
```

Change the `score_item` closure's frecency line (line 685):

```rust
                let frecency_score = frecency.get_score(&item.id);
```

To:

```rust
                let frecency_score = frecency.get_score_at(&item.id, now);
```

Also change the empty-query frecency path (line 636):

```rust
                    let frecency_score = frecency.get_score(&item.id);
```

To:

```rust
                    let frecency_score = frecency.get_score_at(&item.id, now);
```

And add `let now = Utc::now();` before the empty-query block (around line 610) as well.

- [ ] **Step 6: Run all tests**

Run: `cargo test -p owlry-core`

Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/owlry-core/src/data/frecency.rs crates/owlry-core/src/providers/mod.rs
git commit -m "perf(core): sample Utc::now() once per search instead of per-item

get_score() called Utc::now() inside calculate_frecency() for every
item in the search loop. Added get_score_at() that accepts a pre-sampled
timestamp. Eliminates hundreds of unnecessary clock_gettime syscalls
per keystroke."
```

---

### Task 5: Eliminate redundant `results.clone()` in search handlers

Both `update_results()` and the debounce handler clone the full results Vec into `lazy_state.all_results` and then also consume it for `current_results`. The clone is unnecessary — we can split the Vec instead.

**Files:**
- Modify: `crates/owlry/src/ui/main_window.rs:1186-1223` (`update_results`)
- Modify: `crates/owlry/src/ui/main_window.rs:703-723` (debounce handler)

- [ ] **Step 1: Refactor `update_results` to avoid clone**

In `crates/owlry/src/ui/main_window.rs`, replace the `update_results` method body (lines 1186-1223):

```rust
    fn update_results(&self, query: &str) {
        let cfg = self.config.borrow();
        let max_results = cfg.general.max_results;
        drop(cfg);

        let results = self.backend.borrow_mut().search(
            query,
            max_results,
            &self.filter.borrow(),
            &self.config.borrow(),
        );

        // Clear existing results
        while let Some(child) = self.results_list.first_child() {
            self.results_list.remove(&child);
        }

        let initial_count = INITIAL_RESULTS.min(results.len());

        // Display initial batch
        for item in results.iter().take(initial_count) {
            let row = ResultRow::new(item);
            self.results_list.append(&row);
        }

        if let Some(first_row) = self.results_list.row_at_index(0) {
            self.results_list.select_row(Some(&first_row));
        }

        // Split: current_results gets the displayed slice, lazy gets everything
        *self.current_results.borrow_mut() = results[..initial_count].to_vec();
        let mut lazy = self.lazy_state.borrow_mut();
        lazy.all_results = results;
        lazy.displayed_count = initial_count;
    }
```

- [ ] **Step 2: Apply the same pattern to the debounce handler's local-mode path**

In the synchronous fallback path of the debounce handler (the `if !dispatched` block from Task 3), apply the same pattern — replace:

```rust
                        let initial_count = INITIAL_RESULTS.min(results.len());
                        {
                            let mut lazy = lazy_state.borrow_mut();
                            lazy.all_results = results.clone();
                            lazy.displayed_count = initial_count;
                        }
                        // ...
                        *current_results.borrow_mut() =
                            results.into_iter().take(initial_count).collect();
```

With:

```rust
                        let initial_count = INITIAL_RESULTS.min(results.len());

                        for item in results.iter().take(initial_count) {
                            let row = ResultRow::new(item);
                            results_list.append(&row);
                        }

                        if let Some(first_row) = results_list.row_at_index(0) {
                            results_list.select_row(Some(&first_row));
                        }

                        *current_results.borrow_mut() = results[..initial_count].to_vec();
                        let mut lazy = lazy_state.borrow_mut();
                        lazy.all_results = results;
                        lazy.displayed_count = initial_count;
```

- [ ] **Step 2b: Apply the same pattern in the async callback (Task 3)**

In the `on_done` callback within the debounce handler's async path, replace:

```rust
                                    {
                                        let mut lazy = lazy_state_cb.borrow_mut();
                                        lazy.all_results = result.items.clone();
                                        lazy.displayed_count = initial_count;
                                    }
                                    // ...
                                    *current_results_cb.borrow_mut() = result
                                        .items
                                        .into_iter()
                                        .take(initial_count)
                                        .collect();
```

With:

```rust
                                    *current_results_cb.borrow_mut() =
                                        result.items[..initial_count].to_vec();
                                    let mut lazy = lazy_state_cb.borrow_mut();
                                    lazy.all_results = result.items;
                                    lazy.displayed_count = initial_count;
```

- [ ] **Step 3: Apply the same pattern in `schedule_initial_results` (from Task 2)**

In the `schedule_initial_results` method added in Task 2, use the same approach:

```rust
            *current_results.borrow_mut() = results[..initial_count].to_vec();
            let mut lazy = lazy_state.borrow_mut();
            lazy.all_results = results;
            lazy.displayed_count = initial_count;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p owlry`

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry/src/ui/main_window.rs
git commit -m "perf(ui): eliminate redundant results.clone() in search handlers

The full results Vec was cloned into lazy_state.all_results and then
separately consumed for current_results. Now we slice for current_results
and move the original into lazy_state, avoiding one full Vec allocation
per query."
```

---

### Task 6: Use `displayed_count` in `scroll_to_row` instead of child walk

`scroll_to_row` walks all GTK children with `first_child()`/`next_sibling()` to count rows. The count is already tracked in `lazy_state.displayed_count`.

**Files:**
- Modify: `crates/owlry/src/ui/main_window.rs:460-492`

- [ ] **Step 1: Add `lazy_state` parameter to `scroll_to_row`**

Change the method signature and body in `crates/owlry/src/ui/main_window.rs`:

```rust
    fn scroll_to_row(
        scrolled: &ScrolledWindow,
        results_list: &ListBox,
        row: &ListBoxRow,
        lazy_state: &Rc<RefCell<LazyLoadState>>,
    ) {
        let vadj = scrolled.vadjustment();

        let row_index = row.index();
        if row_index < 0 {
            return;
        }

        let visible_height = vadj.page_size();
        let current_scroll = vadj.value();

        let list_height = results_list.height() as f64;
        let row_count = lazy_state.borrow().displayed_count.max(1) as f64;

        let row_height = list_height / row_count;
        let row_top = row_index as f64 * row_height;
        let row_bottom = row_top + row_height;

        if row_top < current_scroll {
            vadj.set_value(row_top);
        } else if row_bottom > current_scroll + visible_height {
            vadj.set_value(row_bottom - visible_height);
        }
    }
```

- [ ] **Step 2: Update all call sites to pass `lazy_state`**

In `setup_signals()`, the `lazy_state` Rc is already cloned into the key handler closure's scope. Update the `Key::Down` and `Key::Up` arms. Both currently call:

```rust
Self::scroll_to_row(&scrolled, &results_list, &next_row);
```

These need to be updated to include the lazy_state. First, ensure `lazy_state` is cloned into the key controller closure. Find the block of clones before `key_controller.connect_key_pressed` and add `lazy_state` if not already there:

```rust
        let lazy_state_for_keys = self.lazy_state.clone();
```

Then update the calls in `Key::Down` and `Key::Up`:

```rust
// In Key::Down:
Self::scroll_to_row(&scrolled, &results_list, &next_row, &lazy_state_for_keys);

// In Key::Up:
Self::scroll_to_row(&scrolled, &results_list, &prev_row, &lazy_state_for_keys);
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p owlry`

Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add crates/owlry/src/ui/main_window.rs
git commit -m "perf(ui): use tracked count in scroll_to_row instead of child walk

scroll_to_row walked all GTK children via first_child/next_sibling
to count rows. The count is already available in LazyLoadState, so
use that directly. Eliminates O(n) widget traversal per arrow key."
```

---

## Execution Notes

### Task dependency order

Tasks 1, 4, and 6 are independent of each other and can be done in any order or in parallel.

Task 2 should be done before Task 3 (Task 3's async code affects the same area).

Task 5 depends on Task 3 being complete (it references the async handler's local-mode fallback).

**Recommended execution order:** 1 → 4 → 6 → 2 → 3 → 5

### What's NOT in this plan

These are real issues identified during analysis but are larger refactors that deserve their own plans:

- **Rune VM-per-query overhead** (`owlry-rune`): Creating a new Rune VM on every `query_provider` call is wasteful but fixing it requires changes to the plugin runtime ABI.
- **Sequential plugin loading at daemon startup**: Parallelizing plugin discovery/init requires careful handling of the `ProviderManager` initialization order.
- **Hot-reload write lock blocking all queries**: The `reload_runtimes()` exclusive lock pattern needs architectural discussion about whether to use double-buffering or copy-on-write.
- **5-second periodic re-query timer**: Currently fires a full IPC search unconditionally. Could be replaced with a lightweight "has anything changed?" check, but requires a new IPC message type.
