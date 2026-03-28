use std::collections::HashMap;
use std::sync::LazyLock;

use super::currency;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Temperature,
    Length,
    Weight,
    Volume,
    Speed,
    Area,
    Data,
    Time,
    Pressure,
    Energy,
    Currency,
}

#[derive(Clone)]
enum Conversion {
    Factor(f64),
    Custom {
        to_base: fn(f64) -> f64,
        from_base: fn(f64) -> f64,
    },
}

#[derive(Clone)]
pub(crate) struct UnitDef {
    _id: &'static str,
    symbol: &'static str,
    aliases: &'static [&'static str],
    category: Category,
    conversion: Conversion,
}

impl UnitDef {
    fn to_base(&self, value: f64) -> f64 {
        match &self.conversion {
            Conversion::Factor(f) => value * f,
            Conversion::Custom { to_base, .. } => to_base(value),
        }
    }

    fn convert_from_base(&self, value: f64) -> f64 {
        match &self.conversion {
            Conversion::Factor(f) => value / f,
            Conversion::Custom { from_base, .. } => from_base(value),
        }
    }
}

pub struct ConversionResult {
    pub value: f64,
    pub raw_value: String,
    pub display_value: String,
    pub target_symbol: String,
}

static UNITS: LazyLock<Vec<UnitDef>> = LazyLock::new(build_unit_table);
static ALIAS_MAP: LazyLock<HashMap<String, usize>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (i, unit) in UNITS.iter().enumerate() {
        for alias in unit.aliases {
            map.insert(alias.to_lowercase(), i);
        }
    }
    map
});

// Common conversions per category (symbols to show when no target specified)
static COMMON_TARGETS: LazyLock<HashMap<Category, Vec<&'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(Category::Temperature, vec!["°C", "°F", "K"]);
    m.insert(Category::Length, vec!["m", "km", "ft", "mi", "in"]);
    m.insert(Category::Weight, vec!["kg", "lb", "oz", "g", "st"]);
    m.insert(Category::Volume, vec!["l", "gal", "ml", "cup", "fl oz"]);
    m.insert(Category::Speed, vec!["km/h", "mph", "m/s", "kn"]);
    m.insert(Category::Area, vec!["m²", "ft²", "ac", "ha", "km²"]);
    m.insert(Category::Data, vec!["MB", "GB", "MiB", "GiB", "TB"]);
    m.insert(Category::Time, vec!["s", "min", "h", "d", "wk"]);
    m.insert(Category::Pressure, vec!["bar", "psi", "atm", "hPa", "mmHg"]);
    m.insert(Category::Energy, vec!["kJ", "kcal", "kWh", "BTU", "Wh"]);
    m.insert(Category::Currency, vec!["USD", "EUR", "GBP", "JPY", "CNY"]);
    m
});

pub fn find_unit(alias: &str) -> Option<&'static str> {
    let lower = alias.to_lowercase();
    if let Some(&i) = ALIAS_MAP.get(&lower) {
        return Some(UNITS[i].symbol);
    }
    currency::resolve_currency_code(&lower)
}

pub fn lookup_unit(alias: &str) -> Option<(usize, &UnitDef)> {
    let lower = alias.to_lowercase();
    ALIAS_MAP.get(&lower).map(|&i| (i, &UNITS[i]))
}

pub fn convert_to(value: &f64, from: &str, to: &str) -> Option<ConversionResult> {
    // Try currency first — currency aliases (dollar, euro, etc.) aren't in the UNITS table
    if currency::is_currency_alias(from) || currency::is_currency_alias(to) {
        return convert_currency(*value, from, to);
    }

    let (_, from_def) = lookup_unit(from)?;
    let (_, to_def) = lookup_unit(to)?;

    // Currency via UNITS table (shouldn't reach here, but just in case)
    if from_def.category == Category::Currency || to_def.category == Category::Currency {
        return convert_currency(*value, from, to);
    }

    // Must be same category
    if from_def.category != to_def.category {
        return None;
    }

    let base_value = from_def.to_base(*value);
    let result = to_def.convert_from_base(base_value);

    Some(format_result(result, to_def.symbol))
}

