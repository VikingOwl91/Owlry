# Built-in Providers Migration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move calculator, converter, and system from external `.so` plugins to native providers compiled into `owlry-core`. Retire 3 plugin AUR packages (transitional) and 4 meta AUR packages (already deleted). Update READMEs.

**Architecture:** Add a `DynamicProvider` trait to owlry-core for built-in providers that produce results per-keystroke (calculator, converter). System uses the existing `Provider` trait (static list). All 3 are registered in `ProviderManager::new_with_config()` gated by config toggles. Conflict detection skips native `.so` plugins when a built-in provider with the same type_id exists.

**Tech Stack:** Rust 1.90+, owlry-core, meval (math), reqwest (currency HTTP), serde

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/owlry-core/Cargo.toml` | Modify | Add/move `meval`, `reqwest` to required deps |
| `crates/owlry-core/src/providers/mod.rs` | Modify | Add `DynamicProvider` trait, `builtin_dynamic` field, iterate in search, register providers, conflict detection |
| `crates/owlry-core/src/providers/calculator.rs` | Create | Calculator provider (port from plugin) |
| `crates/owlry-core/src/providers/system.rs` | Create | System provider (port from plugin) |
| `crates/owlry-core/src/providers/converter/mod.rs` | Create | Converter provider entry (port from plugin) |
| `crates/owlry-core/src/providers/converter/parser.rs` | Create | Query parsing |
| `crates/owlry-core/src/providers/converter/units.rs` | Create | Unit definitions + conversion |
| `crates/owlry-core/src/providers/converter/currency.rs` | Create | ECB rate fetching + currency conversion |
| `crates/owlry-core/src/config/mod.rs` | Modify | Add `converter` toggle |
| `crates/owlry-core/src/lib.rs` | Modify | Re-export new provider modules if needed |
| `README.md` | Modify | Update package tables, remove meta section |

**Plugins repo (separate commits):**

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/owlry-plugin-calculator/` | Remove | Retired — built into core |
| `crates/owlry-plugin-converter/` | Remove | Retired — built into core |
| `crates/owlry-plugin-system/` | Remove | Retired — built into core |
| `aur/owlry-plugin-calculator/` | Modify | Transitional PKGBUILD |
| `aur/owlry-plugin-converter/` | Modify | Transitional PKGBUILD |
| `aur/owlry-plugin-system/` | Modify | Transitional PKGBUILD |
| `README.md` | Modify | Remove retired plugins |

---

### Task 1: Add dependencies and DynamicProvider trait

**Files:**
- Modify: `crates/owlry-core/Cargo.toml`
- Modify: `crates/owlry-core/src/providers/mod.rs`

- [ ] **Step 1: Update Cargo.toml — move meval and reqwest to required deps**

In `crates/owlry-core/Cargo.toml`, add to the `[dependencies]` section (NOT in optional):

```toml
# Built-in provider deps
meval = "0.2"
reqwest = { version = "0.13", default-features = false, features = ["rustls", "blocking"] }
```

Remove `meval` and `reqwest` from the `[features]` section's `lua` feature list. The `lua` feature should become:

```toml
[features]
default = []
lua = ["dep:mlua"]
dev-logging = []
```

Remove the `meval = { ... optional = true }` and `reqwest = { ... optional = true }` lines from `[dependencies]` since we're replacing them with required versions.

- [ ] **Step 2: Add DynamicProvider trait to providers/mod.rs**

In `crates/owlry-core/src/providers/mod.rs`, add after the `Provider` trait definition (after line 105):

```rust
/// Trait for built-in providers that produce results per-keystroke.
/// Unlike static `Provider`s which cache items via `refresh()`/`items()`,
/// dynamic providers generate results on every query.
pub(crate) trait DynamicProvider: Send + Sync {
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn query(&self, query: &str) -> Vec<LaunchItem>;
    fn priority(&self) -> u32;
}
```

- [ ] **Step 3: Add builtin_dynamic field to ProviderManager**

In the `ProviderManager` struct definition, add after the `providers` field:

```rust
    /// Built-in dynamic providers (calculator, converter)
    /// These are queried per-keystroke, like native dynamic plugins
    builtin_dynamic: Vec<Box<dyn DynamicProvider>>,
```

- [ ] **Step 4: Initialize the new field in ProviderManager::new()**

In `ProviderManager::new()`, add `builtin_dynamic: Vec::new(),` to the struct initialization.

- [ ] **Step 5: Iterate builtin_dynamic in search_with_frecency**

In the `search_with_frecency` method, inside the `if !query.is_empty()` block, after the existing `for provider in &self.dynamic_providers` loop, add:

```rust
                // Built-in dynamic providers (calculator, converter)
                for provider in &self.builtin_dynamic {
                    if !filter.is_active(provider.provider_type()) {
                        continue;
                    }
                    let dynamic_results = provider.query(query);
                    let base_score = provider.priority() as i64;

                    let grouping_bonus: i64 = match provider.provider_type() {
                        ProviderType::Plugin(ref id)
                            if matches!(id.as_str(), "calc" | "conv") =>
                        {
                            10_000
                        }
                        _ => 0,
                    };

                    for (idx, item) in dynamic_results.into_iter().enumerate() {
                        results.push((item, base_score + grouping_bonus - idx as i64));
                    }
                }
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p owlry-core`

Expected: Compiles (with warnings about unused fields/imports — that's fine, providers aren't registered yet).

- [ ] **Step 7: Commit**

```bash
git add crates/owlry-core/Cargo.toml crates/owlry-core/src/providers/mod.rs
git commit -m "feat(core): add DynamicProvider trait and builtin_dynamic support

Foundation for built-in calculator, converter, and system providers.
DynamicProvider trait for per-keystroke providers. ProviderManager
iterates builtin_dynamic alongside native dynamic plugins in search."
```

---

### Task 2: Port calculator provider

**Files:**
- Create: `crates/owlry-core/src/providers/calculator.rs`
- Modify: `crates/owlry-core/src/providers/mod.rs` (add `mod calculator;`)

The calculator plugin (`owlry-plugins/crates/owlry-plugin-calculator/src/lib.rs`) evaluates math expressions. It supports `= expr` and `calc expr` prefix triggers, plus auto-detection of math-like input. Port it to implement `DynamicProvider`.

- [ ] **Step 1: Add module declaration**

In `crates/owlry-core/src/providers/mod.rs`, add with the other module declarations at the top:

```rust
pub(crate) mod calculator;
```

- [ ] **Step 2: Write the calculator provider with tests**

Create `crates/owlry-core/src/providers/calculator.rs`:

```rust
//! Built-in calculator provider.
//!
//! Evaluates math expressions and returns the result.
//! Supports `= expr` and `calc expr` prefix triggers, plus auto-detection.

use super::{DynamicProvider, LaunchItem, ProviderType};

const PROVIDER_TYPE_ID: &str = "calc";

pub struct CalculatorProvider;

impl CalculatorProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DynamicProvider for CalculatorProvider {
    fn name(&self) -> &str {
        "Calculator"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(PROVIDER_TYPE_ID.into())
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        let input = query.trim();

        // Strip trigger prefixes
        let expr = if let Some(rest) = input.strip_prefix('=') {
            rest.trim()
        } else if let Some(rest) = input.strip_prefix("calc ") {
            rest.trim()
        } else if let Some(rest) = input.strip_prefix("calc\t") {
            rest.trim()
        } else if looks_like_math(input) {
            input
        } else {
            return Vec::new();
        };

        if expr.is_empty() {
            return Vec::new();
        }

        match meval::eval_str(expr) {
            Ok(result) if result.is_finite() => {
                let display = format_result(result);
                let description = format!("= {}", expr);
                let copy_cmd = format!("printf '%s' '{}' | wl-copy", display);

                vec![LaunchItem {
                    id: format!("calc:{}", expr),
                    name: display,
                    description: Some(description),
                    icon: Some("accessories-calculator".into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: copy_cmd,
                    terminal: false,
                    tags: vec!["math".into(), "calculator".into()],
                }]
            }
            _ => Vec::new(),
        }
    }

    fn priority(&self) -> u32 {
        10_000
    }
}

/// Check if input looks like a math expression (heuristic for auto-detect)
fn looks_like_math(input: &str) -> bool {
    if input.is_empty() || input.len() < 2 {
        return false;
    }
    // Must contain at least one operator
    let has_operator = input.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '%'));
    if !has_operator && !input.contains("sqrt") && !input.contains("sin") && !input.contains("cos") && !input.contains("tan") && !input.contains("log") && !input.contains("ln") && !input.contains("abs") {
        return false;
    }
    // First char should be a digit, minus, open-paren, or function name
    let first = input.chars().next().unwrap();
    first.is_ascii_digit() || first == '-' || first == '(' || first == '.'
        || input.starts_with("sqrt") || input.starts_with("sin")
        || input.starts_with("cos") || input.starts_with("tan")
        || input.starts_with("log") || input.starts_with("ln")
        || input.starts_with("abs") || input.starts_with("pi")
        || input.starts_with("e ")
}

fn format_result(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        let i = n as i64;
        if i.abs() >= 1000 {
            format_with_separators(i)
        } else {
            format!("{}", i)
        }
    } else {
        format!("{:.10}", n)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_with_separators(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if n < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(input: &str) -> Vec<LaunchItem> {
        CalculatorProvider::new().query(input)
    }

    #[test]
    fn test_prefix_equals() {
        let r = query("= 5+3");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "8");
    }

    #[test]
    fn test_prefix_calc() {
        let r = query("calc 10*2");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "20");
    }

    #[test]
    fn test_auto_detect() {
        let r = query("5+3");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "8");
    }

    #[test]
    fn test_complex_expression() {
        let r = query("= sqrt(16) + 2^3");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "12");
    }

    #[test]
    fn test_decimal_result() {
        let r = query("= 10/3");
        assert_eq!(r.len(), 1);
        assert!(r[0].name.starts_with("3.333"));
    }

    #[test]
    fn test_large_number_separators() {
        let r = query("= 1000000");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "1,000,000");
    }

    #[test]
    fn test_invalid_expression_returns_empty() {
        assert!(query("= hello world").is_empty());
    }

    #[test]
    fn test_plain_text_returns_empty() {
        assert!(query("firefox").is_empty());
    }

    #[test]
    fn test_provider_type() {
        let p = CalculatorProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("calc".into()));
    }

    #[test]
    fn test_description_shows_expression() {
        let r = query("= 2+2");
        assert_eq!(r[0].description.as_deref(), Some("= 2+2"));
    }

    #[test]
    fn test_copy_command() {
        let r = query("= 42");
        assert!(r[0].command.contains("42"));
        assert!(r[0].command.contains("wl-copy"));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p owlry-core calculator`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/owlry-core/src/providers/calculator.rs crates/owlry-core/src/providers/mod.rs
git commit -m "feat(core): add built-in calculator provider

Port from owlry-plugin-calculator. Implements DynamicProvider trait.
Supports = prefix, calc prefix, and auto-detection of math expressions.
Uses meval for expression evaluation."
```

---

### Task 3: Port system provider

**Files:**
- Create: `crates/owlry-core/src/providers/system.rs`
- Modify: `crates/owlry-core/src/providers/mod.rs` (add `mod system;`)

System is a static provider — it returns a fixed list of actions (shutdown, reboot, etc.) via `refresh()`/`items()`. Implements the existing `Provider` trait.

- [ ] **Step 1: Add module declaration**

In `crates/owlry-core/src/providers/mod.rs`:

```rust
pub(crate) mod system;
```

- [ ] **Step 2: Write the system provider with tests**

Create `crates/owlry-core/src/providers/system.rs`:

```rust
//! Built-in system provider.
//!
//! Provides power and session management actions: shutdown, reboot, suspend,
//! hibernate, lock screen, and log out.

use super::{LaunchItem, Provider, ProviderType};

const PROVIDER_TYPE_ID: &str = "sys";
const PROVIDER_ICON: &str = "system-shutdown";

struct SystemAction {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    command: &'static str,
}

