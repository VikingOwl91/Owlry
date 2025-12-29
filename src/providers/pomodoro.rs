//! Pomodoro timer widget provider
//!
//! Shows timer with work/break cycles and playback-style controls.
//! State persists across sessions via JSON file.

use super::{LaunchItem, Provider, ProviderType};
use crate::paths;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pomodoro timer provider with persistent state
pub struct PomodoroProvider {
    items: Vec<LaunchItem>,
    state: PomodoroState,
    config: PomodoroConfig,
}

#[derive(Debug, Clone)]
pub struct PomodoroConfig {
    pub work_mins: u32,
    pub break_mins: u32,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            work_mins: 25,
            break_mins: 5,
        }
    }
}

/// Timer phase
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PomodoroPhase {
    Idle,
    Working,
    WorkPaused,
    Break,
    BreakPaused,
}

impl Default for PomodoroPhase {
    fn default() -> Self {
        Self::Idle
    }
}

/// Persistent state (saved to disk)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PomodoroState {
    phase: PomodoroPhase,
    /// Remaining seconds in current phase
    remaining_secs: u32,
    /// Completed work sessions count
    sessions: u32,
    /// Unix timestamp when timer was last updated (for calculating elapsed time)
    last_update: u64,
}

impl PomodoroProvider {
    pub fn new(config: PomodoroConfig) -> Self {
        let state = Self::load_state().unwrap_or_else(|| PomodoroState {
            phase: PomodoroPhase::Idle,
            remaining_secs: config.work_mins * 60,
            sessions: 0,
            last_update: Self::now_secs(),
        });

        let mut provider = Self {
            items: Vec::new(),
            state,
            config,
        };

        // Update timer based on elapsed time since last save
        provider.update_elapsed_time();
        provider.generate_items();
        provider
    }