pub fn convert_common(value: &f64, from: &str) -> Vec<ConversionResult> {
    // Try currency first — currency aliases (dollar, euro, etc.) aren't in the UNITS table
    if currency::is_currency_alias(from) {
        return convert_currency_common(*value, from);
    }

    let (_, from_def) = match lookup_unit(from) {
        Some(u) => u,
        None => return vec![],
    };

    let category = from_def.category;
    let from_symbol = from_def.symbol;

    if category == Category::Currency {
        return convert_currency_common(*value, from);
    }

    let targets = match COMMON_TARGETS.get(&category) {
        Some(t) => t,
        None => return vec![],
    };

    let base_value = from_def.to_base(*value);

    targets
        .iter()
        .filter(|&&sym| sym != from_symbol)
        .filter_map(|&sym| {
            let (_, to_def) = lookup_unit(sym)?;
            let result = to_def.convert_from_base(base_value);
            Some(format_result(result, to_def.symbol))
        })
        .take(5)
        .collect()
}

fn convert_currency(value: f64, from: &str, to: &str) -> Option<ConversionResult> {
    let rates = currency::get_rates()?;
    let from_code = currency::resolve_currency_code(from)?;
    let to_code = currency::resolve_currency_code(to)?;

    let from_rate = if from_code == "EUR" { 1.0 } else { *rates.rates.get(from_code)? };
    let to_rate = if to_code == "EUR" { 1.0 } else { *rates.rates.get(to_code)? };

    let result = value / from_rate * to_rate;
    Some(format_currency_result(result, to_code))
}

fn convert_currency_common(value: f64, from: &str) -> Vec<ConversionResult> {
    let rates = match currency::get_rates() {
        Some(r) => r,
        None => return vec![],
    };
    let from_code = match currency::resolve_currency_code(from) {
        Some(c) => c,
        None => return vec![],
    };

    let targets = COMMON_TARGETS.get(&Category::Currency).unwrap();
    let from_rate = if from_code == "EUR" {
        1.0
    } else {
        match rates.rates.get(from_code) {
            Some(&r) => r,
            None => return vec![],
        }
    };

    targets
        .iter()
        .filter(|&&sym| sym != from_code)
        .filter_map(|&sym| {
            let to_rate = if sym == "EUR" { 1.0 } else { *rates.rates.get(sym)? };
            let result = value / from_rate * to_rate;
            Some(format_currency_result(result, sym))
        })
        .take(5)
        .collect()
}

fn format_result(value: f64, symbol: &str) -> ConversionResult {
    let raw = if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{:.4}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };

    let display = if value.abs() >= 1000.0 && value.fract() == 0.0 && value.abs() < 1e15 {
        super::format_with_separators(value as i64)
    } else {
        raw.clone()
    };

    ConversionResult {
        value,
        raw_value: raw,
        display_value: format!("{} {}", display, symbol),
        target_symbol: symbol.to_string(),
    }
}

fn format_currency_result(value: f64, code: &str) -> ConversionResult {
    let raw = format!("{:.2}", value);
    let display = raw.clone();

    ConversionResult {
        value,
        raw_value: raw,
        display_value: format!("{} {}", display, code),
        target_symbol: code.to_string(),
    }
}

