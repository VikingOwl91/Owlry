# Performance Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix an unsound `unsafe` block, eliminate per-keystroke clone avalanches in the search path, and apply targeted I/O and algorithmic optimizations across both the `owlry` and `owlry-plugins` repos.

**Architecture:** Nine tasks across two repos. Phase 1 removes unsound `unsafe` code. Phase 2 restructures the hot search path to score by reference and clone only the final top-N results, combined with partial-sort (`select_nth_unstable_by`) for O(n) selection. Phase 3 removes unnecessary blocking I/O and simplifies GTK list updates. Phase 4 applies minor algorithmic fixes. Phase 5 covers plugin-repo fixes (separate repo, separate branch).

**Tech Stack:** Rust 1.90+, GTK4 4.12+, owlry-core, owlry-plugins

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/owlry-core/src/providers/native_provider.rs` | Modify | Remove `RwLock<Vec<LaunchItem>>`, eliminate `unsafe` block |
| `crates/owlry-core/src/providers/mod.rs` | Modify | Score-by-reference in `search_with_frecency`, partial sort, clone only top N |
| `crates/owlry-core/src/data/frecency.rs` | Modify | Remove auto-save in `record_launch` |
| `crates/owlry/src/ui/main_window.rs` | Modify | Replace child-removal loop with `remove_all()` |
| `crates/owlry-core/src/providers/application.rs` | Modify | Single-pass double-space cleanup |
| `crates/owlry-core/src/filter.rs` | Modify | Merge full and partial prefix arrays into single-pass lookup |

---

## Phase 1: Safety & Correctness

### Task 1: Remove unsound `unsafe` in NativeProvider::items()

The `items()` implementation creates a raw pointer from an `RwLockReadGuard`, then drops the guard while returning a slice backed by that pointer. This is UB waiting to happen. The inner `RwLock` is unnecessary — `refresh()` takes `&mut self` (exclusive access guaranteed by the outer `Arc<RwLock<ProviderManager>>`), and `items()` takes `&self`. Replace the `RwLock<Vec<LaunchItem>>` with a plain `Vec<LaunchItem>`.

**Files:**
- Modify: `crates/owlry-core/src/providers/native_provider.rs`
- Test: `crates/owlry-core/src/providers/native_provider.rs` (existing tests)

- [ ] **Step 1: Run existing tests to establish baseline**

Run: `cargo test -p owlry-core`

Expected: All tests PASS.

- [ ] **Step 2: Replace `RwLock<Vec<LaunchItem>>` with `Vec<LaunchItem>` in the struct**

In `crates/owlry-core/src/providers/native_provider.rs`, change the struct definition:

```rust
pub struct NativeProvider {
    plugin: Arc<NativePlugin>,
    info: ProviderInfo,
    handle: ProviderHandle,
    items: Vec<LaunchItem>,
}
```

Update `new()` to initialize without RwLock:

```rust
Self {
    plugin,
    info,
    handle,
    items: Vec::new(),
}
```

- [ ] **Step 3: Remove the `unsafe` block from `items()` — return `&self.items` directly**

Replace the entire `Provider::items()` impl:

```rust
fn items(&self) -> &[LaunchItem] {
    &self.items
}
```

- [ ] **Step 4: Update `refresh()` to write directly to `self.items`**

Replace the RwLock write in `refresh()`:

```rust
// Was: *self.items.write().unwrap() = items;
self.items = items;
```

- [ ] **Step 5: Update `query()` to read from `self.items` directly**

In the `query()` method, replace the RwLock read:

```rust
// Was: return self.items.read().unwrap().clone();
return self.items.clone();
```

- [ ] **Step 6: Remove `use std::sync::RwLock;` (no longer needed, `Arc` still used for plugin)**

Remove `RwLock` from the `use std::sync::{Arc, RwLock};` import:

```rust
use std::sync::Arc;
```

- [ ] **Step 7: Run tests and check**

Run: `cargo test -p owlry-core && cargo check -p owlry-core`

Expected: All tests PASS, no warnings about unused imports.

- [ ] **Step 8: Compile full workspace to verify no downstream breakage**

Run: `cargo check --workspace`

Expected: Clean compilation.

- [ ] **Step 9: Commit**

```bash
git add crates/owlry-core/src/providers/native_provider.rs
git commit -m "fix(native-provider): remove unsound unsafe in items()

