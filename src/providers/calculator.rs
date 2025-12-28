use super::{LaunchItem, Provider, ProviderType};
use log::debug;

/// Calculator provider for evaluating math expressions
/// Syntax: `= expression` or `calc expression`
pub struct CalculatorProvider {
    /// Cached result from last evaluation
    cached_result: Option<LaunchItem>,
}

impl CalculatorProvider {
    pub fn new() -> Self {
        Self {
            cached_result: None,
        }
    }

    /// Check if a query is a calculator expression
    pub fn is_calculator_query(query: &str) -> bool {
        let trimmed = query.trim();
        trimmed.starts_with("= ") || trimmed.starts_with("calc ")
    }

    /// Extract the expression from a calculator query
    fn extract_expression(query: &str) -> Option<&str> {
        let trimmed = query.trim();
        if let Some(expr) = trimmed.strip_prefix("= ") {
            Some(expr.trim())
        } else if let Some(expr) = trimmed.strip_prefix("calc ") {
            Some(expr.trim())
        } else {
            None
        }
    }

    /// Evaluate an expression and return a LaunchItem result
    pub fn evaluate(&mut self, query: &str) -> Option<LaunchItem> {
        let expr = Self::extract_expression(query)?;

        if expr.is_empty() {
            return None;
        }

        debug!("Evaluating expression: {}", expr);

        match meval::eval_str(expr) {
            Ok(result) => {
                // Format result nicely
                let result_str = if result.fract() == 0.0 && result.abs() < 1e15 {
                    // Integer result
                    format!("{}", result as i64)
                } else {
                    // Float result with reasonable precision
                    let formatted = format!("{:.10}", result);
                    // Trim trailing zeros
                    formatted.trim_end_matches('0').trim_end_matches('.').to_string()
                };

                let item = LaunchItem {
                    id: format!("calc:{}", expr),
                    name: result_str.clone(),
                    description: Some(format!("= {}", expr)),
                    icon: Some("accessories-calculator".to_string()),
                    provider: ProviderType::Calculator,
                    // Copy result to clipboard using wl-copy
                    command: format!("sh -c 'echo -n \"{}\" | wl-copy'", result_str),
                    terminal: false,
                };

                debug!("Calculator result: {} = {}", expr, result_str);
                self.cached_result = Some(item.clone());
                Some(item)
            }
            Err(e) => {
                debug!("Calculator error for '{}': {}", expr, e);
                None
            }
        }
    }
}

impl Provider for CalculatorProvider {
    fn name(&self) -> &str {
        "Calculator"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Calculator
    }

    fn refresh(&mut self) {
        // Calculator doesn't need refresh - it evaluates on-demand
        self.cached_result = None;
    }

    fn items(&self) -> &[LaunchItem] {
        // Calculator is a dynamic provider - items are generated from query
        // Return cached result if available (for UI display)
        match &self.cached_result {
            Some(item) => std::slice::from_ref(item),
            None => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_calculator_query() {
        assert!(CalculatorProvider::is_calculator_query("= 5+3"));
        assert!(CalculatorProvider::is_calculator_query("calc 5+3"));
        assert!(CalculatorProvider::is_calculator_query("  = 5+3"));
        assert!(!CalculatorProvider::is_calculator_query("5+3"));
        assert!(!CalculatorProvider::is_calculator_query("firefox"));
    }

    #[test]
    fn test_extract_expression() {
        assert_eq!(
            CalculatorProvider::extract_expression("= 5+3"),
            Some("5+3")
        );
        assert_eq!(
            CalculatorProvider::extract_expression("calc 5+3"),
            Some("5+3")
        );
        assert_eq!(
            CalculatorProvider::extract_expression("=  5 + 3 "),
            Some("5 + 3")
        );
        assert_eq!(CalculatorProvider::extract_expression("5+3"), None);
    }

    #[test]
    fn test_evaluate_basic() {
        let mut calc = CalculatorProvider::new();

        let result = calc.evaluate("= 5+3").unwrap();
        assert_eq!(result.name, "8");

        let result = calc.evaluate("= 10 * 2").unwrap();
        assert_eq!(result.name, "20");

        let result = calc.evaluate("= 15 / 3").unwrap();
        assert_eq!(result.name, "5");
    }

    #[test]
    fn test_evaluate_float() {
        let mut calc = CalculatorProvider::new();

        let result = calc.evaluate("= 5/2").unwrap();
        assert_eq!(result.name, "2.5");

        let result = calc.evaluate("= 1/3").unwrap();
        assert!(result.name.starts_with("0.333"));
    }

    #[test]
    fn test_evaluate_functions() {
        let mut calc = CalculatorProvider::new();

        let result = calc.evaluate("= sqrt(16)").unwrap();
        assert_eq!(result.name, "4");

        let result = calc.evaluate("= abs(-5)").unwrap();
        assert_eq!(result.name, "5");
    }

    #[test]
    fn test_evaluate_constants() {
        let mut calc = CalculatorProvider::new();

        let result = calc.evaluate("= pi").unwrap();
        assert!(result.name.starts_with("3.14159"));

        let result = calc.evaluate("= e").unwrap();
        assert!(result.name.starts_with("2.718"));
    }

    #[test]
    fn test_evaluate_invalid() {
        let mut calc = CalculatorProvider::new();

        assert!(calc.evaluate("= ").is_none());
        assert!(calc.evaluate("= invalid").is_none());
        assert!(calc.evaluate("= 5 +").is_none());
    }
}
