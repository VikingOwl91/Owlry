//! Calculator Plugin for Owlry
//!
//! A dynamic provider that evaluates mathematical expressions.
//! Supports queries prefixed with `=` or `calc `.
//!
//! Examples:
//! - `= 5 + 3` → 8
//! - `calc sqrt(16)` → 4
//! - `= pi * 2` → 6.283185...

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind,
    ProviderPosition, API_VERSION,
};

// Plugin metadata
const PLUGIN_ID: &str = "calculator";
const PLUGIN_NAME: &str = "Calculator";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Evaluate mathematical expressions";

// Provider metadata
const PROVIDER_ID: &str = "calculator";
const PROVIDER_NAME: &str = "Calculator";
const PROVIDER_PREFIX: &str = "=";
const PROVIDER_ICON: &str = "accessories-calculator";
const PROVIDER_TYPE_ID: &str = "calc";

/// Calculator provider state (empty for now, but could cache results)
struct CalculatorState;

// ============================================================================
// Plugin Interface Implementation
// ============================================================================

extern "C" fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: RString::from(PLUGIN_ID),
        name: RString::from(PLUGIN_NAME),
        version: RString::from(PLUGIN_VERSION),
        description: RString::from(PLUGIN_DESCRIPTION),
        api_version: API_VERSION,
    }
}

extern "C" fn plugin_providers() -> RVec<ProviderInfo> {
    vec![ProviderInfo {
        id: RString::from(PROVIDER_ID),
        name: RString::from(PROVIDER_NAME),
        prefix: ROption::RSome(RString::from(PROVIDER_PREFIX)),
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Dynamic,
        type_id: RString::from(PROVIDER_TYPE_ID),
        position: ProviderPosition::Normal,
        priority: 10000, // Dynamic: calculator results first
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    // Create state and return handle
    let state = Box::new(CalculatorState);
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(_handle: ProviderHandle) -> RVec<PluginItem> {
    // Dynamic provider - refresh does nothing
    RVec::new()
}

extern "C" fn provider_query(_handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    let query_str = query.as_str();

    // Extract expression from query
    let expr = match extract_expression(query_str) {
        Some(e) if !e.is_empty() => e,
        _ => return RVec::new(),
    };

    // Evaluate the expression
    match evaluate_expression(expr) {
        Some(item) => vec![item].into(),
        None => RVec::new(),
    }
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<CalculatorState>
        unsafe {
            handle.drop_as::<CalculatorState>();
        }
    }
}

// Register the plugin vtable
owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}

// ============================================================================
// Calculator Logic
// ============================================================================

/// Extract expression from query (handles `= expr` and `calc expr` formats)
fn extract_expression(query: &str) -> Option<&str> {
    let trimmed = query.trim();

    // Support both "= expr" and "=expr" (with or without space)
    if let Some(expr) = trimmed.strip_prefix("= ") {
        Some(expr.trim())
    } else if let Some(expr) = trimmed.strip_prefix('=') {
        Some(expr.trim())
    } else if let Some(expr) = trimmed.strip_prefix("calc ") {
        Some(expr.trim())
    } else {
        // For filter mode - accept raw expressions
        Some(trimmed)
    }
}

/// Evaluate a mathematical expression and return a PluginItem
fn evaluate_expression(expr: &str) -> Option<PluginItem> {
    match meval::eval_str(expr) {
        Ok(result) => {
            // Format result nicely
            let result_str = format_result(result);

            Some(
                PluginItem::new(
                    format!("calc:{}", expr),
                    result_str.clone(),
                    format!("sh -c 'echo -n \"{}\" | wl-copy'", result_str),
                )
                .with_description(format!("= {}", expr))
                .with_icon(PROVIDER_ICON)
                .with_keywords(vec!["math".to_string(), "calculator".to_string()]),
            )
        }
        Err(_) => None,
    }
}

/// Format a numeric result nicely
fn format_result(result: f64) -> String {
    if result.fract() == 0.0 && result.abs() < 1e15 {
        // Integer result
        format!("{}", result as i64)
    } else {
        // Float result with reasonable precision, trimming trailing zeros
        let formatted = format!("{:.10}", result);
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_expression() {
        assert_eq!(extract_expression("= 5+3"), Some("5+3"));
        assert_eq!(extract_expression("=5+3"), Some("5+3"));
        assert_eq!(extract_expression("calc 5+3"), Some("5+3"));
        assert_eq!(extract_expression("  = 5 + 3 "), Some("5 + 3"));
        assert_eq!(extract_expression("5+3"), Some("5+3")); // Raw expression
    }

    #[test]
    fn test_format_result() {
        assert_eq!(format_result(8.0), "8");
        assert_eq!(format_result(2.5), "2.5");
        assert_eq!(format_result(3.14159265358979), "3.1415926536");
    }

    #[test]
    fn test_evaluate_basic() {
        let item = evaluate_expression("5+3").unwrap();
        assert_eq!(item.name.as_str(), "8");

        let item = evaluate_expression("10 * 2").unwrap();
        assert_eq!(item.name.as_str(), "20");

        let item = evaluate_expression("15 / 3").unwrap();
        assert_eq!(item.name.as_str(), "5");
    }

    #[test]
    fn test_evaluate_float() {
        let item = evaluate_expression("5/2").unwrap();
        assert_eq!(item.name.as_str(), "2.5");
    }

    #[test]
    fn test_evaluate_functions() {
        let item = evaluate_expression("sqrt(16)").unwrap();
        assert_eq!(item.name.as_str(), "4");

        let item = evaluate_expression("abs(-5)").unwrap();
        assert_eq!(item.name.as_str(), "5");
    }

    #[test]
    fn test_evaluate_constants() {
        let item = evaluate_expression("pi").unwrap();
        assert!(item.name.as_str().starts_with("3.14159"));

        let item = evaluate_expression("e").unwrap();
        assert!(item.name.as_str().starts_with("2.718"));
    }

    #[test]
    fn test_evaluate_invalid() {
        assert!(evaluate_expression("").is_none());
        assert!(evaluate_expression("invalid").is_none());
        assert!(evaluate_expression("5 +").is_none());
    }
}