Replace RwLock<Vec<LaunchItem>> with plain Vec. The inner RwLock
was unnecessary — refresh() takes &mut self (exclusive access
guaranteed by the outer Arc<RwLock<ProviderManager>>). The unsafe
block in items() dropped the RwLockReadGuard while returning a
slice backed by the raw pointer, creating a dangling reference."
```

---

## Phase 2: Hot Path Optimization

### Task 2: Eliminate clone avalanche in search_with_frecency

Currently every matching `LaunchItem` (5 Strings + Vec) is cloned during scoring, then the Vec is sorted and truncated to ~15 results — discarding 95%+ of clones. Refactor to score items by reference, partial-sort the lightweight `(&LaunchItem, i64)` tuples with `select_nth_unstable_by` (O(n) average), and clone only the top-N survivors.

**Files:**
- Modify: `crates/owlry-core/src/providers/mod.rs` — `search_with_frecency` method (~lines 652-875)
- Test: existing tests in `crates/owlry-core/src/providers/mod.rs`

- [ ] **Step 1: Run existing search tests to establish baseline**

Run: `cargo test -p owlry-core search`

Expected: All search-related tests PASS.

- [ ] **Step 2: Refactor `score_item` closure to return `Option<i64>` instead of `Option<(LaunchItem, i64)>`**

In `search_with_frecency`, change the closure at ~line 780 from:

```rust
let score_item = |item: &LaunchItem| -> Option<(LaunchItem, i64)> {
```

To:

```rust
let score_item = |item: &LaunchItem| -> Option<i64> {
```

And change the return from:

```rust
base_score.map(|s| {
    let frecency_score = frecency.get_score_at(&item.id, now);
    let frecency_boost = (frecency_score * frecency_weight * 10.0) as i64;
    let exact_match_boost = if item.name.eq_ignore_ascii_case(query) {
        match &item.provider {
            ProviderType::Application => 50_000,
            _ => 30_000,
        }
    } else {
        0
    };
    (item.clone(), s + frecency_boost + exact_match_boost)
})
```

To:

```rust
base_score.map(|s| {
    let frecency_score = frecency.get_score_at(&item.id, now);
    let frecency_boost = (frecency_score * frecency_weight * 10.0) as i64;
    let exact_match_boost = if item.name.eq_ignore_ascii_case(query) {
        match &item.provider {
            ProviderType::Application => 50_000,
            _ => 30_000,
        }
    } else {
        0
    };
    s + frecency_boost + exact_match_boost
})
```

- [ ] **Step 3: Replace the static-item scoring loops to collect references**

Replace the scoring loops at ~lines 831-853:

```rust
// Was:
// for provider in &self.providers { ... results.push(scored); }
// for provider in &self.static_native_providers { ... results.push(scored); }

// Score static items by reference (no cloning)
let mut scored_refs: Vec<(&LaunchItem, i64)> = Vec::new();

for provider in &self.providers {
    if !filter.is_active(provider.provider_type()) {
        continue;
    }
    for item in provider.items() {
        if let Some(score) = score_item(item) {
            scored_refs.push((item, score));
        }
    }
}

for provider in &self.static_native_providers {
    if !filter.is_active(provider.provider_type()) {
        continue;
    }
    for item in provider.items() {
        if let Some(score) = score_item(item) {
            scored_refs.push((item, score));
        }
    }
}

// Partial sort: O(n) average to find top max_results, then O(k log k) to order them
if scored_refs.len() > max_results {
    scored_refs.select_nth_unstable_by(max_results, |a, b| b.1.cmp(&a.1));
    scored_refs.truncate(max_results);
}
scored_refs.sort_by(|a, b| b.1.cmp(&a.1));

// Clone only the survivors
results.extend(scored_refs.into_iter().map(|(item, score)| (item.clone(), score)));
```

- [ ] **Step 4: Add final merge-sort for dynamic + static results**

After extending results, add the final sort (dynamic results from earlier in the function + the newly added static results need unified ordering):

```rust
// Final sort merges dynamic results with static top-N
results.sort_by(|a, b| b.1.cmp(&a.1));
results.truncate(max_results);
```

This replaces the existing `results.sort_by(...)` and `results.truncate(...)` lines at ~854-855 — the logic is the same, just confirming it's still present after the refactor.

- [ ] **Step 5: Optimize the empty-query path to score by reference too**

Replace the empty-query block at ~lines 739-776. Change:

```rust
let core_items = self
    .providers
    .iter()
    .filter(|p| filter.is_active(p.provider_type()))
    .flat_map(|p| p.items().iter().cloned());

let native_items = self
    .static_native_providers
    .iter()
    .filter(|p| filter.is_active(p.provider_type()))
    .flat_map(|p| p.items().iter().cloned());

let items: Vec<(LaunchItem, i64)> = core_items
    .chain(native_items)
    .filter(|item| {
        if let Some(tag) = tag_filter {
            item.tags.iter().any(|t| t.to_lowercase().contains(tag))
        } else {
            true
        }
    })
    .map(|item| {
        let frecency_score = frecency.get_score_at(&item.id, now);
        let boosted = (frecency_score * frecency_weight * 100.0) as i64;
        (item, boosted)
    })
    .collect();

results.extend(items);
results.sort_by(|a, b| b.1.cmp(&a.1));
results.truncate(max_results);
return results;
```

With:

```rust
let mut scored_refs: Vec<(&LaunchItem, i64)> = self
    .providers
    .iter()
    .filter(|p| filter.is_active(p.provider_type()))
    .flat_map(|p| p.items().iter())
    .chain(
        self.static_native_providers
            .iter()
            .filter(|p| filter.is_active(p.provider_type()))
            .flat_map(|p| p.items().iter()),
    )
    .filter(|item| {
        if let Some(tag) = tag_filter {
            item.tags.iter().any(|t| t.to_lowercase().contains(tag))
        } else {
            true
        }
    })
    .map(|item| {
        let frecency_score = frecency.get_score_at(&item.id, now);
        let boosted = (frecency_score * frecency_weight * 100.0) as i64;
        (item, boosted)
    })
    .collect();

if scored_refs.len() > max_results {
    scored_refs.select_nth_unstable_by(max_results, |a, b| b.1.cmp(&a.1));
    scored_refs.truncate(max_results);
}
scored_refs.sort_by(|a, b| b.1.cmp(&a.1));

results.extend(scored_refs.into_iter().map(|(item, score)| (item.clone(), score)));
results.sort_by(|a, b| b.1.cmp(&a.1));
results.truncate(max_results);
return results;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p owlry-core`

Expected: All tests PASS. Search results are identical (same items, same ordering).

- [ ] **Step 7: Compile full workspace**

Run: `cargo check --workspace`

Expected: Clean compilation.

- [ ] **Step 8: Commit**

```bash
git add crates/owlry-core/src/providers/mod.rs
git commit -m "perf(search): score by reference, clone only top-N results

Refactor search_with_frecency to score static provider items by
reference (&LaunchItem, i64) instead of cloning every match.
Use select_nth_unstable_by for O(n) partial sort, then clone
only the max_results survivors. Reduces clones from O(total_matches)
to O(max_results) — typically from hundreds to ~15."
```

---

## Phase 3: I/O Optimization

### Task 3: Remove frecency auto-save on every launch

`record_launch` calls `self.save()` synchronously — serializing JSON and writing to disk on every item launch. The `Drop` impl already saves on shutdown. Mark dirty and let the caller (or shutdown) handle persistence.

**Files:**
- Modify: `crates/owlry-core/src/data/frecency.rs:98-123`
- Test: `crates/owlry-core/src/data/frecency.rs` (existing + new)

- [ ] **Step 1: Write test verifying record_launch sets dirty without saving**

Add to the `#[cfg(test)]` block in `frecency.rs`:

```rust
#[test]
fn record_launch_sets_dirty_without_saving() {
    let mut store = FrecencyStore {
        data: FrecencyData::default(),
        path: PathBuf::from("/dev/null"),
        dirty: false,
    };

    store.record_launch("test-item");

    assert!(store.dirty, "record_launch should set dirty flag");
    assert_eq!(store.data.entries["test-item"].launch_count, 1);
}
```

- [ ] **Step 2: Run test to verify it fails (current code auto-saves, clearing dirty)**

Run: `cargo test -p owlry-core record_launch_sets_dirty`

Expected: FAIL — `store.dirty` is `false` because `save()` clears it.

- [ ] **Step 3: Remove the auto-save from `record_launch`**

In `crates/owlry-core/src/data/frecency.rs`, remove lines 119-122 from `record_launch`:

```rust
// Remove this block:
// Auto-save after recording
if let Err(e) = self.save() {
    warn!("Failed to save frecency data: {}", e);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p owlry-core record_launch_sets_dirty`

Expected: PASS.

- [ ] **Step 5: Run all frecency tests**

Run: `cargo test -p owlry-core frecency`

Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/owlry-core/src/data/frecency.rs
git commit -m "perf(frecency): remove blocking auto-save on every launch

record_launch no longer calls save() synchronously. The dirty flag
is set and the Drop impl flushes on shutdown. Removes a JSON
serialize + fs::write from the hot launch path."
```

---

### Task 4: GTK ListBox — replace child-removal loop with `remove_all()`

The current code removes children one-by-one in a `while` loop, triggering layout invalidation per removal. GTK 4.12 provides `remove_all()` which batches the operation.

**Files:**
- Modify: `crates/owlry/src/ui/main_window.rs` — two locations (~lines 705-707 and ~742-744)

- [ ] **Step 1: Replace daemon-mode child removal loop**

In the `spawn_future_local` async block (~line 705), replace:

```rust
while let Some(child) = results_list_cb.first_child() {
    results_list_cb.remove(&child);
}
```

With:

```rust
results_list_cb.remove_all();
```

- [ ] **Step 2: Replace local-mode (dmenu) child removal loop**

In the synchronous (local/dmenu) branch (~line 742), replace:

```rust
while let Some(child) = results_list.first_child() {
    results_list.remove(&child);
}
```

With:

```rust
results_list.remove_all();
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p owlry`

Expected: Clean compilation. `remove_all()` is available in gtk4 4.12+.

- [ ] **Step 4: Commit**

```bash
git add crates/owlry/src/ui/main_window.rs
git commit -m "perf(ui): use ListBox::remove_all() instead of per-child loop

Replaces two while-loop child removal patterns with the batched
remove_all() method available since GTK 4.12. Avoids per-removal
layout invalidation."
```

---

## Phase 4: Minor Optimizations

### Task 5: Single-pass double-space cleanup in application.rs

The `clean_desktop_exec_field` function uses a `while contains("  ") { replace("  ", " ") }` loop — O(n²) on pathological input with repeated allocations. Replace with a single-pass char iterator.

**Files:**
- Modify: `crates/owlry-core/src/providers/application.rs:60-64`
- Test: existing tests in `crates/owlry-core/src/providers/application.rs`

- [ ] **Step 1: Add test for pathological input**

Add to the existing `#[cfg(test)]` block:

```rust
#[test]
fn test_clean_desktop_exec_collapses_spaces() {
    assert_eq!(clean_desktop_exec_field("app   --flag    arg"), "app --flag arg");
    // Pathological: many consecutive spaces
    let input = format!("app{}arg", " ".repeat(100));
    assert_eq!(clean_desktop_exec_field(&input), "app arg");
}
```

- [ ] **Step 2: Run test to verify it passes with current implementation**

Run: `cargo test -p owlry-core clean_desktop_exec`

Expected: All PASS (current implementation works, just inefficiently).

- [ ] **Step 3: Replace the while-loop with a single-pass approach**

Replace lines 60-64 of `application.rs`:

```rust
// Was:
// let mut cleaned = result.trim().to_string();
// while cleaned.contains("  ") {
//     cleaned = cleaned.replace("  ", " ");
// }
// cleaned

let trimmed = result.trim();
let mut cleaned = String::with_capacity(trimmed.len());
let mut prev_space = false;
for c in trimmed.chars() {
    if c == ' ' {
        if !prev_space {
            cleaned.push(' ');
        }
        prev_space = true;
    } else {
        cleaned.push(c);
        prev_space = false;
    }
}
cleaned
```

- [ ] **Step 4: Run all application tests**

Run: `cargo test -p owlry-core clean_desktop_exec`

Expected: All PASS including the new pathological test.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/providers/application.rs
git commit -m "perf(application): single-pass double-space collapse

Replace while-contains-replace loop with a single-pass char
iterator. Eliminates O(n²) behavior and repeated allocations
on pathological desktop file Exec values."
```

---

### Task 6: Consolidate parse_query prefix matching into single pass

`parse_query` maintains four separate arrays (core_prefixes, plugin_prefixes, partial_core, partial_plugin) with duplicated prefix strings, iterating them in sequence. Merge full and partial matching into a single array and a single loop per category.

**Files:**
- Modify: `crates/owlry-core/src/filter.rs:202-406`
- Test: existing tests in `crates/owlry-core/src/filter.rs`

- [ ] **Step 1: Run existing filter tests to establish baseline**

Run: `cargo test -p owlry-core filter`

Expected: All PASS.

- [ ] **Step 2: Define a unified prefix entry struct and static arrays**

At the top of `parse_query`, replace the four separate arrays with two unified arrays:

```rust
pub fn parse_query(query: &str) -> ParsedQuery {
    let trimmed = query.trim_start();

    // Tag filter: ":tag:XXX query" — check first (unchanged)
    if let Some(rest) = trimmed.strip_prefix(":tag:") {
        if let Some(space_idx) = rest.find(' ') {
            let tag = rest[..space_idx].to_lowercase();
            let query_part = rest[space_idx + 1..].to_string();
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] parse_query({:?}) -> tag={:?}, query={:?}",
                query, tag, query_part
            );
            return ParsedQuery {
                prefix: None,
                tag_filter: Some(tag),
                query: query_part,
            };
        } else {
            let tag = rest.to_lowercase();
            return ParsedQuery {
                prefix: None,
                tag_filter: Some(tag),
                query: String::new(),
            };
        }
    }

    // Core prefixes — each entry is tried as ":prefix " (full) and ":prefix" (partial)
    const CORE_PREFIXES: &[(&str, fn() -> ProviderType)] = &[
        ("app", || ProviderType::Application),
        ("apps", || ProviderType::Application),
        ("cmd", || ProviderType::Command),
        ("command", || ProviderType::Command),
    ];

    // Plugin prefixes — each entry maps to a plugin type_id
    const PLUGIN_PREFIXES: &[(&str, &str)] = &[
        ("bm", "bookmarks"),
        ("bookmark", "bookmarks"),
        ("bookmarks", "bookmarks"),
        ("calc", "calc"),
        ("calculator", "calc"),
        ("clip", "clipboard"),
        ("clipboard", "clipboard"),
        ("emoji", "emoji"),
        ("emojis", "emoji"),
        ("file", "filesearch"),
        ("files", "filesearch"),
        ("find", "filesearch"),
        ("script", "scripts"),
        ("scripts", "scripts"),
        ("ssh", "ssh"),
        ("sys", "system"),
        ("system", "system"),
        ("power", "system"),
        ("uuctl", "uuctl"),
        ("systemd", "uuctl"),
        ("web", "websearch"),
        ("search", "websearch"),
        ("config", "config"),
        ("settings", "config"),
        ("conv", "conv"),
        ("converter", "conv"),
    ];

    // Single-pass: try each core prefix as both full (":prefix query") and partial (":prefix")
    for (name, make_provider) in CORE_PREFIXES {
        let with_space = format!(":{} ", name);
        if let Some(rest) = trimmed.strip_prefix(with_space.as_str()) {
            let provider = make_provider();
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] parse_query({:?}) -> prefix={:?}, query={:?}",
                query, provider, rest
            );
            return ParsedQuery {
                prefix: Some(provider),
                tag_filter: None,
                query: rest.to_string(),
            };
        }
        let exact = format!(":{}", name);
        if trimmed == exact {
            let provider = make_provider();
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] parse_query({:?}) -> partial prefix {:?}",
                query, provider
            );
            return ParsedQuery {
                prefix: Some(provider),
                tag_filter: None,
                query: String::new(),
            };
        }
    }

    // Single-pass: try each plugin prefix as both full and partial
    for (name, type_id) in PLUGIN_PREFIXES {
        let with_space = format!(":{} ", name);
        if let Some(rest) = trimmed.strip_prefix(with_space.as_str()) {
            let provider = ProviderType::Plugin(type_id.to_string());
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] parse_query({:?}) -> prefix={:?}, query={:?}",
                query, provider, rest
            );
            return ParsedQuery {
                prefix: Some(provider),
                tag_filter: None,
                query: rest.to_string(),
            };
        }
        let exact = format!(":{}", name);
        if trimmed == exact {
            let provider = ProviderType::Plugin(type_id.to_string());
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] parse_query({:?}) -> partial prefix {:?}",
                query, provider
            );
            return ParsedQuery {
                prefix: Some(provider),
                tag_filter: None,
                query: String::new(),
            };
        }
    }

    // Dynamic plugin prefix fallback (unchanged)
    if let Some(rest) = trimmed.strip_prefix(':') {
        if let Some(space_idx) = rest.find(' ') {
            let prefix_word = &rest[..space_idx];
            if !prefix_word.is_empty()
                && prefix_word
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return ParsedQuery {
                    prefix: Some(ProviderType::Plugin(prefix_word.to_string())),
                    tag_filter: None,
                    query: rest[space_idx + 1..].to_string(),
                };
            }
        } else if !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return ParsedQuery {
                prefix: Some(ProviderType::Plugin(rest.to_string())),
                tag_filter: None,
                query: String::new(),
            };
        }
    }

    let result = ParsedQuery {
        prefix: None,
        tag_filter: None,
        query: query.to_string(),
    };

    #[cfg(feature = "dev-logging")]
    debug!(
        "[Filter] parse_query({:?}) -> prefix={:?}, tag={:?}, query={:?}",
        query, result.prefix, result.tag_filter, result.query
    );

    result
}
```

> **Note:** `CORE_PREFIXES` uses function pointers (`fn() -> ProviderType`) because `ProviderType::Application` and `ProviderType::Command` are fieldless variants that can be constructed in a const context via a trivial function. `PLUGIN_PREFIXES` stays as `(&str, &str)` because the `to_string()` allocation only happens once a prefix actually matches.

- [ ] **Step 3: Run all filter tests**

Run: `cargo test -p owlry-core filter`

Expected: All existing tests PASS (behavior unchanged).

- [ ] **Step 4: Run full check**

Run: `cargo check -p owlry-core`

Expected: Clean compilation.

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/filter.rs
git commit -m "refactor(filter): consolidate parse_query prefix arrays

Merge four separate prefix arrays (core full, plugin full, core
partial, plugin partial) into two arrays with a single loop each
that checks both full and partial match. Halves the data and
eliminates the duplicate iteration."
```

