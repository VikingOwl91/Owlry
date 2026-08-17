mod currency;
mod parser;
mod units;

use super::{DynamicProvider, ItemSource, LaunchItem, ProviderType};

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

    fn priority(&self) -> u32 {
        9_000
    }

    fn prefix(&self) -> Option<&str> {
        Some(":conv")
    }

    fn icon(&self) -> &str {
        PROVIDER_ICON
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Convert")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("unit / currency conversion")
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        let query_str = query.trim();
        // Strip prefix
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
            .map(|r| LaunchItem {
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
                command: format!(
                    "printf '%s' '{}' | wl-copy",
                    r.raw_value.replace('\'', "'\\''")
                ),
                terminal: false,
                tags: vec!["converter".into(), "units".into()],
                source: ItemSource::Core,
            })
            .collect()
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
    fn test_common_conversions() {
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
        assert_eq!(
            ConverterProvider::new().provider_type(),
            ProviderType::Plugin("conv".into())
        );
    }

    #[test]
    fn test_no_double_unit() {
        let r = query("100 km to mi");
        if let Some(item) = r.first() {
            let desc = item.description.as_deref().unwrap();
            assert!(!desc.ends_with(" mi mi"), "double unit in: {}", desc);
        }
    }

    #[test]
    fn test_format_number_integer() {
        assert_eq!(format_number(42.0), "42");
    }

    #[test]
    fn test_format_number_large_integer() {
        assert_eq!(format_number(1000000.0), "1,000,000");
    }

    #[test]
    fn test_format_number_decimal() {
        assert_eq!(format_number(3.25), "3.25");
    }

    #[test]
    fn test_format_with_separators() {
        assert_eq!(format_with_separators(1234567), "1,234,567");
        assert_eq!(format_with_separators(999), "999");
        assert_eq!(format_with_separators(-1234), "-1,234");
    }
}
