use super::units;

pub struct ParsedQuery {
    pub value: f64,
    pub from_unit: String,
    pub from_symbol: String,
    pub target_unit: Option<String>,
}

pub fn parse_conversion(input: &str) -> Option<ParsedQuery> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Extract leading number
    let (value, rest) = extract_number(input)?;
    let rest = rest.trim();

    if rest.is_empty() {
        return None;
    }

    // Split on " to " or " in " (case-insensitive)
    let (from_str, target_str) = split_on_connector(rest);

    // Resolve from unit
    let from_lower = from_str.trim().to_lowercase();
    let from_symbol = units::find_unit(&from_lower)?;

    let from_symbol_str = from_symbol.to_string();

    // Resolve target unit if present
    let target_unit = target_str.and_then(|t| {
        let t_lower = t.trim().to_lowercase();
        if t_lower.is_empty() {
            None
        } else {
            units::find_unit(&t_lower).map(|_| t_lower)
        }
    });

    Some(ParsedQuery {
        value,
        from_unit: from_lower,
        from_symbol: from_symbol_str,
        target_unit,
    })
}

fn extract_number(input: &str) -> Option<(f64, &str)> {
    let bytes = input.as_bytes();
    let mut i = 0;

    // Optional negative sign
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }

    // Must have at least one digit or start with .
    if i >= bytes.len() {
        return None;
    }

    let start_digits = i;

    // Integer part
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }

    // Decimal part
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }

    if i == start_digits && !(i > 0 && bytes[0] == b'-') {
        // No digits found (and not just a negative sign before a dot)
        // Handle ".5" case
        if bytes[start_digits] == b'.' {
            // already advanced past dot above
        } else {
            return None;
        }
    }

    if i == 0 || (i == 1 && bytes[0] == b'-') {
        return None;
    }

    let num_str = &input[..i];
    let value: f64 = num_str.parse().ok()?;
    let rest = &input[i..];

    Some((value, rest))
}

fn split_on_connector(input: &str) -> (&str, Option<&str>) {
    let lower = input.to_lowercase();

    // Try " to " first
    if let Some(pos) = lower.find(" to ") {
        let from = &input[..pos];
        let target = &input[pos + 4..];
        return (from, Some(target));
    }

    // Try " in "
    if let Some(pos) = lower.find(" in ") {
        let from = &input[..pos];
        let target = &input[pos + 4..];
        return (from, Some(target));
    }

    (input, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_number_and_unit_with_space() {
        let p = parse_conversion("100 km").unwrap();
        assert!((p.value - 100.0).abs() < 0.001);
        assert_eq!(p.from_unit, "km");
        assert!(p.target_unit.is_none());
    }

    #[test]
    fn test_number_and_unit_no_space() {
        let p = parse_conversion("100km").unwrap();
        assert!((p.value - 100.0).abs() < 0.001);
        assert_eq!(p.from_unit, "km");
    }

    #[test]
    fn test_with_target_to() {
        let p = parse_conversion("100 km to mi").unwrap();
        assert!((p.value - 100.0).abs() < 0.001);
        assert_eq!(p.from_unit, "km");
        assert_eq!(p.target_unit.as_deref(), Some("mi"));
    }

    #[test]
    fn test_with_target_in() {
        let p = parse_conversion("100 km in mi").unwrap();
        assert_eq!(p.target_unit.as_deref(), Some("mi"));
    }

    #[test]
    fn test_temperature_no_space() {
        let p = parse_conversion("102F to C").unwrap();
        assert!((p.value - 102.0).abs() < 0.001);
        assert_eq!(p.from_unit, "f");
        assert_eq!(p.target_unit.as_deref(), Some("c"));
    }

    #[test]
    fn test_temperature_with_space() {
        let p = parse_conversion("102 F in K").unwrap();
        assert!((p.value - 102.0).abs() < 0.001);
        assert_eq!(p.from_unit, "f");
        assert_eq!(p.target_unit.as_deref(), Some("k"));
    }

    #[test]
    fn test_decimal_number() {
        let p = parse_conversion("3.5 kg to lb").unwrap();
        assert!((p.value - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_decimal_starting_with_dot() {
        let p = parse_conversion(".5 kg").unwrap();
        assert!((p.value - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_full_unit_names() {
        let p = parse_conversion("100 kilometers to miles").unwrap();
        assert_eq!(p.from_unit, "kilometers");
        assert_eq!(p.target_unit.as_deref(), Some("miles"));
    }

    #[test]
    fn test_case_insensitive() {
        let p = parse_conversion("100 KM TO MI").unwrap();
        assert_eq!(p.from_unit, "km");
        assert_eq!(p.target_unit.as_deref(), Some("mi"));
    }

    #[test]
    fn test_currency() {
        let p = parse_conversion("100 eur to usd").unwrap();
        assert_eq!(p.from_unit, "eur");
        assert_eq!(p.target_unit.as_deref(), Some("usd"));
    }

    #[test]
    fn test_no_number_returns_none() {
        assert!(parse_conversion("km to mi").is_none());
    }

    #[test]
    fn test_unknown_unit_returns_none() {
        assert!(parse_conversion("100 xyz to abc").is_none());
    }

    #[test]
    fn test_empty_returns_none() {
        assert!(parse_conversion("").is_none());
    }

    #[test]
    fn test_number_only_returns_none() {
        assert!(parse_conversion("100").is_none());
    }

    #[test]
    fn test_compound_unit_alias() {
        let p = parse_conversion("100 km/h to mph").unwrap();
        assert_eq!(p.from_unit, "km/h");
        assert_eq!(p.target_unit.as_deref(), Some("mph"));
    }

    #[test]
    fn test_multi_word_unit() {
        let p = parse_conversion("100 fl_oz to ml").unwrap();
        assert_eq!(p.from_unit, "fl_oz");
    }
}