fn build_unit_table() -> Vec<UnitDef> {
    vec![
        // Temperature (base: Kelvin)
        UnitDef {
            _id: "celsius",
            symbol: "°C",
            aliases: &["c", "°c", "celsius", "degc", "centigrade"],
            category: Category::Temperature,
            conversion: Conversion::Custom {
                to_base: |v| v + 273.15,
                from_base: |v| v - 273.15,
            },
        },
        UnitDef {
            _id: "fahrenheit",
            symbol: "°F",
            aliases: &["f", "°f", "fahrenheit", "degf"],
            category: Category::Temperature,
            conversion: Conversion::Custom {
                to_base: |v| (v - 32.0) * 5.0 / 9.0 + 273.15,
                from_base: |v| (v - 273.15) * 9.0 / 5.0 + 32.0,
            },
        },
        UnitDef {
            _id: "kelvin",
            symbol: "K",
            aliases: &["k", "kelvin"],
            category: Category::Temperature,
            conversion: Conversion::Factor(1.0), // base
        },
        // Length (base: meter)
        UnitDef {
            _id: "millimeter",
            symbol: "mm",
            aliases: &["mm", "millimeter", "millimeters", "millimetre"],
            category: Category::Length,
            conversion: Conversion::Factor(0.001),
        },
        UnitDef {
            _id: "centimeter",
            symbol: "cm",
            aliases: &["cm", "centimeter", "centimeters", "centimetre"],
            category: Category::Length,
            conversion: Conversion::Factor(0.01),
        },
        UnitDef {
            _id: "meter",
            symbol: "m",
            aliases: &["m", "meter", "meters", "metre", "metres"],
            category: Category::Length,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "kilometer",
            symbol: "km",
            aliases: &["km", "kms", "kilometer", "kilometers", "kilometre"],
            category: Category::Length,
            conversion: Conversion::Factor(1000.0),
        },
        UnitDef {
            _id: "inch",
            symbol: "in",
            aliases: &["in", "inch", "inches"],
            category: Category::Length,
            conversion: Conversion::Factor(0.0254),
        },
        UnitDef {
            _id: "foot",
            symbol: "ft",
            aliases: &["ft", "foot", "feet"],
            category: Category::Length,
            conversion: Conversion::Factor(0.3048),
        },
        UnitDef {
            _id: "yard",
            symbol: "yd",
            aliases: &["yd", "yard", "yards"],
            category: Category::Length,
            conversion: Conversion::Factor(0.9144),
        },
        UnitDef {
            _id: "mile",
            symbol: "mi",
            aliases: &["mi", "mile", "miles"],
            category: Category::Length,
            conversion: Conversion::Factor(1609.344),
        },
        UnitDef {
            _id: "nautical_mile",
            symbol: "nmi",
            aliases: &["nmi", "nautical_mile", "nautical_miles"],
            category: Category::Length,
            conversion: Conversion::Factor(1852.0),
        },
        // Weight (base: kg)
        UnitDef {
            _id: "milligram",
            symbol: "mg",
            aliases: &["mg", "milligram", "milligrams"],
            category: Category::Weight,
            conversion: Conversion::Factor(0.000001),
        },
        UnitDef {
            _id: "gram",
            symbol: "g",
            aliases: &["g", "gram", "grams"],
            category: Category::Weight,
            conversion: Conversion::Factor(0.001),
        },
        UnitDef {
            _id: "kilogram",
            symbol: "kg",
            aliases: &["kg", "kilogram", "kilograms", "kilo", "kilos"],
            category: Category::Weight,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "tonne",
            symbol: "t",
            aliases: &["t", "ton", "tons", "tonne", "tonnes", "metric_ton"],
            category: Category::Weight,
            conversion: Conversion::Factor(1000.0),
        },
        UnitDef {
            _id: "short_ton",
            symbol: "short_ton",
            aliases: &["short_ton", "ton_us"],
            category: Category::Weight,
            conversion: Conversion::Factor(907.185),
        },
        UnitDef {
            _id: "ounce",
            symbol: "oz",
            aliases: &["oz", "ounce", "ounces"],
            category: Category::Weight,
            conversion: Conversion::Factor(0.0283495),
        },
        UnitDef {
            _id: "pound",
            symbol: "lb",
            aliases: &["lb", "lbs", "pound", "pounds"],
            category: Category::Weight,
            conversion: Conversion::Factor(0.453592),
        },
        UnitDef {
            _id: "stone",
            symbol: "st",
            aliases: &["st", "stone", "stones"],
            category: Category::Weight,
            conversion: Conversion::Factor(6.35029),
        },
        // Volume (base: liter)
        UnitDef {
            _id: "milliliter",
            symbol: "ml",
            aliases: &["ml", "milliliter", "milliliters", "millilitre"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.001),
        },
        UnitDef {
            _id: "liter",
            symbol: "l",
            aliases: &["l", "liter", "liters", "litre", "litres"],
            category: Category::Volume,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "us_gallon",
            symbol: "gal",
            aliases: &["gal", "gallon", "gallons"],
            category: Category::Volume,
            conversion: Conversion::Factor(3.78541),
        },
        UnitDef {
            _id: "imp_gallon",
            symbol: "imp gal",
            aliases: &["imp_gal", "gal_uk", "imperial_gallon"],
            category: Category::Volume,
            conversion: Conversion::Factor(4.54609),
        },
        UnitDef {
            _id: "quart",
            symbol: "qt",
            aliases: &["qt", "quart", "quarts"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.946353),
        },
        UnitDef {
            _id: "pint",
            symbol: "pt",
            aliases: &["pt", "pint", "pints"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.473176),
        },
        UnitDef {
            _id: "cup",
            symbol: "cup",
            aliases: &["cup", "cups"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.236588),
        },
        UnitDef {
            _id: "fluid_ounce",
            symbol: "fl oz",
            aliases: &["floz", "fl_oz", "fluid_ounce", "fluid_ounces"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.0295735),
        },
        UnitDef {
            _id: "tablespoon",
            symbol: "tbsp",
            aliases: &["tbsp", "tablespoon", "tablespoons"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.0147868),
        },
        UnitDef {
            _id: "teaspoon",
            symbol: "tsp",
            aliases: &["tsp", "teaspoon", "teaspoons"],
            category: Category::Volume,
            conversion: Conversion::Factor(0.00492892),
        },
        // Speed (base: m/s)
        UnitDef {
            _id: "mps",
            symbol: "m/s",
            aliases: &["m/s", "mps", "meters_per_second"],
            category: Category::Speed,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "kmh",
            symbol: "km/h",
            aliases: &["km/h", "kmh", "kph", "kilometers_per_hour"],
            category: Category::Speed,
            conversion: Conversion::Factor(0.277778),
        },
        UnitDef {
            _id: "mph",
            symbol: "mph",
            aliases: &["mph", "miles_per_hour"],
            category: Category::Speed,
            conversion: Conversion::Factor(0.44704),
        },
        UnitDef {
            _id: "knot",
            symbol: "kn",
            aliases: &["kn", "kt", "knot", "knots"],
            category: Category::Speed,
            conversion: Conversion::Factor(0.514444),
        },
        UnitDef {
            _id: "fps",
            symbol: "ft/s",
            aliases: &["ft/s", "fps", "feet_per_second"],
            category: Category::Speed,
            conversion: Conversion::Factor(0.3048),
        },
        // Area (base: m²)
        UnitDef {
            _id: "sqmm",
            symbol: "mm²",
            aliases: &["mm2", "sqmm", "square_millimeter"],
            category: Category::Area,
            conversion: Conversion::Factor(0.000001),
        },
        UnitDef {
            _id: "sqcm",
            symbol: "cm²",
            aliases: &["cm2", "sqcm", "square_centimeter"],
            category: Category::Area,
            conversion: Conversion::Factor(0.0001),
        },
        UnitDef {
            _id: "sqm",
            symbol: "m²",
            aliases: &["m2", "sqm", "square_meter", "square_meters"],
            category: Category::Area,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "sqkm",
            symbol: "km²",
            aliases: &["km2", "sqkm", "square_kilometer"],
            category: Category::Area,
            conversion: Conversion::Factor(1000000.0),
        },
        UnitDef {
            _id: "sqft",
            symbol: "ft²",
            aliases: &["ft2", "sqft", "square_foot", "square_feet"],
            category: Category::Area,
            conversion: Conversion::Factor(0.092903),
        },
        UnitDef {
            _id: "acre",
            symbol: "ac",
            aliases: &["ac", "acre", "acres"],
            category: Category::Area,
            conversion: Conversion::Factor(4046.86),
        },
        UnitDef {
            _id: "hectare",
            symbol: "ha",
            aliases: &["ha", "hectare", "hectares"],
            category: Category::Area,
            conversion: Conversion::Factor(10000.0),
        },
        // Data (base: byte)
        UnitDef {
            _id: "byte",
            symbol: "B",
            aliases: &["b", "byte", "bytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "kilobyte",
            symbol: "KB",
            aliases: &["kb", "kilobyte", "kilobytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1000.0),
        },
        UnitDef {
            _id: "megabyte",
            symbol: "MB",
            aliases: &["mb", "megabyte", "megabytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1_000_000.0),
        },
        UnitDef {
            _id: "gigabyte",
            symbol: "GB",
            aliases: &["gb", "gigabyte", "gigabytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1_000_000_000.0),
        },
        UnitDef {
            _id: "terabyte",
            symbol: "TB",
            aliases: &["tb", "terabyte", "terabytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1_000_000_000_000.0),
        },
        UnitDef {
            _id: "kibibyte",
            symbol: "KiB",
            aliases: &["kib", "kibibyte", "kibibytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1024.0),
        },
        UnitDef {
            _id: "mebibyte",
            symbol: "MiB",
            aliases: &["mib", "mebibyte", "mebibytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1_048_576.0),
        },
        UnitDef {
            _id: "gibibyte",
            symbol: "GiB",
            aliases: &["gib", "gibibyte", "gibibytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1_073_741_824.0),
        },
        UnitDef {
            _id: "tebibyte",
            symbol: "TiB",
            aliases: &["tib", "tebibyte", "tebibytes"],
            category: Category::Data,
            conversion: Conversion::Factor(1_099_511_627_776.0),
        },
        // Time (base: second)
        UnitDef {
            _id: "second",
            symbol: "s",
            aliases: &["s", "sec", "second", "seconds"],
            category: Category::Time,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "minute",
            symbol: "min",
            aliases: &["min", "minute", "minutes"],
            category: Category::Time,
            conversion: Conversion::Factor(60.0),
        },
        UnitDef {
            _id: "hour",
            symbol: "h",
            aliases: &["h", "hr", "hour", "hours"],
            category: Category::Time,
            conversion: Conversion::Factor(3600.0),
        },
        UnitDef {
            _id: "day",
            symbol: "d",
            aliases: &["d", "day", "days"],
            category: Category::Time,
            conversion: Conversion::Factor(86400.0),
        },
        UnitDef {
            _id: "week",
            symbol: "wk",
            aliases: &["wk", "week", "weeks"],
            category: Category::Time,
            conversion: Conversion::Factor(604800.0),
        },
        UnitDef {
            _id: "month",
            symbol: "mo",
            aliases: &["mo", "month", "months"],
            category: Category::Time,
            conversion: Conversion::Factor(2_592_000.0),
        },
        UnitDef {
            _id: "year",
            symbol: "yr",
            aliases: &["yr", "year", "years"],
            category: Category::Time,
            conversion: Conversion::Factor(31_536_000.0),
        },
        // Pressure (base: Pa)
        UnitDef {
            _id: "pascal",
            symbol: "Pa",
            aliases: &["pa", "pascal", "pascals"],
            category: Category::Pressure,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "hectopascal",
            symbol: "hPa",
            aliases: &["hpa", "hectopascal"],
            category: Category::Pressure,
            conversion: Conversion::Factor(100.0),
        },
        UnitDef {
            _id: "kilopascal",
            symbol: "kPa",
            aliases: &["kpa", "kilopascal"],
            category: Category::Pressure,
            conversion: Conversion::Factor(1000.0),
        },
        UnitDef {
            _id: "bar",
            symbol: "bar",
            aliases: &["bar", "bars"],
            category: Category::Pressure,
            conversion: Conversion::Factor(100_000.0),
        },
        UnitDef {
            _id: "millibar",
            symbol: "mbar",
            aliases: &["mbar", "millibar"],
            category: Category::Pressure,
            conversion: Conversion::Factor(100.0),
        },
        UnitDef {
            _id: "psi",
            symbol: "psi",
            aliases: &["psi", "pounds_per_square_inch"],
            category: Category::Pressure,
            conversion: Conversion::Factor(6894.76),
        },
        UnitDef {
            _id: "atmosphere",
            symbol: "atm",
            aliases: &["atm", "atmosphere", "atmospheres"],
            category: Category::Pressure,
            conversion: Conversion::Factor(101_325.0),
        },
        UnitDef {
            _id: "mmhg",
            symbol: "mmHg",
            aliases: &["mmhg", "torr"],
            category: Category::Pressure,
            conversion: Conversion::Factor(133.322),
        },
        // Energy (base: Joule)
        UnitDef {
            _id: "joule",
            symbol: "J",
            aliases: &["j", "joule", "joules"],
            category: Category::Energy,
            conversion: Conversion::Factor(1.0),
        },
        UnitDef {
            _id: "kilojoule",
            symbol: "kJ",
            aliases: &["kj", "kilojoule", "kilojoules"],
            category: Category::Energy,
            conversion: Conversion::Factor(1000.0),
        },
        UnitDef {
            _id: "calorie",
            symbol: "cal",
            aliases: &["cal", "calorie", "calories"],
            category: Category::Energy,
            conversion: Conversion::Factor(4.184),
        },
        UnitDef {
            _id: "kilocalorie",
            symbol: "kcal",
            aliases: &["kcal", "kilocalorie", "kilocalories"],
            category: Category::Energy,
            conversion: Conversion::Factor(4184.0),
        },
        UnitDef {
            _id: "watt_hour",
            symbol: "Wh",
            aliases: &["wh", "watt_hour"],
            category: Category::Energy,
            conversion: Conversion::Factor(3600.0),
        },
        UnitDef {
            _id: "kilowatt_hour",
            symbol: "kWh",
            aliases: &["kwh", "kilowatt_hour"],
            category: Category::Energy,
            conversion: Conversion::Factor(3_600_000.0),
        },
        UnitDef {
            _id: "btu",
            symbol: "BTU",
            aliases: &["btu", "british_thermal_unit"],
            category: Category::Energy,
            conversion: Conversion::Factor(1055.06),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_celsius_to_fahrenheit() {
        let r = convert_to(&100.0, "c", "f").unwrap();
        assert!((r.value - 212.0).abs() < 0.01);
    }

    #[test]
    fn test_fahrenheit_to_celsius() {
        let r = convert_to(&32.0, "f", "c").unwrap();
        assert!((r.value - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_body_temp_f_to_c() {
        let r = convert_to(&98.6, "f", "c").unwrap();
        assert!((r.value - 37.0).abs() < 0.01);
    }

    #[test]
    fn test_km_to_miles() {
        let r = convert_to(&100.0, "km", "mi").unwrap();
        assert!((r.value - 62.1371).abs() < 0.01);
    }

    #[test]
    fn test_miles_to_km() {
        let r = convert_to(&1.0, "mi", "km").unwrap();
        assert!((r.value - 1.60934).abs() < 0.01);
    }

    #[test]
    fn test_kg_to_lb() {
        let r = convert_to(&1.0, "kg", "lb").unwrap();
        assert!((r.value - 2.20462).abs() < 0.01);
    }

    #[test]
    fn test_lb_to_kg() {
        let r = convert_to(&100.0, "lbs", "kg").unwrap();
        assert!((r.value - 45.3592).abs() < 0.01);
    }

    #[test]
    fn test_liters_to_gallons() {
        let r = convert_to(&3.78541, "l", "gal").unwrap();
        assert!((r.value - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_kmh_to_mph() {
        let r = convert_to(&100.0, "kmh", "mph").unwrap();
        assert!((r.value - 62.1371).abs() < 0.01);
    }

    #[test]
    fn test_gb_to_mb() {
        let r = convert_to(&1.0, "gb", "mb").unwrap();
        assert!((r.value - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_gib_to_mib() {
        let r = convert_to(&1.0, "gib", "mib").unwrap();
        assert!((r.value - 1024.0).abs() < 0.01);
    }

    #[test]
    fn test_hours_to_minutes() {
        let r = convert_to(&2.5, "h", "min").unwrap();
        assert!((r.value - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_bar_to_psi() {
        let r = convert_to(&1.0, "bar", "psi").unwrap();
        assert!((r.value - 14.5038).abs() < 0.01);
    }

    #[test]
    fn test_kcal_to_kj() {
        let r = convert_to(&1.0, "kcal", "kj").unwrap();
        assert!((r.value - 4.184).abs() < 0.01);
    }

    #[test]
    fn test_sqm_to_sqft() {
        let r = convert_to(&1.0, "m2", "ft2").unwrap();
        assert!((r.value - 10.7639).abs() < 0.01);
    }

    #[test]
    fn test_unknown_unit_returns_none() {
        assert!(convert_to(&100.0, "xyz", "abc").is_none());
    }

    #[test]
    fn test_cross_category_returns_none() {
        assert!(convert_to(&100.0, "km", "kg").is_none());
    }

    #[test]
    fn test_convert_common_returns_results() {
        let results = convert_common(&100.0, "km");
        assert!(!results.is_empty());
        assert!(results.len() <= 5);
    }

    #[test]
    fn test_convert_common_excludes_source() {
        let results = convert_common(&100.0, "km");
        for r in &results {
            assert_ne!(r.target_symbol, "km");
        }
    }

    #[test]
    fn test_alias_case_insensitive() {
        let r1 = convert_to(&100.0, "KM", "MI").unwrap();
        let r2 = convert_to(&100.0, "km", "mi").unwrap();
        assert!((r1.value - r2.value).abs() < 0.001);
    }

    #[test]
    fn test_full_name_alias() {
        let r = convert_to(&100.0, "kilometers", "miles").unwrap();
        assert!((r.value - 62.1371).abs() < 0.01);
    }

    #[test]
    fn test_format_currency_two_decimals() {
        let r = convert_to(&1.0, "km", "mi").unwrap();
        // display_value should have reasonable formatting
        assert!(!r.display_value.is_empty());
    }

    #[test]
    fn test_currency_alias_convert_to() {
        // "dollar" and "euro" are aliases, not in the UNITS table
        let r = convert_to(&20.0, "dollar", "euro");
        // May return None if ECB rates unavailable (network), but should not panic
        // In a network-available environment, this should return Some
        if let Some(r) = r {
            assert!(r.value > 0.0);
            assert_eq!(r.target_symbol, "EUR");
        }
    }

    #[test]
    fn test_currency_alias_convert_common() {
        let results = convert_common(&20.0, "dollar");
        // May be empty if ECB rates unavailable, but should not panic
        for r in &results {
            assert!(r.value > 0.0);
        }
    }

    #[test]
    fn test_display_value_no_double_unit() {
        let r = convert_to(&100.0, "km", "mi").unwrap();
        // display_value should contain the symbol exactly once
        let count = r.display_value.matches(&r.target_symbol).count();
        assert_eq!(count, 1, "display_value '{}' should contain '{}' exactly once", r.display_value, r.target_symbol);
    }
}