const ACTIONS: &[SystemAction] = &[
    SystemAction {
        id: "shutdown",
        name: "Shutdown",
        description: "Power off the system",
        icon: "system-shutdown",
        command: "systemctl poweroff",
    },
    SystemAction {
        id: "reboot",
        name: "Reboot",
        description: "Restart the system",
        icon: "system-reboot",
        command: "systemctl reboot",
    },
    SystemAction {
        id: "reboot-bios",
        name: "Reboot to BIOS",
        description: "Restart into firmware setup",
        icon: "system-reboot",
        command: "systemctl reboot --firmware-setup",
    },
    SystemAction {
        id: "suspend",
        name: "Suspend",
        description: "Suspend the system to RAM",
        icon: "system-suspend",
        command: "systemctl suspend",
    },
    SystemAction {
        id: "hibernate",
        name: "Hibernate",
        description: "Hibernate the system to disk",
        icon: "system-hibernate",
        command: "systemctl hibernate",
    },
    SystemAction {
        id: "lock",
        name: "Lock Screen",
        description: "Lock the current session",
        icon: "system-lock-screen",
        command: "loginctl lock-session",
    },
    SystemAction {
        id: "logout",
        name: "Log Out",
        description: "End the current session",
        icon: "system-log-out",
        command: "loginctl terminate-session self",
    },
];

pub struct SystemProvider {
    items: Vec<LaunchItem>,
}

impl SystemProvider {
    pub fn new() -> Self {
        let items = ACTIONS
            .iter()
            .map(|a| LaunchItem {
                id: format!("sys:{}", a.id),
                name: a.name.into(),
                description: Some(a.description.into()),
                icon: Some(a.icon.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: a.command.into(),
                terminal: false,
                tags: vec!["system".into()],
            })
            .collect();

        Self { items }
    }
}

impl Provider for SystemProvider {
    fn name(&self) -> &str {
        "System"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(PROVIDER_TYPE_ID.into())
    }

    fn refresh(&mut self) {
        // Static list — nothing to refresh
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_all_actions() {
        let p = SystemProvider::new();
        assert_eq!(p.items().len(), 7);
    }

    #[test]
    fn test_action_names() {
        let p = SystemProvider::new();
        let names: Vec<&str> = p.items().iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Shutdown"));
        assert!(names.contains(&"Reboot"));
        assert!(names.contains(&"Lock Screen"));
        assert!(names.contains(&"Log Out"));
    }

    #[test]
    fn test_provider_type() {
        let p = SystemProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("sys".into()));
    }

    #[test]
    fn test_shutdown_command() {
        let p = SystemProvider::new();
        let shutdown = p.items().iter().find(|i| i.id == "sys:shutdown").unwrap();
        assert_eq!(shutdown.command, "systemctl poweroff");
    }

