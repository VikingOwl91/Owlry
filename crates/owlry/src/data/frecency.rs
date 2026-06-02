use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::paths;

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

const MAX_ENTRIES: usize = 5000;
const PRUNE_AGE_DAYS: i64 = 180;
const MIN_LAUNCHES_TO_KEEP: u32 = 3;

/// Thresholds controlling startup pruning of the frecency store. Defaults
/// match the historical hardcoded constants; the daemon overrides them from
/// `[providers]` config. A `0` for `max_entries` or `max_age_days` disables
/// that limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrecencyPruneConfig {
    pub max_entries: usize,
    pub max_age_days: i64,
    pub min_launches: u32,
}

impl Default for FrecencyPruneConfig {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_age_days: PRUNE_AGE_DAYS,
            min_launches: MIN_LAUNCHES_TO_KEEP,
        }
    }
}

/// Frecency store for tracking and boosting recently/frequently used items
pub struct FrecencyStore {
    data: FrecencyData,
    path: PathBuf,
    dirty: bool,
    prune_config: FrecencyPruneConfig,
}

impl FrecencyStore {
    /// Create a new frecency store with default prune thresholds.
    pub fn new() -> Self {
        Self::with_config(FrecencyPruneConfig::default())
    }

    /// Create a new frecency store, loading existing data and pruning with the
    /// given thresholds.
    pub fn with_config(prune_config: FrecencyPruneConfig) -> Self {
        let path = Self::data_path();
        let data = Self::load_from_path(&path).unwrap_or_default();

        info!("Frecency store loaded with {} entries", data.entries.len());

        let mut store = Self {
            data,
            path,
            dirty: false,
            prune_config,
        };
        store.prune();
        store
    }

    /// Remove stale low-usage entries and enforce the hard cap, using this
    /// store's configured thresholds and the current time.
    fn prune(&mut self) {
        let before = self.data.entries.len();
        Self::prune_entries(&mut self.data.entries, &self.prune_config, Utc::now());

        let removed = before - self.data.entries.len();
        if removed > 0 {
            info!(
                "Frecency: pruned {} stale entries ({} remaining)",
                removed,
                self.data.entries.len()
            );
            self.dirty = true;
        }
    }

    /// Pure pruning policy (no IO, injectable `now`):
    ///
    /// 1. Age: when `max_age_days > 0`, drop entries older than that many days
    ///    unless they have at least `min_launches` launches.
    /// 2. Cap: when `max_entries > 0` and still over the cap, keep the
    ///    highest-scoring `max_entries`.
    ///
    /// A `0` for either limit disables that stage.
    fn prune_entries(
        entries: &mut HashMap<String, FrecencyEntry>,
        cfg: &FrecencyPruneConfig,
        now: DateTime<Utc>,
    ) {
        // Guard against absurd values (e.g. a huge number meant as "never"):
        // chrono panics if the cutoff underflows DateTime's range, and
        // FrecencyStore::with_config -> prune runs on every startup path. An
        // out-of-range cutoff means "older than the dawn of time" → keep all.
        if cfg.max_age_days > 0
            && let Some(cutoff) =
                chrono::Duration::try_days(cfg.max_age_days).and_then(|d| now.checked_sub_signed(d))
        {
            entries.retain(|_, e| e.last_launch > cutoff || e.launch_count >= cfg.min_launches);
        }

        if cfg.max_entries > 0 && entries.len() > cfg.max_entries {
            let mut scored: Vec<(String, f64)> = entries
                .iter()
                .map(|(k, e)| {
                    (
                        k.clone(),
                        Self::calculate_frecency_at(e.launch_count, e.last_launch, now),
                    )
                })
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let keep: std::collections::HashSet<String> = scored
                .into_iter()
                .take(cfg.max_entries)
                .map(|(k, _)| k)
                .collect();
            entries.retain(|k, _| keep.contains(k));
        }
    }

    /// Alias for new() - loads from disk or creates default
    pub fn load_or_default() -> Self {
        Self::new()
    }

    /// Get the path to the frecency data file
    fn data_path() -> PathBuf {
        paths::frecency_file().unwrap_or_else(|| PathBuf::from("frecency.json"))
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

        paths::ensure_parent_dir(&self.path)?;

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
    }

    /// Calculate frecency score for an item
    /// Uses Firefox-style algorithm: score = launch_count * recency_weight
    pub fn get_score(&self, item_id: &str) -> f64 {
        match self.data.entries.get(item_id) {
            Some(entry) => Self::calculate_frecency(entry.launch_count, entry.last_launch),
            None => 0.0,
        }
    }

    /// Calculate frecency score using a pre-sampled timestamp.
    /// Use this in hot loops to avoid repeated Utc::now() syscalls.
    pub fn get_score_at(&self, item_id: &str, now: DateTime<Utc>) -> f64 {
        match self.data.entries.get(item_id) {
            Some(entry) => Self::calculate_frecency_at(entry.launch_count, entry.last_launch, now),
            None => 0.0,
        }
    }

    /// Calculate frecency using Firefox-style algorithm
    fn calculate_frecency(launch_count: u32, last_launch: DateTime<Utc>) -> f64 {
        let now = Utc::now();
        Self::calculate_frecency_at(launch_count, last_launch, now)
    }

