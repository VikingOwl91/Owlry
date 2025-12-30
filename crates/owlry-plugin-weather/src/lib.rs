//! Weather Widget Plugin for Owlry
//!
//! Shows current weather with support for multiple APIs:
//! - wttr.in (default, no API key required)
//! - OpenWeatherMap (requires API key)
//! - Open-Meteo (no API key required)
//!
//! Weather data is cached for 15 minutes.
//!
//! ## Configuration
//!
//! Configure via `~/.config/owlry/config.toml`:
//!
//! ```toml
//! [plugins.weather]
//! provider = "wttr.in"  # or: openweathermap, open-meteo
//! location = "Berlin"   # city name or "lat,lon"
//! # api_key = "..."     # Required for OpenWeatherMap
//! ```

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind,
    ProviderPosition, API_VERSION,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Plugin metadata
const PLUGIN_ID: &str = "weather";
const PLUGIN_NAME: &str = "Weather";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Weather widget with multiple API support";

// Provider metadata
const PROVIDER_ID: &str = "weather";
const PROVIDER_NAME: &str = "Weather";
const PROVIDER_ICON: &str = "weather-clear";
const PROVIDER_TYPE_ID: &str = "weather";

// Timing constants
const CACHE_DURATION_SECS: u64 = 900; // 15 minutes
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = "owlry-launcher/0.3";

#[derive(Debug, Clone, PartialEq)]
enum WeatherProviderType {
    WttrIn,
    OpenWeatherMap,
    OpenMeteo,
}

