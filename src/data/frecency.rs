use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A single frecency entry tracking launch count and recency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrecencyEntry {
    pub launch_count: u32,
    pub last_launch: DateTime<Utc>,
}

/// Persistent frecency data store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrecencyData {
    pub version: u32,
    pub entries: HashMap<String, FrecencyEntry>,
}

impl Default for FrecencyData {
    fn default() -> Self {
        Self {
            version: 1,
            entries: HashMap::new(),
        }
    }
}

/// Frecency store for tracking and boosting recently/frequently used items
pub struct FrecencyStore {
    data: FrecencyData,
    path: PathBuf,
    dirty: bool,
}

impl FrecencyStore {
    /// Create a new frecency store, loading existing data if available
    pub fn new() -> Self {
        let path = Self::data_path();
        let data = Self::load_from_path(&path).unwrap_or_default();

        info!("Frecency store loaded with {} entries", data.entries.len());

        Self {
            data,
            path,
            dirty: false,
        }
    }

    /// Alias for new() - loads from disk or creates default
    pub fn load_or_default() -> Self {
        Self::new()
    }

    /// Get the path to the frecency data file
    fn data_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("owlry")
            .join("frecency.json")
    }

    /// Load frecency data from a file
    fn load_from_path(path: &PathBuf) -> Option<FrecencyData> {
        if !path.exists() {
            debug!("Frecency file not found at {:?}", path);
            return None;
        }

        let content = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str(&content) {
            Ok(data) => Some(data),
            Err(e) => {
                warn!("Failed to parse frecency data: {}", e);
                None
            }
        }
    }

    /// Save frecency data to disk
    pub fn save(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.dirty {
            return Ok(());
        }

        // Ensure directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.path, content)?;
        self.dirty = false;

        debug!("Frecency data saved to {:?}", self.path);
        Ok(())
    }

    /// Record a launch event for an item
    pub fn record_launch(&mut self, item_id: &str) {
        let now = Utc::now();

        let entry = self
            .data
            .entries
            .entry(item_id.to_string())
            .or_insert(FrecencyEntry {
                launch_count: 0,
                last_launch: now,
            });

        entry.launch_count += 1;
        entry.last_launch = now;
        self.dirty = true;

        debug!(
            "Recorded launch for '{}': count={}, last={}",
            item_id, entry.launch_count, entry.last_launch
        );

        // Auto-save after recording
        if let Err(e) = self.save() {
            warn!("Failed to save frecency data: {}", e);
        }
    }

    /// Calculate frecency score for an item
    /// Uses Firefox-style algorithm: score = launch_count * recency_weight
    pub fn get_score(&self, item_id: &str) -> f64 {
        match self.data.entries.get(item_id) {
            Some(entry) => Self::calculate_frecency(entry.launch_count, entry.last_launch),
            None => 0.0,
        }
    }

    /// Calculate frecency using Firefox-style algorithm
    fn calculate_frecency(launch_count: u32, last_launch: DateTime<Utc>) -> f64 {
        let now = Utc::now();
        let age = now.signed_duration_since(last_launch);
        let age_days = age.num_hours() as f64 / 24.0;

        // Recency weight based on how recently the item was used
        let recency_weight = if age_days < 1.0 {
            100.0 // Today
        } else if age_days < 7.0 {
            70.0 // This week
        } else if age_days < 30.0 {
            50.0 // This month
        } else if age_days < 90.0 {
            30.0 // This quarter
        } else {
            10.0 // Older
        };

        launch_count as f64 * recency_weight
    }

    /// Get all entries (for debugging/display)
    #[allow(dead_code)]
    pub fn entries(&self) -> &HashMap<String, FrecencyEntry> {
        &self.data.entries
    }

    /// Clear all frecency data
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.data.entries.clear();
        self.dirty = true;
    }
}

impl Default for FrecencyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FrecencyStore {
    fn drop(&mut self) {
        // Attempt to save on drop
        if let Err(e) = self.save() {
            warn!("Failed to save frecency data on drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frecency_calculation() {
        let now = Utc::now();

        // Recent launch should have high score
        let score_today = FrecencyStore::calculate_frecency(10, now);
        assert!(score_today > 900.0); // 10 * 100

        // Older launch should have lower score
        let week_ago = now - chrono::Duration::days(5);
        let score_week = FrecencyStore::calculate_frecency(10, week_ago);
        assert!(score_week < score_today);
        assert!(score_week > 600.0); // 10 * 70

        // Much older launch
        let month_ago = now - chrono::Duration::days(45);
        let score_month = FrecencyStore::calculate_frecency(10, month_ago);
        assert!(score_month < score_week);
    }

    #[test]
    fn test_launch_count_matters() {
        let now = Utc::now();

        let score_few = FrecencyStore::calculate_frecency(2, now);
        let score_many = FrecencyStore::calculate_frecency(20, now);

        assert!(score_many > score_few);
        assert!((score_many / score_few - 10.0).abs() < 0.1); // Should be ~10x
    }
}