    /// Calculate frecency using a caller-provided timestamp.
    fn calculate_frecency_at(
        launch_count: u32,
        last_launch: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> f64 {
        let age = now.signed_duration_since(last_launch);
        let age_days = age.num_hours() as f64 / 24.0;

        let recency_weight = if age_days < 1.0 {
            100.0
        } else if age_days < 7.0 {
            70.0
        } else if age_days < 30.0 {
            50.0
        } else if age_days < 90.0 {
            30.0
        } else {
            10.0
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
    fn get_score_at_matches_get_score() {
        let mut store = FrecencyStore {
            data: FrecencyData {
                version: 1,
                entries: HashMap::new(),
            },
            path: PathBuf::from("/dev/null"),
            dirty: false,
            prune_config: FrecencyPruneConfig::default(),
        };
        store.data.entries.insert(
            "test".to_string(),
            FrecencyEntry {
                launch_count: 5,
                last_launch: Utc::now(),
            },
        );

        let now = Utc::now();
        let score_at = store.get_score_at("test", now);
        let score = store.get_score("test");

        // Both should be very close (same timestamp, within rounding)
        assert!((score_at - score).abs() < 1.0);
    }

    #[test]
    fn test_launch_count_matters() {
        let now = Utc::now();

        let score_few = FrecencyStore::calculate_frecency(2, now);
        let score_many = FrecencyStore::calculate_frecency(20, now);

        assert!(score_many > score_few);
        assert!((score_many / score_few - 10.0).abs() < 0.1); // Should be ~10x
    }

    #[test]
    fn record_launch_sets_dirty_without_saving() {
        let mut store = FrecencyStore {
            data: FrecencyData::default(),
            path: PathBuf::from("/dev/null"),
            dirty: false,
            prune_config: FrecencyPruneConfig::default(),
        };

        store.record_launch("test-item");

        assert!(store.dirty, "record_launch should set dirty flag");
        assert_eq!(store.data.entries["test-item"].launch_count, 1);
    }

    fn entries_with(
        specs: &[(&str, u32, i64)],
        now: DateTime<Utc>,
    ) -> HashMap<String, FrecencyEntry> {
        specs
            .iter()
            .map(|(id, count, days_ago)| {
                (
                    id.to_string(),
                    FrecencyEntry {
                        launch_count: *count,
                        last_launch: now - chrono::Duration::days(*days_ago),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn prune_drops_old_low_usage_but_keeps_recent_and_frequent() {
        let now = Utc::now();
        let mut entries = entries_with(
            &[
                ("recent", 1, 10),  // recent, low usage — keep (within age window)
                ("old_rare", 1, 400), // old + rare — drop
                ("old_frequent", 5, 400), // old but >= min_launches — keep
            ],
            now,
        );
        let cfg = FrecencyPruneConfig {
            max_entries: 0,
            max_age_days: 180,
            min_launches: 3,
        };
        FrecencyStore::prune_entries(&mut entries, &cfg, now);

        assert!(entries.contains_key("recent"));
        assert!(entries.contains_key("old_frequent"));
        assert!(!entries.contains_key("old_rare"));
    }

    #[test]
    fn prune_age_zero_disables_age_pruning() {
        let now = Utc::now();
        let mut entries = entries_with(&[("ancient", 1, 9999)], now);
        let cfg = FrecencyPruneConfig {
            max_entries: 0,
            max_age_days: 0,
            min_launches: 3,
        };
        FrecencyStore::prune_entries(&mut entries, &cfg, now);
        assert!(entries.contains_key("ancient"), "age=0 must keep all entries");
    }

    #[test]
    fn prune_caps_to_max_entries_by_score() {
        let now = Utc::now();
        // All recent so age pruning is a no-op; cap must keep the two highest
        // launch_count entries.
        let mut entries = entries_with(
            &[("a", 1, 0), ("b", 5, 0), ("c", 9, 0)],
            now,
        );
        let cfg = FrecencyPruneConfig {
            max_entries: 2,
            max_age_days: 0,
            min_launches: 3,
        };
        FrecencyStore::prune_entries(&mut entries, &cfg, now);
        assert_eq!(entries.len(), 2);
        assert!(entries.contains_key("c") && entries.contains_key("b"));
        assert!(!entries.contains_key("a"));
    }

    #[test]
    fn prune_huge_max_age_days_does_not_panic() {
        // A user setting a giant value (meaning "never") must not crash startup:
        // an out-of-range cutoff is treated as "keep all".
        let now = Utc::now();
        let mut entries = entries_with(&[("a", 1, 0), ("b", 1, 9999)], now);
        let cfg = FrecencyPruneConfig {
            max_entries: 0,
            max_age_days: i64::MAX,
            min_launches: 3,
        };
        FrecencyStore::prune_entries(&mut entries, &cfg, now);
        assert_eq!(entries.len(), 2, "out-of-range age cutoff keeps everything");
    }

    #[test]
    fn prune_max_entries_zero_disables_cap() {
        let now = Utc::now();
        let mut entries = entries_with(&[("a", 1, 0), ("b", 1, 0), ("c", 1, 0)], now);
        let cfg = FrecencyPruneConfig {
            max_entries: 0,
            max_age_days: 0,
            min_launches: 3,
        };
        FrecencyStore::prune_entries(&mut entries, &cfg, now);
        assert_eq!(entries.len(), 3, "max_entries=0 must keep everything");
    }
}