impl std::str::FromStr for WeatherProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "wttr.in" | "wttr" | "wttrin" => Ok(Self::WttrIn),
            "openweathermap" | "owm" => Ok(Self::OpenWeatherMap),
            "open-meteo" | "openmeteo" | "meteo" => Ok(Self::OpenMeteo),
            _ => Err(format!("Unknown weather provider: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
struct WeatherConfig {
    provider: WeatherProviderType,
    api_key: Option<String>,
    location: String,
}

impl WeatherConfig {
    /// Load config from ~/.config/owlry/config.toml
    ///
    /// Reads from [plugins.weather] section, with fallback to [providers] for compatibility.
    fn load() -> Self {
        let config_path = dirs::config_dir()
            .map(|d| d.join("owlry").join("config.toml"));

        let config_content = config_path
            .and_then(|p| fs::read_to_string(p).ok());

        if let Some(content) = config_content
            && let Ok(toml) = content.parse::<toml::Table>()
        {
            // Try [plugins.weather] first (new format)
            if let Some(plugins) = toml.get("plugins").and_then(|v| v.as_table())
                && let Some(weather) = plugins.get("weather").and_then(|v| v.as_table())
            {
                return Self::from_toml_table(weather);
            }

            // Fallback to [providers] section (old format)
            if let Some(providers) = toml.get("providers").and_then(|v| v.as_table()) {
                let provider_str = providers
                    .get("weather_provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("wttr.in");

                let provider = provider_str.parse().unwrap_or(WeatherProviderType::WttrIn);

                let api_key = providers
                    .get("weather_api_key")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let location = providers
                    .get("weather_location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                return Self {
                    provider,
                    api_key,
                    location,
                };
            }
        }

        // Default config
        Self {
            provider: WeatherProviderType::WttrIn,
            api_key: None,
            location: String::new(),
        }
    }

    /// Parse config from a TOML table
    fn from_toml_table(table: &toml::Table) -> Self {
        let provider_str = table
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("wttr.in");

        let provider = provider_str.parse().unwrap_or(WeatherProviderType::WttrIn);

        let api_key = table
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(String::from);

        let location = table
            .get("location")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Self {
            provider,
            api_key,
            location,
        }
    }
}

/// Cached weather data (persisted to disk)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WeatherData {
    temperature: f32,
    feels_like: Option<f32>,
    condition: String,
    humidity: Option<u8>,
    wind_speed: Option<f32>,
    icon: String,
    location: String,
}

/// Persistent cache structure (saved to ~/.local/share/owlry/weather_cache.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WeatherCache {
    last_fetch_epoch: u64,
    data: WeatherData,
}

/// Weather provider state
struct WeatherState {
    items: Vec<PluginItem>,
    config: WeatherConfig,
    last_fetch_epoch: u64,
    cached_data: Option<WeatherData>,
}

impl WeatherState {
    fn new() -> Self {
        Self::with_config(WeatherConfig::load())
    }

    fn with_config(config: WeatherConfig) -> Self {
        // Load cached weather from disk if available
        // This prevents blocking HTTP requests on every app open
        let (last_fetch_epoch, cached_data) = Self::load_cache()
            .map(|c| (c.last_fetch_epoch, Some(c.data)))
            .unwrap_or((0, None));

        Self {
            items: Vec::new(),
            config,
            last_fetch_epoch,
            cached_data,
        }
    }

    fn data_dir() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("owlry"))
    }

    fn cache_path() -> Option<PathBuf> {
        Self::data_dir().map(|d| d.join("weather_cache.json"))
    }

    fn load_cache() -> Option<WeatherCache> {
        let path = Self::cache_path()?;
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_cache(&self) {
        if let (Some(data_dir), Some(cache_path), Some(data)) =
            (Self::data_dir(), Self::cache_path(), &self.cached_data)
        {
            if fs::create_dir_all(&data_dir).is_err() {
                return;
            }
            let cache = WeatherCache {
                last_fetch_epoch: self.last_fetch_epoch,
                data: data.clone(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&cache) {
                let _ = fs::write(&cache_path, json);
            }
        }
    }

    fn now_epoch() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn is_cache_valid(&self) -> bool {
        if self.last_fetch_epoch == 0 {
            return false;
        }
        let now = Self::now_epoch();
        now.saturating_sub(self.last_fetch_epoch) < CACHE_DURATION_SECS
    }

    fn refresh(&mut self) {
        // Use cache if still valid (works across app restarts)
        if self.is_cache_valid()
            && let Some(data) = self.cached_data.clone() {
                self.generate_items(&data);
                return;
            }

        // Fetch new data from API
        if let Some(data) = self.fetch_weather() {
            self.cached_data = Some(data.clone());
            self.last_fetch_epoch = Self::now_epoch();
            self.save_cache(); // Persist to disk for next app open
            self.generate_items(&data);
        } else {
            // On fetch failure, try to use stale cache if available
            if let Some(data) = self.cached_data.clone() {
                self.generate_items(&data);
            } else {
                self.items.clear();
            }
        }
    }

    fn fetch_weather(&self) -> Option<WeatherData> {
        match self.config.provider {
            WeatherProviderType::WttrIn => self.fetch_wttr_in(),
            WeatherProviderType::OpenWeatherMap => self.fetch_openweathermap(),
            WeatherProviderType::OpenMeteo => self.fetch_open_meteo(),
        }
    }

    fn fetch_wttr_in(&self) -> Option<WeatherData> {
        let location = if self.config.location.is_empty() {
            String::new()
        } else {
            self.config.location.clone()
        };

        let url = format!("https://wttr.in/{}?format=j1", location);

        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .ok()?;

        let response = client.get(&url).send().ok()?;
        let json: WttrInResponse = response.json().ok()?;

        let current = json.current_condition.first()?;
        let nearest = json.nearest_area.first()?;

        let location_name = nearest
            .area_name
            .first()
            .map(|a| a.value.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        Some(WeatherData {
            temperature: current.temp_c.parse().unwrap_or(0.0),
            feels_like: current.feels_like_c.parse().ok(),
            condition: current
                .weather_desc
                .first()
                .map(|d| d.value.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            humidity: current.humidity.parse().ok(),
            wind_speed: current.windspeed_kmph.parse().ok(),
            icon: Self::wttr_code_to_icon(&current.weather_code),
            location: location_name,
        })
    }

    fn fetch_openweathermap(&self) -> Option<WeatherData> {
        let api_key = self.config.api_key.as_ref()?;
        if self.config.location.is_empty() {
            return None; // OWM requires a location
        }

        let url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric",
            self.config.location, api_key
        );

        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .ok()?;

        let response = client.get(&url).send().ok()?;
        let json: OpenWeatherMapResponse = response.json().ok()?;

        let weather = json.weather.first()?;

        Some(WeatherData {
            temperature: json.main.temp,
            feels_like: Some(json.main.feels_like),
            condition: weather.description.clone(),
            humidity: Some(json.main.humidity),
            wind_speed: Some(json.wind.speed * 3.6), // m/s to km/h
            icon: Self::owm_icon_to_freedesktop(&weather.icon),
            location: json.name,
        })
    }

    fn fetch_open_meteo(&self) -> Option<WeatherData> {
        let (lat, lon, location_name) = self.get_coordinates()?;

        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&timezone=auto",
            lat, lon
        );

        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .ok()?;

        let response = client.get(&url).send().ok()?;
        let json: OpenMeteoResponse = response.json().ok()?;

        let current = json.current;

        Some(WeatherData {
            temperature: current.temperature_2m,
            feels_like: None,
            condition: Self::wmo_code_to_description(current.weather_code),
            humidity: Some(current.relative_humidity_2m as u8),
            wind_speed: Some(current.wind_speed_10m),
            icon: Self::wmo_code_to_icon(current.weather_code),
            location: location_name,
        })
    }

    fn get_coordinates(&self) -> Option<(f64, f64, String)> {
        let location = &self.config.location;

        // Check if location is already coordinates (lat,lon)
        if location.contains(',') {
            let parts: Vec<&str> = location.split(',').collect();
            if parts.len() == 2
                && let (Ok(lat), Ok(lon)) = (
                    parts[0].trim().parse::<f64>(),
                    parts[1].trim().parse::<f64>(),
                ) {
                    return Some((lat, lon, location.clone()));
                }
        }

        // Use Open-Meteo geocoding API
        let url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
            location
        );

        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .ok()?;

        let response = client.get(&url).send().ok()?;
        let json: GeocodingResponse = response.json().ok()?;

        let result = json.results?.into_iter().next()?;
        Some((result.latitude, result.longitude, result.name))
    }

    fn wttr_code_to_icon(code: &str) -> String {
        match code {
            "113" => "weather-clear",
            "116" => "weather-few-clouds",
            "119" => "weather-overcast",
            "122" => "weather-overcast",
            "143" | "248" | "260" => "weather-fog",
            "176" | "263" | "266" | "293" | "296" | "299" | "302" | "305" | "308" => {
                "weather-showers"
            }
            "179" | "182" | "185" | "227" | "230" | "323" | "326" | "329" | "332" | "335"
            | "338" | "350" | "368" | "371" | "374" | "377" => "weather-snow",
            "200" | "386" | "389" | "392" | "395" => "weather-storm",
            _ => "weather-clear",
        }
        .to_string()
    }

    fn owm_icon_to_freedesktop(icon: &str) -> String {
        match icon {
            "01d" | "01n" => "weather-clear",
            "02d" | "02n" => "weather-few-clouds",
            "03d" | "03n" | "04d" | "04n" => "weather-overcast",
            "09d" | "09n" | "10d" | "10n" => "weather-showers",
            "11d" | "11n" => "weather-storm",
            "13d" | "13n" => "weather-snow",
            "50d" | "50n" => "weather-fog",
            _ => "weather-clear",
        }
        .to_string()
    }

    fn wmo_code_to_description(code: i32) -> String {
        match code {
            0 => "Clear sky",
            1 => "Mainly clear",
            2 => "Partly cloudy",
            3 => "Overcast",
            45 | 48 => "Foggy",
            51 | 53 | 55 => "Drizzle",
            61 | 63 | 65 => "Rain",
            66 | 67 => "Freezing rain",
            71 | 73 | 75 | 77 => "Snow",
            80..=82 => "Rain showers",
            85 | 86 => "Snow showers",
            95 | 96 | 99 => "Thunderstorm",
            _ => "Unknown",
        }
        .to_string()
    }

    fn wmo_code_to_icon(code: i32) -> String {
        match code {
            0 | 1 => "weather-clear",
            2 => "weather-few-clouds",
            3 => "weather-overcast",
            45 | 48 => "weather-fog",
            51 | 53 | 55 | 61 | 63 | 65 | 80 | 81 | 82 => "weather-showers",
            66 | 67 | 71 | 73 | 75 | 77 | 85 | 86 => "weather-snow",
            95 | 96 | 99 => "weather-storm",
            _ => "weather-clear",
        }
        .to_string()
    }

    fn icon_to_resource_path(icon: &str) -> String {
        let weather_icon = if icon.contains("clear") {
            "wi-day-sunny"
        } else if icon.contains("few-clouds") {
            "wi-day-cloudy"
        } else if icon.contains("overcast") || icon.contains("clouds") {
            "wi-cloudy"
        } else if icon.contains("fog") {
            "wi-fog"
        } else if icon.contains("showers") || icon.contains("rain") {
            "wi-rain"
        } else if icon.contains("snow") {
            "wi-snow"
        } else if icon.contains("storm") {
            "wi-thunderstorm"
        } else {
            "wi-thermometer"
        };
        format!("/org/owlry/launcher/icons/weather/{}.svg", weather_icon)
    }

    fn generate_items(&mut self, data: &WeatherData) {
        self.items.clear();

        let temp_str = format!("{}°C", data.temperature.round() as i32);
        let name = format!("{} {}", temp_str, data.condition);

        let mut details = vec![data.location.clone()];
        if let Some(humidity) = data.humidity {
            details.push(format!("Humidity {}%", humidity));
        }
        if let Some(wind) = data.wind_speed {
            details.push(format!("Wind {} km/h", wind.round() as i32));
        }
        if let Some(feels) = data.feels_like
            && (feels - data.temperature).abs() > 2.0 {
                details.push(format!("Feels like {}°C", feels.round() as i32));
            }

        let encoded_location = data.location.replace(' ', "+");
        let command = format!("xdg-open 'https://wttr.in/{}'", encoded_location);

        self.items.push(
            PluginItem::new("weather-current", name, command)
                .with_description(details.join(" | "))
                .with_icon(Self::icon_to_resource_path(&data.icon))
                .with_keywords(vec!["weather".to_string(), "widget".to_string()]),
        );
    }
}

