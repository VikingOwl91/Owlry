use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const CACHE_MAX_AGE_SECS: u64 = 86400; // 24 hours
// Keep the fetch deadline below the IPC client's five-second read timeout so
// a stalled response cannot arrive after the client has moved on and be
// mistaken for the answer to the next query.
const FETCH_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const FETCH_TOTAL_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

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
    fetch_rates_from(ECB_URL, FETCH_TOTAL_TIMEOUT)
}

fn fetch_rates_from(url: &str, timeout: Duration) -> Option<CurrencyRates> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(FETCH_CONNECT_TIMEOUT.min(timeout))
        .timeout(timeout)
        .build()
        .ok()?;
    let response = client.get(url).send().ok()?.error_for_status().ok()?;

    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES)
    {
        return None;
    }

    let mut body = String::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_string(&mut body)
        .ok()?;
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return None;
    }

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

        // Extract rate: <Cube currency='USD' rate='1.0832'/>. A single
        // malformed line should not discard every rate in the document.
        if trimmed.contains("currency=") && trimmed.contains("rate=") {
            let (Some(currency), Some(rate_str)) = (
                extract_attr(trimmed, "currency"),
                extract_attr(trimmed, "rate"),
            ) else {
                continue;
            };
            let Ok(rate) = rate_str.parse::<f64>() else {
                continue;
            };
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
    fn malformed_rate_line_does_not_discard_the_document() {
        let xml = "<Cube time='2026-10-27'>\n\
                   <Cube currency='USD' rate='1.1'/>\n\
                   <Cube currency='BROKEN' rate='not-a-number'/>\n\
                   <Cube currency='GBP' rate='0.8'/>\n\
                   </Cube>";
        let rates = parse_ecb_xml(xml).unwrap();
        assert_eq!(rates.rates["USD"], 1.1);
        assert_eq!(rates.rates["GBP"], 0.8);
        assert!(!rates.rates.contains_key("BROKEN"));
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

    fn serve_once(body: Vec<u8>, content_length: Option<usize>, delay: Duration) -> String {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            let header = match content_length {
                Some(len) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nContent-Type: application/xml\r\nConnection: close\r\n\r\n"
                ),
                None => {
                    "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nConnection: close\r\n\r\n"
                        .to_string()
                }
            };
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        });
        format!("http://{addr}/")
    }

    #[test]
    fn fetch_parses_a_bounded_response() {
        let body = br#"<Cube time='2026-10-27'><Cube currency='USD' rate='1.1'/></Cube>"#;
        let url = serve_once(body.to_vec(), Some(body.len()), Duration::ZERO);
        let rates = fetch_rates_from(&url, Duration::from_secs(1)).unwrap();
        assert_eq!(rates.rates["USD"], 1.1);
    }

    #[test]
    fn fetch_rejects_an_oversized_content_length() {
        let url = serve_once(
            vec![b'x'; 32],
            Some(MAX_RESPONSE_BYTES as usize + 1),
            Duration::ZERO,
        );
        assert!(fetch_rates_from(&url, Duration::from_secs(1)).is_none());
    }

    #[test]
    fn fetch_caps_a_stream_without_content_length() {
        let body = vec![b'x'; MAX_RESPONSE_BYTES as usize + 1024];
        let url = serve_once(body, None, Duration::ZERO);
        assert!(fetch_rates_from(&url, Duration::from_secs(2)).is_none());
    }

    #[test]
    fn fetch_times_out_on_a_stalled_server() {
        let body = br#"<Cube time='2026-10-27'><Cube currency='USD' rate='1.1'/></Cube>"#;
        let url = serve_once(body.to_vec(), Some(body.len()), Duration::from_millis(500));
        assert!(fetch_rates_from(&url, Duration::from_millis(50)).is_none());
    }
}
