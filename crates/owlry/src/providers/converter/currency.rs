use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const CACHE_MAX_AGE_SECS: u64 = 86400; // 24 hours

static CACHED_RATES: Mutex<Option<CurrencyRates>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyRates {
    pub date: String,
    pub rates: HashMap<String, f64>,
}

struct CurrencyAlias {
    code: &'static str,
    aliases: &'static [&'static str],
}

static CURRENCY_ALIASES: &[CurrencyAlias] = &[
    CurrencyAlias {
        code: "EUR",
        aliases: &["eur", "euro", "euros", "€"],
    },
    CurrencyAlias {
        code: "USD",
        aliases: &["usd", "dollar", "dollars", "$", "us_dollar"],
    },
    CurrencyAlias {
        code: "GBP",
        aliases: &["gbp", "pound_sterling", "£", "british_pound", "pounds"],
    },
    CurrencyAlias {
        code: "JPY",
        aliases: &["jpy", "yen", "¥", "japanese_yen"],
    },
    CurrencyAlias {
        code: "CHF",
        aliases: &["chf", "swiss_franc", "francs"],
    },
    CurrencyAlias {
        code: "CAD",
        aliases: &["cad", "canadian_dollar", "c$"],
    },
    CurrencyAlias {
        code: "AUD",
        aliases: &["aud", "australian_dollar", "a$"],
    },
    CurrencyAlias {
        code: "CNY",
        aliases: &["cny", "yuan", "renminbi", "rmb"],
    },
    CurrencyAlias {
        code: "SEK",
        aliases: &["sek", "swedish_krona", "kronor"],
    },
    CurrencyAlias {
        code: "NOK",
        aliases: &["nok", "norwegian_krone"],
    },
    CurrencyAlias {
        code: "DKK",
        aliases: &["dkk", "danish_krone"],
    },
    CurrencyAlias {
        code: "PLN",
        aliases: &["pln", "zloty", "złoty"],
    },
    CurrencyAlias {
        code: "CZK",
        aliases: &["czk", "czech_koruna"],
    },
    CurrencyAlias {
        code: "HUF",
        aliases: &["huf", "forint"],
    },
    CurrencyAlias {
        code: "TRY",
        aliases: &["try", "turkish_lira", "lira"],
    },
];

pub fn resolve_currency_code(alias: &str) -> Option<&'static str> {
    let lower = alias.to_lowercase();

    // Check aliases
    for ca in CURRENCY_ALIASES {
        if ca.aliases.contains(&lower.as_str()) {
            return Some(ca.code); // ca.code is already &'static str
        }
    }

    // Check if it's a raw 3-letter ISO code we know about
    let upper = alias.to_uppercase();
    if upper.len() == 3 {
        if upper == "EUR" {
            return Some("EUR");
        }
        if let Some(rates) = get_rates()
            && rates.rates.contains_key(&upper)
        {
            for ca in CURRENCY_ALIASES {
                if ca.code == upper {
                    return Some(ca.code);
                }
            }
        }
    }

    None
}

#[allow(dead_code)]
pub fn is_currency_alias(alias: &str) -> bool {
    resolve_currency_code(alias).is_some()
}

pub fn get_rates() -> Option<CurrencyRates> {
    // Check memory cache first
    {
        let cache = CACHED_RATES.lock().ok()?;
        if let Some(ref rates) = *cache {
            return Some(rates.clone());
        }
    }

    // Try disk cache
    if let Some(rates) = load_cache()
        && !is_stale(&rates)
    {
        let mut cache = CACHED_RATES.lock().ok()?;
        *cache = Some(rates.clone());
        return Some(rates);
    }

    // Fetch fresh rates
    if let Some(rates) = fetch_rates() {
        save_cache(&rates);
        let mut cache = CACHED_RATES.lock().ok()?;
        *cache = Some(rates.clone());
        return Some(rates);
    }

    // Fall back to stale cache
    load_cache()
}

fn cache_path() -> Option<PathBuf> {
    let cache_dir = dirs::cache_dir()?.join("owlry");
    Some(cache_dir.join("ecb_rates.json"))
}