---

## Phase 5: Plugin Repo Fixes

> **These tasks target `somegit.dev/Owlibou/owlry-plugins` — a separate repository.**
> Clone and branch separately. They can be done independently of Phases 1-4.

### Task 7: Filesearch — add minimum query length threshold

The filesearch plugin spawns an `fd` subprocess on every keystroke (after debounce). For short queries this is wasteful and returns too many results. Add a 3-character minimum before spawning.

**Files:**
- Modify: `owlry-plugin-filesearch/src/lib.rs` — `query()` method
- Test: existing or new tests

- [ ] **Step 1: Add early return in the `query` method**

At the top of the `query` function (which receives the search text), add:

```rust
fn query(&self, query: &str) -> Vec<PluginItem> {
    // Don't spawn fd for very short queries — too many results, too slow
    if query.len() < 3 {
        return Vec::new();
    }
    // ... existing code ...
}
```

Find the correct method — it may be the `ProviderVTable::provider_query` path or a helper like `search_with_fd`. The guard should be placed at the earliest point before `Command::new("fd")` is invoked.

- [ ] **Step 2: Verify compilation and test**

Run: `cargo check -p owlry-plugin-filesearch`

Expected: Clean compilation.

- [ ] **Step 3: Commit**

```bash
git add owlry-plugin-filesearch/src/lib.rs
git commit -m "perf(filesearch): skip fd subprocess for queries under 3 chars

Avoids spawning a subprocess per keystroke when the user has
only typed 1-2 characters. Short queries return too many results
from fd and block the daemon's read lock."
```

