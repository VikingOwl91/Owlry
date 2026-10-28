use expr_solver::{SymTable, eval_with_table};

use super::{DynamicProvider, ItemActivation, ItemSource, LaunchItem, ProviderType};

/// Built-in calculator provider. Evaluates mathematical expressions via `expr-solver-lib`.
///
/// Triggered by:
/// - `= expr` / `=expr` / `calc expr` (explicit prefix)
/// - Raw math expressions containing operators or known functions (auto-detect)
pub(crate) struct CalculatorProvider;

impl DynamicProvider for CalculatorProvider {
    fn name(&self) -> &str {
        "Calculator"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin("calc".into())
    }

    fn priority(&self) -> u32 {
        10_000
    }

    fn prefix(&self) -> Option<&str> {
        Some(":calc")
    }

    fn icon(&self) -> &str {
        "accessories-calculator"
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Calc")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("math expression")
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        let expr = match extract_expression(query) {
            Some(e) if !e.is_empty() => e,
            _ => return Vec::new(),
        };

        match eval_math(expr) {
            Ok(result) => {
                let display = format_result(result);
                let copy_cmd =
                    format!("printf '%s' '{}' | wl-copy", display.replace('\'', "'\\''"));
                vec![LaunchItem {
                    id: format!("calc:{}", expr),
                    name: display.clone(),
                    description: Some(format!("= {}", expr)),
                    icon: Some("accessories-calculator".into()),
                    provider: ProviderType::Plugin("calc".into()),
                    command: copy_cmd,
                    terminal: false,
                    activation: ItemActivation::Stdin {
                        program: "wl-copy".into(),
                        args: Vec::new(),
                        input: display,
                    },
                    tags: vec!["math".into(), "calculator".into()],
                    source: ItemSource::Core,
                }]
            }
            Err(_) => Vec::new(),
        }
    }
}

/// Evaluate a math expression string, returning the result as f64.
///
/// Extends the stdlib with `ln` as an alias for natural log (meval compat).
/// In expr-solver-lib, `log(x)` is natural log and `log10(x)` is base-10.
fn eval_math(expr: &str) -> Result<f64, String> {
    let mut table = SymTable::stdlib();
    table
        .add_func("ln", 1, false, |args| Ok(args[0].ln()), false)
        .expect("ln alias is valid");
    eval_with_table(expr, table)
}

/// Extract the math expression from a query string.
///
/// Handles:
/// - `= expr` and `=expr` (explicit calculator prefix)
/// - `calc expr` (word prefix)
/// - Raw expressions if they look like math (auto-detect)
///
/// Returns `None` only when input is empty after trimming.
fn extract_expression(query: &str) -> Option<&str> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Explicit prefixes
    if let Some(rest) = trimmed.strip_prefix("= ") {
        return Some(rest.trim());
    }
    if let Some(rest) = trimmed.strip_prefix('=') {
        return Some(rest.trim());
    }
    if let Some(rest) = trimmed.strip_prefix("calc ") {
        return Some(rest.trim());
    }

    // Auto-detect: only forward if the expression looks like math.
    // Plain words like "firefox" should not reach the evaluator.
    if looks_like_math(trimmed) {
        Some(trimmed)
    } else {
        None
    }
}

/// Heuristic: does this string look like a math expression?
///
/// Returns true when the string contains binary operators, digits mixed with
/// operators, or known function names. Plain alphabetic words return false.
fn looks_like_math(s: &str) -> bool {
    // Must contain at least one digit or a known constant/function name
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    let has_operator =
        s.contains('+') || s.contains('*') || s.contains('/') || s.contains('^') || s.contains('%');
    // Subtraction/negation is ambiguous; only count it as an operator when
    // there are already digits present to avoid matching bare words with hyphens.
    let has_minus_operator = has_digit && s.contains('-');

    // Known math functions that are safe to auto-evaluate
    const MATH_FUNCTIONS: &[&str] = &[
        "sqrt", "sin", "cos", "tan", "log", "ln", "abs", "floor", "ceil", "round",
    ];
    let has_function = MATH_FUNCTIONS.iter().any(|f| s.contains(f));

    has_digit && (has_operator || has_minus_operator) || has_function
}

/// Format a floating-point result for display.
///
/// Integer-valued results are shown as integers with thousands separators.
/// Non-integer results are shown with up to 10 decimal places, trailing zeros trimmed.
fn format_result(result: f64) -> String {
    if result.fract() == 0.0 && result.abs() < 1e15 {
        format_integer_with_separators(result as i64)
    } else {
        let formatted = format!("{:.10}", result);
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_integer_with_separators(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let with_commas = s
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join(",");

    if n < 0 {
        format!("-{}", with_commas)
    } else {
        with_commas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(q: &str) -> Vec<LaunchItem> {
        CalculatorProvider.query(q)
    }

    // --- Trigger prefix tests ---

    #[test]
    fn equals_prefix_addition() {
        let results = query("= 5+3");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "8");
    }

    #[test]
    fn calc_prefix_multiplication() {
        let results = query("calc 10*2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "20");
    }

    // --- Auto-detect tests ---

    #[test]
    fn auto_detect_addition() {
        let results = query("5+3");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "8");
    }

    #[test]
    fn equals_prefix_complex_expression() {
        let results = query("= sqrt(16) + 2^3");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "12");
    }

    #[test]
    fn decimal_result() {
        let results = query("= 10/3");
        assert_eq!(results.len(), 1);
        assert!(
            results[0].name.starts_with("3.333"),
            "expected result starting with 3.333, got: {}",
            results[0].name
        );
    }

    #[test]
    fn large_integer_thousands_separators() {
        let results = query("= 1000000");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "1,000,000");
    }

    // --- Invalid / non-math input ---

    #[test]
    fn invalid_expression_returns_empty() {
        let results = query("= 5 +");
        assert!(results.is_empty());
    }

    #[test]
    fn plain_text_returns_empty() {
        let results = query("firefox");
        assert!(results.is_empty());
    }

    // --- Metadata tests ---

    #[test]
    fn provider_type_is_calc_plugin() {
        assert_eq!(
            CalculatorProvider.provider_type(),
            ProviderType::Plugin("calc".into())
        );
    }

    #[test]
    fn description_shows_expression() {
        let results = query("= 5+3");
        assert_eq!(results[0].description.as_deref(), Some("= 5+3"));
    }

    #[test]
    fn copy_command_contains_wl_copy() {
        let results = query("= 5+3");
        assert!(results[0].command.contains("wl-copy"));
    }
}