    #[test]
    fn test_all_items_have_system_tag() {
        let p = SystemProvider::new();
        for item in p.items() {
            assert!(item.tags.contains(&"system".into()));
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p owlry-core system`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/owlry-core/src/providers/system.rs crates/owlry-core/src/providers/mod.rs
git commit -m "feat(core): add built-in system provider

Port from owlry-plugin-system. Static provider with 7 actions:
shutdown, reboot, reboot-to-BIOS, suspend, hibernate, lock, logout."
```

---

### Task 4: Port converter provider

**Files:**
- Create: `crates/owlry-core/src/providers/converter/mod.rs`
- Create: `crates/owlry-core/src/providers/converter/parser.rs`
- Create: `crates/owlry-core/src/providers/converter/units.rs`
- Create: `crates/owlry-core/src/providers/converter/currency.rs`
- Modify: `crates/owlry-core/src/providers/mod.rs` (add `mod converter;`)

This is the largest port — 4 files, ~1700 lines. The logic is identical to the plugin; the port removes FFI types (`RString`, `RVec`, `PluginItem`) and uses `LaunchItem` directly.

- [ ] **Step 1: Add module declaration**

In `crates/owlry-core/src/providers/mod.rs`:

```rust
pub(crate) mod converter;
```

- [ ] **Step 2: Create converter/parser.rs**

Create `crates/owlry-core/src/providers/converter/parser.rs`. This is a direct copy from `owlry-plugins/crates/owlry-plugin-converter/src/parser.rs` with `crate::` import paths changed to `super::`:

Copy the complete file from `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins/crates/owlry-plugin-converter/src/parser.rs`, changing:
- `use crate::units;` → `use super::units;`

No other changes — the parser has no FFI types.

- [ ] **Step 3: Create converter/units.rs**

Copy the complete file from `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins/crates/owlry-plugin-converter/src/units.rs`, changing:
- `use crate::currency;` → `use super::currency;`
- `crate::format_with_separators` → `super::format_with_separators`

No other changes — units.rs has no FFI types.

- [ ] **Step 4: Create converter/currency.rs**

Copy the complete file from `/home/cnachtigall/ssd/git/archive/owlibou/owlry-plugins/crates/owlry-plugin-converter/src/currency.rs`.

No import changes needed — currency.rs uses only `std`, `serde`, `reqwest`, and `dirs`.

- [ ] **Step 5: Create converter/mod.rs**

Create `crates/owlry-core/src/providers/converter/mod.rs`. This replaces the plugin's `lib.rs`, removing all FFI types and implementing `DynamicProvider`:

```rust
//! Built-in converter provider.
//!
//! Converts between units and currencies.
//! Supports `> expr` prefix or auto-detection.

pub(crate) mod currency;
pub(crate) mod parser;
pub(crate) mod units;

use super::{DynamicProvider, LaunchItem, ProviderType};

const PROVIDER_TYPE_ID: &str = "conv";
const PROVIDER_ICON: &str = "edit-find-replace-symbolic";

pub struct ConverterProvider;

impl ConverterProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DynamicProvider for ConverterProvider {
    fn name(&self) -> &str {
        "Converter"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(PROVIDER_TYPE_ID.into())
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        let query_str = query.trim();
        let input = if let Some(rest) = query_str.strip_prefix('>') {
            rest.trim()
        } else {
            query_str
        };

        let parsed = match parser::parse_conversion(input) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let results = if let Some(ref target) = parsed.target_unit {
            units::convert_to(&parsed.value, &parsed.from_unit, target)
                .into_iter()
                .collect()
        } else {
            units::convert_common(&parsed.value, &parsed.from_unit)
        };

        results
            .into_iter()
            .map(|r| {
                LaunchItem {
                    id: format!("conv:{}:{}:{}", parsed.from_unit, r.target_symbol, r.value),
                    name: r.display_value.clone(),
                    description: Some(format!(
                        "{} {} = {}",
                        format_number(parsed.value),
                        parsed.from_symbol,
                        r.display_value,
                    )),
                    icon: Some(PROVIDER_ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("printf '%s' '{}' | wl-copy", r.raw_value.replace('\'', "'\\''")),
                    terminal: false,
                    tags: vec![],
                }
            })
            .collect()
    }

    fn priority(&self) -> u32 {
        9_000
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        let i = n as i64;
        if i.abs() >= 1000 {
            format_with_separators(i)
        } else {
            format!("{}", i)
        }
    } else {
        format!("{:.4}", n)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

pub(crate) fn format_with_separators(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if n < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(input: &str) -> Vec<LaunchItem> {
        ConverterProvider::new().query(input)
    }

    #[test]
    fn test_prefix_trigger() {
        let r = query("> 100 km to mi");
        assert!(!r.is_empty());
    }

    #[test]
    fn test_auto_detect() {
        let r = query("100 km to mi");
        assert!(!r.is_empty());
    }

    #[test]
    fn test_no_target_returns_common() {
        let r = query("> 100 km");
        assert!(r.len() > 1);
    }

    #[test]
    fn test_temperature() {
        let r = query("102F to C");
        assert!(!r.is_empty());
    }

    #[test]
    fn test_nonsense_returns_empty() {
        assert!(query("hello world").is_empty());
    }

    #[test]
    fn test_provider_type() {
        let p = ConverterProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("conv".into()));
    }

    #[test]
    fn test_description_no_double_unit() {
        let r = query("100 km to mi");
        if let Some(item) = r.first() {
            let desc = item.description.as_deref().unwrap();
            // Should be "100 km = 62.1371 mi", not "100 km = 62.1371 mi mi"
            assert!(!desc.ends_with(" mi mi"), "double unit in: {}", desc);
        }
    }

    #[test]
    fn test_copy_command() {
        let r = query("= 100 km to mi");
        if let Some(item) = r.first() {
            assert!(item.command.contains("wl-copy"));
        }
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p owlry-core converter`

Expected: All converter tests pass (currency tests may skip if network unavailable).

- [ ] **Step 7: Run all tests**

Run: `cargo test -p owlry-core --lib`

Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/owlry-core/src/providers/converter/
git add crates/owlry-core/src/providers/mod.rs
git commit -m "feat(core): add built-in converter provider

Port from owlry-plugin-converter. Implements DynamicProvider trait.
Supports unit conversion (10 categories, 75+ units) and currency
conversion via ECB API with 24-hour file cache."
```

---

### Task 5: Register providers and add config toggle

**Files:**
- Modify: `crates/owlry-core/src/config/mod.rs`
- Modify: `crates/owlry-core/src/providers/mod.rs`

- [ ] **Step 1: Add converter config toggle**

In `crates/owlry-core/src/config/mod.rs`, add to the `ProvidersConfig` struct after the `calculator` field:

```rust
    /// Enable converter provider (> expression or auto-detect)
    #[serde(default = "default_true")]
    pub converter: bool,
```

Add `converter: true,` to the `Default` impl for `ProvidersConfig`.

- [ ] **Step 2: Register built-in providers in new_with_config**

In `crates/owlry-core/src/providers/mod.rs`, in `new_with_config()`, after creating the `core_providers` vec and before the native plugin loader section, add:

```rust
        // Built-in dynamic providers
        let mut builtin_dynamic: Vec<Box<dyn DynamicProvider>> = Vec::new();

        if config.providers.calculator {
            builtin_dynamic.push(Box::new(calculator::CalculatorProvider::new()));
            info!("Registered built-in calculator provider");
        }

        if config.providers.converter {
            builtin_dynamic.push(Box::new(converter::ConverterProvider::new()));
            info!("Registered built-in converter provider");
        }

        // Built-in static providers
        if config.providers.system {
            core_providers.push(Box::new(system::SystemProvider::new()));
            info!("Registered built-in system provider");
        }
```

Then pass `builtin_dynamic` into the `ProviderManager` construction. In `ProviderManager::new()`, accept it as a parameter:

Change `pub fn new(core_providers: Vec<Box<dyn Provider>>, native_providers: Vec<NativeProvider>) -> Self` to:

```rust
pub fn new(
    core_providers: Vec<Box<dyn Provider>>,
    native_providers: Vec<NativeProvider>,
    builtin_dynamic: Vec<Box<dyn DynamicProvider>>,
) -> Self
```

And set the field: `builtin_dynamic,` in the struct initialization.

Update all call sites of `ProviderManager::new()`:
- In `new_with_config()`: pass the `builtin_dynamic` vec
- In `app.rs` (owlry crate, dmenu mode): pass `Vec::new()` as the third argument
- In `app.rs` (local fallback): pass `Vec::new()` as the third argument

- [ ] **Step 3: Verify it compiles and tests pass**

Run: `cargo check --workspace && cargo test -p owlry-core --lib`

Expected: Clean compile, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/owlry-core/src/config/mod.rs crates/owlry-core/src/providers/mod.rs crates/owlry/src/app.rs
git commit -m "feat(core): register built-in providers in ProviderManager

Calculator and converter registered as built-in dynamic providers.
System registered as built-in static provider. All gated by config
toggles (calculator, converter, system — default true)."
```

---

### Task 6: Add conflict detection

**Files:**
- Modify: `crates/owlry-core/src/providers/mod.rs`

When users upgrade to the new owlry-core but still have the old `.so` plugins installed, both the built-in and native plugin would produce duplicate results. Skip native plugins whose type_id matches a built-in.

- [ ] **Step 1: Write test**

Add to the tests in `crates/owlry-core/src/providers/mod.rs`:

```rust
    #[test]
    fn test_builtin_type_ids() {
        let pm = ProviderManager::new(vec![], vec![], vec![
            Box::new(calculator::CalculatorProvider::new()),
            Box::new(converter::ConverterProvider::new()),
        ]);
        let ids = pm.builtin_type_ids();
        assert!(ids.contains("calc"));
        assert!(ids.contains("conv"));
    }
```

- [ ] **Step 2: Implement builtin_type_ids and conflict detection**

Add a helper method to `ProviderManager`:

```rust
    /// Get type IDs of built-in dynamic providers (for conflict detection)
    fn builtin_type_ids(&self) -> std::collections::HashSet<String> {
        let mut ids: std::collections::HashSet<String> = self
            .builtin_dynamic
            .iter()
            .filter_map(|p| match p.provider_type() {
                ProviderType::Plugin(id) => Some(id),
                _ => None,
            })
            .collect();
        // Also include built-in static providers (system)
        for p in &self.providers {
            if let ProviderType::Plugin(id) = p.provider_type() {
                ids.insert(id);
            }
        }
        ids
    }
```

In `new_with_config()`, after creating the `ProviderManager` with `Self::new(...)`, add conflict detection before the native plugin classification loop. In the loop that classifies native providers (around the `for provider in native_providers` block), add a skip check:

```rust
            let builtin_ids = manager.builtin_type_ids();
```

Then inside the loop, before the `if provider.is_dynamic()` check:

```rust
                // Skip native plugins that conflict with built-in providers
                if builtin_ids.contains(&type_id) {
                    info!(
                        "Skipping native plugin '{}' — built-in provider exists",
                        type_id
                    );
                    continue;
                }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p owlry-core --lib`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/owlry-core/src/providers/mod.rs
git commit -m "feat(core): skip native plugins that conflict with built-in providers

When users upgrade owlry-core but still have old .so plugins installed,
the conflict detection skips the native plugin to prevent duplicate
results."
```

---

### Task 7: Remove meta package AUR dirs from main repo

**Files:**
- Remove: `aur/owlry-meta-essentials/`
- Remove: `aur/owlry-meta-full/`
- Remove: `aur/owlry-meta-tools/`
- Remove: `aur/owlry-meta-widgets/`

- [ ] **Step 1: Remove meta package directories**

The meta packages have already been deleted from AUR. Remove the local directories:

```bash
# Hide .git dirs, remove from git tracking, restore .git dirs
for pkg in owlry-meta-essentials owlry-meta-full owlry-meta-tools owlry-meta-widgets; do
    dir="aur/$pkg"
    if [ -d "$dir/.git" ]; then
        mv "$dir/.git" "$dir/.git.bak"
        git rm -r "$dir"
        # Don't restore .git — the dir is gone from tracking
        # But keep the local .git.bak in case we need the AUR repo history
    fi
done
```

Actually, since the AUR packages are already deleted, just remove the directories entirely:

```bash
for pkg in owlry-meta-essentials owlry-meta-full owlry-meta-tools owlry-meta-widgets; do
    rm -rf "aur/$pkg"
done
git add -A aur/
```

- [ ] **Step 2: Commit**

```bash
git commit -m "chore: remove retired meta package AUR dirs

owlry-meta-essentials, owlry-meta-full, owlry-meta-tools, and
owlry-meta-widgets have been deleted from AUR. Remove local dirs."
```

---

### Task 8: Update main repo README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update package tables and install instructions**

In the README:

1. In the AUR install section, remove the meta package lines and update:
```bash
# Core (includes calculator, converter, system actions)
yay -S owlry

# Add individual plugins
yay -S owlry-plugin-bookmarks owlry-plugin-weather
```

2. In the "Core packages" table, update owlry-core description:
```
| `owlry-core` | Headless daemon with built-in calculator, converter, and system providers |
```

3. Remove the "Meta bundles" table entirely.

4. Remove calculator, converter, and system from the "Plugin packages" table.

5. Update the plugin count in the Features section from "14 native plugins" to "11 plugin packages" or similar.

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README for built-in providers migration

Calculator, converter, and system are now built into owlry-core.
Remove meta package references. Update install instructions."
```

---

### Task 9: Remove retired plugins from plugins repo

**Files (in owlry-plugins repo):**
- Remove: `crates/owlry-plugin-calculator/`
- Remove: `crates/owlry-plugin-converter/`
- Remove: `crates/owlry-plugin-system/`
- Modify: `Cargo.toml` (workspace members)
- Modify: `Cargo.lock`

- [ ] **Step 1: Remove crate directories and update workspace**

```bash
rm -rf crates/owlry-plugin-calculator crates/owlry-plugin-converter crates/owlry-plugin-system
```

Edit the workspace `Cargo.toml` to remove the three members from the `[workspace] members` list.

Run: `cargo check --workspace` to verify remaining plugins still build.

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "chore: remove calculator, converter, system plugins

These providers are now built into owlry-core. The plugins are
retired — transitional AUR packages redirect to owlry-core."
```

- [ ] **Step 3: Push**

```bash
git push
```

---

### Task 10: Update plugins repo README

**Files (in owlry-plugins repo):**
- Modify: `README.md`

- [ ] **Step 1: Update plugin listing**

Remove calculator, converter, and system from the plugin listing. Add a note:

```markdown
> **Note:** Calculator, converter, and system actions are built into `owlry-core` as of v1.2.0.
> They no longer need to be installed separately.
```

Update the plugin count.

- [ ] **Step 2: Commit and push**

```bash
git add README.md
git commit -m "docs: update README — calculator, converter, system moved to core"
git push
```

---

### Task 11: Transitional AUR packages for retired plugins

**Files (in owlry-plugins repo):**
- Modify: `aur/owlry-plugin-calculator/PKGBUILD`
- Modify: `aur/owlry-plugin-converter/PKGBUILD`
- Modify: `aur/owlry-plugin-system/PKGBUILD`

- [ ] **Step 1: Create transitional PKGBUILDs**

For each of the 3 retired plugins, replace the PKGBUILD with a transitional package. Example for calculator (repeat for converter and system):

```bash
# Maintainer: vikingowl <christian@nachtigall.dev>
pkgname=owlry-plugin-calculator
pkgver=1.0.1
pkgrel=99
pkgdesc="Transitional package — calculator is now built into owlry-core"
arch=('any')
url="https://somegit.dev/Owlibou/owlry"
license=('GPL-3.0-or-later')
depends=('owlry-core')
replaces=('owlry-plugin-calculator')
```

No `source`, `prepare()`, `build()`, `check()`, or `package()` functions. The `pkgrel=99` ensures this version is higher than any previous release.

Regenerate `.SRCINFO` for each:

```bash
cd aur/owlry-plugin-calculator && makepkg --printsrcinfo > .SRCINFO
cd ../owlry-plugin-converter && makepkg --printsrcinfo > .SRCINFO
cd ../owlry-plugin-system && makepkg --printsrcinfo > .SRCINFO
```

- [ ] **Step 2: Commit transitional PKGBUILDs to plugins repo**

```bash
# Stage using the .git workaround
for pkg in owlry-plugin-calculator owlry-plugin-converter owlry-plugin-system; do
    just aur-stage "$pkg"
done
git commit -m "chore(aur): transitional packages for retired plugins"
git push
```

- [ ] **Step 3: Publish to AUR**

```bash
for pkg in owlry-plugin-calculator owlry-plugin-converter owlry-plugin-system; do
    just aur-publish-pkg "$pkg"
done
```

---

### Task 12: Tag and deploy owlry-core to AUR

- [ ] **Step 1: Bump, tag, and deploy**

```bash
just release-crate owlry-core <new_version>
```

This runs the full pipeline: bump → push → tag → AUR update → publish.

---

## Execution Notes

### Task dependency order

Tasks 1-6 are sequential (each builds on the previous).
Task 7 is independent (can be done anytime).
Task 8 depends on Tasks 1-6 (README reflects built-in providers).
Tasks 9-11 are in the plugins repo and depend on Task 12 (need the new owlry-core version for transitional package `depends`).
Task 12 depends on Tasks 1-6.

**Recommended order:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 12 → 9 → 10 → 11
