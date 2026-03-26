//! Math calculation API for Lua plugins
//!
//! Provides safe math expression evaluation:
//! - `owlry.math.calculate(expression)` - Evaluate a math expression

use mlua::{Lua, Result as LuaResult, Table};

/// Register math APIs
pub fn register_math_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let math_table = lua.create_table()?;

    // owlry.math.calculate(expression) -> number or nil, error
    // Evaluates a mathematical expression safely
    // Returns (result, nil) on success or (nil, error_message) on failure
    math_table.set(
        "calculate",
        lua.create_function(
            |_lua, expr: String| -> LuaResult<(Option<f64>, Option<String>)> {
                match meval::eval_str(&expr) {
                    Ok(result) => {
                        if result.is_finite() {
                            Ok((Some(result), None))
                        } else {
                            Ok((None, Some("Result is not a finite number".to_string())))
                        }
                    }
                    Err(e) => Ok((None, Some(e.to_string()))),
                }
            },
        )?,
    )?;

    // owlry.math.calc(expression) -> number (throws on error)
    // Convenience function that throws instead of returning error
    math_table.set(
        "calc",
        lua.create_function(|_lua, expr: String| {
            meval::eval_str(&expr)
                .map_err(|e| mlua::Error::external(format!("Math error: {}", e)))
                .and_then(|r| {
                    if r.is_finite() {
                        Ok(r)
                    } else {
                        Err(mlua::Error::external("Result is not a finite number"))
                    }
                })
        })?,
    )?;

    // owlry.math.is_expression(str) -> boolean
    // Check if a string looks like a math expression
    math_table.set(
        "is_expression",
        lua.create_function(|_lua, expr: String| {
            let trimmed = expr.trim();

            // Must have at least one digit
            if !trimmed.chars().any(|c| c.is_ascii_digit()) {
                return Ok(false);
            }

            // Should only contain valid math characters
            let valid = trimmed.chars().all(|c| {
                c.is_ascii_digit()
                    || c.is_ascii_alphabetic()
                    || matches!(c, '+' | '-' | '*' | '/' | '^' | '(' | ')' | '.' | ' ' | '%')
            });

            Ok(valid)
        })?,
    )?;

    // owlry.math.format(number, decimals?) -> string
    // Format a number with optional decimal places
    math_table.set(
        "format",
        lua.create_function(|_lua, (num, decimals): (f64, Option<usize>)| {
            let decimals = decimals.unwrap_or(2);

            // Check if it's effectively an integer
            if (num - num.round()).abs() < f64::EPSILON {
                Ok(format!("{}", num as i64))
            } else {
                Ok(format!("{:.prec$}", num, prec = decimals))
            }
        })?,
    )?;

    owlry.set("math", math_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_math_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_calculate_basic() {
        let lua = setup_lua();

        let chunk = lua.load(
            r#"
            local result, err = owlry.math.calculate("2 + 2")
            if err then error(err) end
            return result
        "#,
        );
        let result: f64 = chunk.call(()).unwrap();
        assert!((result - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_complex() {
        let lua = setup_lua();

        let chunk = lua.load(
            r#"
            local result, err = owlry.math.calculate("sqrt(16) + 2^3")
            if err then error(err) end
            return result
        "#,
        );
        let result: f64 = chunk.call(()).unwrap();
        assert!((result - 12.0).abs() < f64::EPSILON); // sqrt(16) = 4, 2^3 = 8
    }

    #[test]
    fn test_calculate_error() {
        let lua = setup_lua();

        let chunk = lua.load(
            r#"
            local result, err = owlry.math.calculate("invalid expression @@")
            if result then
                return false -- should not succeed
            else
                return true -- correctly failed
            end
        "#,
        );
        let had_error: bool = chunk.call(()).unwrap();
        assert!(had_error);
    }

    #[test]
    fn test_calc_throws() {
        let lua = setup_lua();

        let chunk = lua.load(r#"return owlry.math.calc("3 * 4")"#);
        let result: f64 = chunk.call(()).unwrap();
        assert!((result - 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_is_expression() {
        let lua = setup_lua();

        let chunk = lua.load(r#"return owlry.math.is_expression("2 + 2")"#);
        let is_expr: bool = chunk.call(()).unwrap();
        assert!(is_expr);

        let chunk = lua.load(r#"return owlry.math.is_expression("hello world")"#);
        let is_expr: bool = chunk.call(()).unwrap();
        assert!(!is_expr);
    }

    #[test]
    fn test_format() {
        let lua = setup_lua();

        let chunk = lua.load(r#"return owlry.math.format(3.14159, 2)"#);
        let formatted: String = chunk.call(()).unwrap();
        assert_eq!(formatted, "3.14");

        let chunk = lua.load(r#"return owlry.math.format(42.0)"#);
        let formatted: String = chunk.call(()).unwrap();
        assert_eq!(formatted, "42");
    }
}