// ============================================================================
// API Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct WttrInResponse {
    current_condition: Vec<WttrInCurrent>,
    nearest_area: Vec<WttrInArea>,
}

#[derive(Debug, Deserialize)]
struct WttrInCurrent {
    #[serde(rename = "temp_C")]
    temp_c: String,
    #[serde(rename = "FeelsLikeC")]
    feels_like_c: String,
    humidity: String,
    #[serde(rename = "weatherCode")]
    weather_code: String,
    #[serde(rename = "weatherDesc")]
    weather_desc: Vec<WttrInValue>,
    #[serde(rename = "windspeedKmph")]
    windspeed_kmph: String,
}

#[derive(Debug, Deserialize)]
struct WttrInValue {
    value: String,
}

#[derive(Debug, Deserialize)]
struct WttrInArea {
    #[serde(rename = "areaName")]
    area_name: Vec<WttrInValue>,
}

#[derive(Debug, Deserialize)]
struct OpenWeatherMapResponse {
    main: OwmMain,
    weather: Vec<OwmWeather>,
    wind: OwmWind,
    name: String,
}

#[derive(Debug, Deserialize)]
struct OwmMain {
    temp: f32,
    feels_like: f32,
    humidity: u8,
}

#[derive(Debug, Deserialize)]
struct OwmWeather {
    description: String,
    icon: String,
}

