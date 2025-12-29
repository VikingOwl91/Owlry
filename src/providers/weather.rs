//! Weather widget provider with multiple API support
//!
//! Supports:
//! - wttr.in (default, no API key required)
//! - OpenWeatherMap (requires API key)
//! - Open-Meteo (no API key required)

use super::{LaunchItem, Provider, ProviderType};
use log::{debug, error, warn};
use serde::Deserialize;
use std::time::{Duration, Instant};

const CACHE_DURATION: Duration = Duration::from_secs(900); // 15 minutes
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = "owlry-launcher/0.3";

/// Weather provider with caching and multiple API support
pub struct WeatherProvider {
    items: Vec<LaunchItem>,
    config: WeatherConfig,
    last_fetch: Option<Instant>,
    cached_data: Option<WeatherData>,
}

#[derive(Debug, Clone)]
pub struct WeatherConfig {
    pub provider: WeatherProviderType,
    pub api_key: Option<String>,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WeatherProviderType {
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
struct WeatherData {
    temperature: f32,
    feels_like: Option<f32>,
    condition: String,
    humidity: Option<u8>,
    wind_speed: Option<f32>,
    icon: String,
    location: String,
}

impl WeatherProvider {
    pub fn new(config: WeatherConfig) -> Self {
        let mut provider = Self {
            items: Vec::new(),
            config,
            last_fetch: None,
            cached_data: None,
        };
        provider.refresh();
        provider
    }

    /// Create with default config (wttr.in, auto-detect location)
    pub fn with_defaults() -> Self {
        Self::new(WeatherConfig {
            provider: WeatherProviderType::WttrIn,
            api_key: None,
            location: String::new(), // Empty = auto-detect
        })
    }

    /// Check if cache is still valid
    fn is_cache_valid(&self) -> bool {
        if let Some(last_fetch) = self.last_fetch {
            last_fetch.elapsed() < CACHE_DURATION
        } else {
            false
        }
    }

    /// Fetch weather data from the configured provider
    fn fetch_weather(&self) -> Option<WeatherData> {
        match self.config.provider {
            WeatherProviderType::WttrIn => self.fetch_wttr_in(),
            WeatherProviderType::OpenWeatherMap => self.fetch_openweathermap(),
            WeatherProviderType::OpenMeteo => self.fetch_open_meteo(),
        }
    }

    /// Fetch from wttr.in
    fn fetch_wttr_in(&self) -> Option<WeatherData> {
        let location = if self.config.location.is_empty() {
            String::new()
        } else {
            self.config.location.clone()
        };

        let url = format!("https://wttr.in/{}?format=j1", location);
        debug!("Fetching weather from: {}", url);

        let client = match reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to build HTTP client: {}", e);
                return None;
            }
        };

        let response = match client.get(&url).send() {
            Ok(r) => r,
            Err(e) => {
                error!("Weather request failed: {}", e);
                return None;
            }
        };

        let json: WttrInResponse = match response.json() {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to parse weather JSON: {}", e);
                return None;
            }
        };

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
            icon: Self::condition_to_icon(&current.weather_code),
            location: location_name,
        })
    }

    /// Fetch from OpenWeatherMap
    fn fetch_openweathermap(&self) -> Option<WeatherData> {
        let api_key = self.config.api_key.as_ref()?;
        let location = if self.config.location.is_empty() {
            warn!("OpenWeatherMap requires a location to be configured");
            return None;
        } else {
            &self.config.location
        };

        let url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric",
            location, api_key
        );
        debug!("Fetching weather from OpenWeatherMap");

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

    /// Fetch from Open-Meteo
    fn fetch_open_meteo(&self) -> Option<WeatherData> {
        // Open-Meteo requires coordinates, so we need to geocode first
        // For simplicity, we'll use a geocoding step if location is a city name
        let (lat, lon, location_name) = self.get_coordinates()?;

        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&timezone=auto",
            lat, lon
        );
        debug!("Fetching weather from Open-Meteo for {}", location_name);

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

    /// Get coordinates and location name for Open-Meteo (simple parsing or geocoding)
    fn get_coordinates(&self) -> Option<(f64, f64, String)> {
        let location = &self.config.location;

        // Check if location is already coordinates (lat,lon)
        if location.contains(',') {
            let parts: Vec<&str> = location.split(',').collect();
            if parts.len() == 2 {
                if let (Ok(lat), Ok(lon)) = (
                    parts[0].trim().parse::<f64>(),
                    parts[1].trim().parse::<f64>(),
                ) {
                    // Use coordinates as location name (will be overwritten if we had a name)
                    return Some((lat, lon, location.clone()));
                }
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

    /// Convert wttr.in weather code to freedesktop icon name
    fn condition_to_icon(code: &str) -> String {
        let icon = match code {
            "113" => "weather-clear",        // Sunny
            "116" => "weather-few-clouds",   // Partly cloudy
            "119" => "weather-overcast",     // Cloudy
            "122" => "weather-overcast",     // Overcast
            "143" | "248" | "260" => "weather-fog",
            "176" | "263" | "266" | "293" | "296" | "299" | "302" | "305" | "308" => {
                "weather-showers"
            }
            "179" | "182" | "185" | "227" | "230" | "323" | "326" | "329" | "332" | "335"
            | "338" | "350" | "368" | "371" | "374" | "377" => "weather-snow",
            "200" | "386" | "389" | "392" | "395" => "weather-storm",
            _ => "weather-clear",
        };
        // Try symbolic version first (more likely to exist)
        format!("{}-symbolic", icon)
    }

    /// Convert OpenWeatherMap icon code to freedesktop icon
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

    /// Convert WMO weather code to description
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
            80 | 81 | 82 => "Rain showers",
            85 | 86 => "Snow showers",
            95 | 96 | 99 => "Thunderstorm",
            _ => "Unknown",
        }
        .to_string()
    }

    /// Convert WMO weather code to icon name (used for emoji conversion)
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

    /// Generate LaunchItems from weather data
    fn generate_items(&mut self, data: &WeatherData) {
        self.items.clear();

        // Use emoji in name since icon themes are unreliable
        let emoji = Self::weather_icon_to_emoji(&data.icon);
        let temp_str = format!("{}°C", data.temperature.round() as i32);
        let name = format!("{} {} {}", emoji, temp_str, data.condition);

        let mut details = vec![data.location.clone()];
        if let Some(humidity) = data.humidity {
            details.push(format!("Humidity {}%", humidity));
        }
        if let Some(wind) = data.wind_speed {
            details.push(format!("Wind {} km/h", wind.round() as i32));
        }
        if let Some(feels) = data.feels_like {
            if (feels - data.temperature).abs() > 2.0 {
                details.push(format!("Feels like {}°C", feels.round() as i32));
            }
        }

        // Simple URL encoding for location
        let encoded_location = data.location.replace(' ', "+");

        self.items.push(LaunchItem {
            id: "weather-current".to_string(),
            name,
            description: Some(details.join(" | ")),
            icon: None, // Use emoji in name instead
            provider: ProviderType::Weather,
            command: format!("xdg-open 'https://wttr.in/{}'", encoded_location),
            terminal: false,
            tags: vec!["weather".to_string(), "widget".to_string()],
        });
    }

    /// Convert icon name to emoji for display
    fn weather_icon_to_emoji(icon: &str) -> &'static str {
        if icon.contains("clear") {
            "☀️"
        } else if icon.contains("few-clouds") {
            "⛅"
        } else if icon.contains("overcast") || icon.contains("clouds") {
            "☁️"
        } else if icon.contains("fog") {
            "🌫️"
        } else if icon.contains("showers") || icon.contains("rain") {
            "🌧️"
        } else if icon.contains("snow") {
            "❄️"
        } else if icon.contains("storm") {
            "⛈️"
        } else {
            "🌡️"
        }
    }
}

impl Provider for WeatherProvider {
    fn name(&self) -> &str {
        "Weather"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Weather
    }

    fn refresh(&mut self) {
        // Use cache if still valid
        if self.is_cache_valid() {
            if let Some(data) = self.cached_data.clone() {
                self.generate_items(&data);
                return;
            }
        }

        // Fetch new data
        match self.fetch_weather() {
            Some(data) => {
                debug!("Weather fetched: {}°C, {}", data.temperature, data.condition);
                self.cached_data = Some(data.clone());
                self.last_fetch = Some(Instant::now());
                self.generate_items(&data);
            }
            None => {
                warn!("Failed to fetch weather data");
                self.items.clear();
            }
        }
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

impl Default for WeatherProvider {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ─── API Response Types ─────────────────────────────────────────────────────

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