    /// Current unix timestamp
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Load state from disk
    fn load_state() -> Option<PomodoroState> {
        let path = paths::owlry_data_dir()?.join("pomodoro.json");
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save state to disk
    fn save_state(&self) {
        if let Some(data_dir) = paths::owlry_data_dir() {
            let path = data_dir.join("pomodoro.json");
            if let Err(e) = fs::create_dir_all(&data_dir) {
                warn!("Failed to create data dir: {}", e);
                return;
            }
            let mut state = self.state.clone();
            state.last_update = Self::now_secs();
            if let Ok(json) = serde_json::to_string_pretty(&state) {
                if let Err(e) = fs::write(&path, json) {
                    warn!("Failed to save pomodoro state: {}", e);
                }
            }
        }
    }

    /// Update remaining time based on elapsed time since last update
    fn update_elapsed_time(&mut self) {
        let now = Self::now_secs();
        let elapsed = now.saturating_sub(self.state.last_update);

        match self.state.phase {
            PomodoroPhase::Working | PomodoroPhase::Break => {
                if elapsed >= self.state.remaining_secs as u64 {
                    // Timer completed while app was closed
                    self.complete_phase();
                } else {
                    self.state.remaining_secs -= elapsed as u32;
                }
            }
            // Paused or idle - no time passes
            _ => {}
        }
        self.state.last_update = now;
    }

    /// Complete current phase and transition to next
    fn complete_phase(&mut self) {
        match self.state.phase {
            PomodoroPhase::Working => {
                self.state.sessions += 1;
                self.state.phase = PomodoroPhase::Break;
                self.state.remaining_secs = self.config.break_mins * 60;
                debug!("Work session {} completed, starting break", self.state.sessions);
            }
            PomodoroPhase::Break => {
                self.state.phase = PomodoroPhase::Idle;
                self.state.remaining_secs = self.config.work_mins * 60;
                debug!("Break completed, ready for next session");
            }
            _ => {}
        }
        self.save_state();
    }

    /// Handle action commands (called from MainWindow)
    pub fn handle_action(&mut self, action: &str) {
        debug!("Pomodoro action: {}", action);
        match action {
            "start" | "resume" => self.start_or_resume(),
            "pause" => self.pause(),
            "reset" => self.reset(),
            "skip" => self.skip_phase(),
            _ => warn!("Unknown pomodoro action: {}", action),
        }
        self.save_state();
        self.generate_items();
    }

    fn start_or_resume(&mut self) {
        match self.state.phase {
            PomodoroPhase::Idle => {
                self.state.phase = PomodoroPhase::Working;
                self.state.remaining_secs = self.config.work_mins * 60;
            }
            PomodoroPhase::WorkPaused => {
                self.state.phase = PomodoroPhase::Working;
            }
            PomodoroPhase::BreakPaused => {
                self.state.phase = PomodoroPhase::Break;
            }
            _ => {} // Already running
        }
        self.state.last_update = Self::now_secs();
    }

    fn pause(&mut self) {
        match self.state.phase {
            PomodoroPhase::Working => {
                self.state.phase = PomodoroPhase::WorkPaused;
            }
            PomodoroPhase::Break => {
                self.state.phase = PomodoroPhase::BreakPaused;
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.state.phase = PomodoroPhase::Idle;
        self.state.remaining_secs = self.config.work_mins * 60;
        self.state.sessions = 0;
    }

    fn skip_phase(&mut self) {
        self.complete_phase();
    }

    /// Format seconds as MM:SS
    fn format_time(secs: u32) -> String {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    /// Generate LaunchItems from current state
    fn generate_items(&mut self) {
        self.items.clear();

        let (phase_name, phase_icon, is_running) = match self.state.phase {
            PomodoroPhase::Idle => ("Ready", "media-playback-start", false),
            PomodoroPhase::Working => ("Work", "media-playback-start", true),
            PomodoroPhase::WorkPaused => ("Work (Paused)", "media-playback-pause", false),
            PomodoroPhase::Break => ("Break", "face-cool", true),
            PomodoroPhase::BreakPaused => ("Break (Paused)", "media-playback-pause", false),
        };

        // Timer display row
        let time_str = Self::format_time(self.state.remaining_secs);
        let name = format!("{}: {}", phase_name, time_str);
        let description = if self.state.sessions > 0 {
            Some(format!("Sessions: {} | {}min work / {}min break",
                self.state.sessions, self.config.work_mins, self.config.break_mins))
        } else {
            Some(format!("{}min work / {}min break",
                self.config.work_mins, self.config.break_mins))
        };

        self.items.push(LaunchItem {
            id: "pomo-timer".to_string(),
            name,
            description,
            icon: Some(phase_icon.to_string()),
            provider: ProviderType::Pomodoro,
            command: String::new(), // Info only
            terminal: false,
            tags: vec!["pomodoro".to_string(), "widget".to_string(), "timer".to_string()],
        });

        // Primary control: Start/Pause
        let (control_name, control_icon, control_action) = if is_running {
            ("Pause", "media-playback-pause", "pause")
        } else {
            match self.state.phase {
                PomodoroPhase::Idle => ("Start Work", "media-playback-start", "start"),
                _ => ("Resume", "media-playback-start", "resume"),
            }
        };

        self.items.push(LaunchItem {
            id: "pomo-control".to_string(),
            name: control_name.to_string(),
            description: Some(format!("{} timer", control_name)),
            icon: Some(control_icon.to_string()),
            provider: ProviderType::Pomodoro,
            command: format!("POMODORO:{}", control_action),
            terminal: false,
            tags: vec!["pomodoro".to_string(), "control".to_string()],
        });

        // Secondary controls: Reset and Skip
        if self.state.phase != PomodoroPhase::Idle {
            self.items.push(LaunchItem {
                id: "pomo-skip".to_string(),
                name: "Skip".to_string(),
                description: Some("Skip to next phase".to_string()),
                icon: Some("media-skip-forward".to_string()),
                provider: ProviderType::Pomodoro,
                command: "POMODORO:skip".to_string(),
                terminal: false,
                tags: vec!["pomodoro".to_string(), "control".to_string()],
            });
        }

        self.items.push(LaunchItem {
            id: "pomo-reset".to_string(),
            name: "Reset".to_string(),
            description: Some("Reset timer and sessions".to_string()),
            icon: Some("view-refresh".to_string()),
            provider: ProviderType::Pomodoro,
            command: "POMODORO:reset".to_string(),
            terminal: false,
            tags: vec!["pomodoro".to_string(), "control".to_string()],
        });
    }

    /// Check if timer has completed (for external polling)
    #[allow(dead_code)]
    pub fn check_completion(&mut self) -> bool {
        if matches!(self.state.phase, PomodoroPhase::Working | PomodoroPhase::Break) {
            self.update_elapsed_time();
            self.generate_items();
            self.save_state();
            true
        } else {
            false
        }
    }
}

impl Provider for PomodoroProvider {
    fn name(&self) -> &str {
        "Pomodoro"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Pomodoro
    }

    fn refresh(&mut self) {
        self.update_elapsed_time();
        self.generate_items();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

impl Default for PomodoroProvider {
    fn default() -> Self {
        Self::new(PomodoroConfig::default())
    }
}