#[derive(Debug, Deserialize)]
struct OwmWind {
    speed: f32,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f32,
    relative_humidity_2m: f32,
    weather_code: i32,
    wind_speed_10m: f32,
}

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    results: Option<Vec<GeocodingResult>>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
}

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
        prefix: ROption::RNone,
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Static,
        type_id: RString::from(PROVIDER_TYPE_ID),
        position: ProviderPosition::Widget,
        priority: 12000, // Widget: highest priority
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(WeatherState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<WeatherState>
    let state = unsafe { &mut *(handle.ptr as *mut WeatherState) };

    state.refresh();
    state.items.clone().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query not used, return empty
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<WeatherState>
        unsafe {
            handle.drop_as::<WeatherState>();
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weather_provider_type_from_str() {
        assert_eq!(
            "wttr.in".parse::<WeatherProviderType>().unwrap(),
            WeatherProviderType::WttrIn
        );
        assert_eq!(
            "owm".parse::<WeatherProviderType>().unwrap(),
            WeatherProviderType::OpenWeatherMap
        );
        assert_eq!(
            "open-meteo".parse::<WeatherProviderType>().unwrap(),
            WeatherProviderType::OpenMeteo
        );
    }

    #[test]
    fn test_wttr_code_to_icon() {
        assert_eq!(WeatherState::wttr_code_to_icon("113"), "weather-clear");
        assert_eq!(WeatherState::wttr_code_to_icon("116"), "weather-few-clouds");
        assert_eq!(WeatherState::wttr_code_to_icon("176"), "weather-showers");
        assert_eq!(WeatherState::wttr_code_to_icon("200"), "weather-storm");
    }

    #[test]
    fn test_wmo_code_to_description() {
        assert_eq!(WeatherState::wmo_code_to_description(0), "Clear sky");
        assert_eq!(WeatherState::wmo_code_to_description(3), "Overcast");
        assert_eq!(WeatherState::wmo_code_to_description(95), "Thunderstorm");
    }

    #[test]
    fn test_icon_to_resource_path() {
        assert_eq!(
            WeatherState::icon_to_resource_path("weather-clear"),
            "/org/owlry/launcher/icons/weather/wi-day-sunny.svg"
        );
    }

    #[test]
    fn test_cache_validity() {
        let state = WeatherState {
            items: Vec::new(),
            config: WeatherConfig {
                provider: WeatherProviderType::WttrIn,
                api_key: None,
                location: String::new(),
            },
            last_fetch_epoch: 0,
            cached_data: None,
        };
        assert!(!state.is_cache_valid());
    }
}