fn load_cache() -> Option<CurrencyRates> {
    let path = cache_path()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_cache(rates: &CurrencyRates) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        if let Ok(json) = serde_json::to_string_pretty(rates) {
            fs::write(path, json).ok();
        }
    }
}

fn is_stale(_rates: &CurrencyRates) -> bool {
    let path = match cache_path() {
        Some(p) => p,
        None => return true,
    };
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return true,
    };
    let modified = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return true,
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age.as_secs() > CACHE_MAX_AGE_SECS,
        Err(_) => true,
    }
}

fn fetch_rates() -> Option<CurrencyRates> {
    let response = reqwest::blocking::get(ECB_URL).ok()?;
    let body = response.text().ok()?;
    parse_ecb_xml(&body)
}

fn parse_ecb_xml(xml: &str) -> Option<CurrencyRates> {
    let mut rates = HashMap::new();
    let mut date = String::new();

    for line in xml.lines() {
        let trimmed = line.trim();

        // Extract date: <Cube time='2026-03-26'>
        if trimmed.contains("time=")
            && let Some(start) = trimmed.find("time='")
        {
            let rest = &trimmed[start + 6..];
            if let Some(end) = rest.find('\'') {
                date = rest[..end].to_string();
            }
        }

        // Extract rate: <Cube currency='USD' rate='1.0832'/>
        if trimmed.contains("currency=") && trimmed.contains("rate=") {
            let currency = extract_attr(trimmed, "currency")?;
            let rate_str = extract_attr(trimmed, "rate")?;
            let rate: f64 = rate_str.parse().ok()?;
            rates.insert(currency, rate);
        }
    }

    if rates.is_empty() {
        return None;
    }

    Some(CurrencyRates { date, rates })
}

fn extract_attr(line: &str, attr: &str) -> Option<String> {
    let needle = format!("{}='", attr);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_currency_code_iso() {
        assert_eq!(resolve_currency_code("usd"), Some("USD"));
        assert_eq!(resolve_currency_code("EUR"), Some("EUR"));
    }

    #[test]
    fn test_resolve_currency_code_name() {
        assert_eq!(resolve_currency_code("dollar"), Some("USD"));
        assert_eq!(resolve_currency_code("euro"), Some("EUR"));
        assert_eq!(resolve_currency_code("pounds"), Some("GBP"));
    }

    #[test]
    fn test_resolve_currency_code_symbol() {
        assert_eq!(resolve_currency_code("$"), Some("USD"));
        assert_eq!(resolve_currency_code("€"), Some("EUR"));
        assert_eq!(resolve_currency_code("£"), Some("GBP"));
    }

    #[test]
    fn test_resolve_currency_unknown() {
        assert_eq!(resolve_currency_code("xyz"), None);
    }

    #[test]
    fn test_is_currency_alias() {
        assert!(is_currency_alias("usd"));
        assert!(is_currency_alias("euro"));
        assert!(is_currency_alias("$"));
        assert!(!is_currency_alias("km"));
    }

    #[test]
    fn test_parse_ecb_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
    <gesmes:subject>Reference rates</gesmes:subject>
    <Cube>
        <Cube time='2026-03-26'>
            <Cube currency='USD' rate='1.0832'/>
            <Cube currency='JPY' rate='161.94'/>
            <Cube currency='GBP' rate='0.83450'/>
        </Cube>
    </Cube>
</gesmes:Envelope>"#;

        let rates = parse_ecb_xml(xml).unwrap();
        assert!((rates.rates["USD"] - 1.0832).abs() < 0.001);
        assert!((rates.rates["GBP"] - 0.8345).abs() < 0.001);
        assert!((rates.rates["JPY"] - 161.94).abs() < 0.01);
    }

    #[test]
    fn test_cache_roundtrip() {
        let rates = CurrencyRates {
            date: "2026-03-26".to_string(),
            rates: {
                let mut m = HashMap::new();
                m.insert("USD".to_string(), 1.0832);
                m.insert("GBP".to_string(), 0.8345);
                m
            },
        };
        let json = serde_json::to_string(&rates).unwrap();
        let parsed: CurrencyRates = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rates["USD"], 1.0832);
    }
}