---

### Task 8: Emoji plugin — avoid double clone on refresh

The emoji provider's `refresh()` returns `state.items.to_vec().into()` which clones all ~400 `PluginItem` structs. The core's `NativeProvider::refresh()` then converts each to `LaunchItem` (another set of allocations). If the plugin API supports transferring ownership instead of cloning, use that. Otherwise, this is an API-level limitation.

**Files:**
- Modify: `owlry-plugin-emoji/src/lib.rs` — `provider_refresh` function

- [ ] **Step 1: Check if items can be drained instead of cloned**

If `state.items` is a `Vec<PluginItem>` that gets rebuilt on each refresh anyway, drain it:

```rust
// Was: state.items.to_vec().into()
// If items are rebuilt each refresh:
std::mem::take(&mut state.items).into()
```

If `state.items` must be preserved between refreshes (because refresh is called multiple times and the items don't change), then the clone is necessary and this task is a no-op. Check the `refresh()` implementation to determine which case applies.

- [ ] **Step 2: Verify and commit if applicable**

Run: `cargo check -p owlry-plugin-emoji`

---

### Task 9: Clipboard plugin — add caching for `cliphist list`

The clipboard provider calls `cliphist list` synchronously on every refresh. If the daemon's periodic refresh timer triggers this, it blocks the RwLock. Add a simple staleness check — only re-run `cliphist list` if more than N seconds have elapsed since the last successful fetch.

**Files:**
- Modify: `owlry-plugin-clipboard/src/lib.rs`

- [ ] **Step 1: Add a `last_refresh` timestamp to the provider state**

```rust
use std::time::Instant;

struct ClipboardState {
    items: Vec<PluginItem>,
    last_refresh: Option<Instant>,
}
```

- [ ] **Step 2: Guard the subprocess call with a staleness check**

In the `refresh()` path:

```rust
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

if let Some(last) = state.last_refresh {
    if last.elapsed() < REFRESH_INTERVAL {
        return state.items.clone().into();
    }
}

// ... existing cliphist list call ...

state.last_refresh = Some(Instant::now());
```

- [ ] **Step 3: Verify and commit**

Run: `cargo check -p owlry-plugin-clipboard`

```bash
git commit -m "perf(clipboard): cache cliphist results for 5 seconds

Avoids re-spawning cliphist list on every refresh cycle when the
previous results are still fresh."
```

---

## Summary

| Task | Impact | Repo | Risk |
|------|--------|------|------|
| 1. Remove unsafe in NativeProvider | CRITICAL (soundness) | owlry | Low — drops unnecessary RwLock |
| 2. Score-by-ref + partial sort | HIGH (keystroke perf) | owlry | Medium — touches hot path, verify with tests |
| 3. Remove frecency auto-save | MEDIUM (launch perf) | owlry | Low — Drop impl already saves |
| 4. ListBox remove_all() | MEDIUM (UI smoothness) | owlry | Low — direct GTK API replacement |
| 5. Single-pass space collapse | LOW (startup) | owlry | Low — purely algorithmic |
| 6. Consolidate parse_query | LOW (keystroke) | owlry | Low — existing tests cover behavior |
| 7. Filesearch min query length | HIGH (keystroke perf) | plugins | Low — early return guard |
| 8. Emoji refresh optimization | MEDIUM (startup) | plugins | Low — depends on API check |
| 9. Clipboard caching | MEDIUM (refresh perf) | plugins | Low — simple staleness check |
