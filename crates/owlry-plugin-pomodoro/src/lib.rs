//! Pomodoro Timer Widget Plugin for Owlry
//!
//! Shows timer with work/break cycles. Select to open controls submenu.
//! State persists across sessions via JSON file.
//!
//! ## Configuration
//!
//! Configure via `~/.config/owlry/config.toml`:
//!
//! ```toml
//! [plugins.pomodoro]
//! work_mins = 25      # Work session duration (default: 25)
//! break_mins = 5      # Break duration (default: 5)
//! ```

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    notify_with_urgency, owlry_plugin, NotifyUrgency, PluginInfo, PluginItem, ProviderHandle,
    ProviderInfo, ProviderKind, ProviderPosition, API_VERSION,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Plugin metadata
const PLUGIN_ID: &str = "pomodoro";
const PLUGIN_NAME: &str = "Pomodoro Timer";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Pomodoro timer widget with work/break cycles";

// Provider metadata
const PROVIDER_ID: &str = "pomodoro";
const PROVIDER_NAME: &str = "Pomodoro";
const PROVIDER_ICON: &str = "alarm";
const PROVIDER_TYPE_ID: &str = "pomodoro";

// Default timing (in minutes)
const DEFAULT_WORK_MINS: u32 = 25;
const DEFAULT_BREAK_MINS: u32 = 5;

/// Pomodoro configuration
#[derive(Debug, Clone)]
struct PomodoroConfig {
    work_mins: u32,
    break_mins: u32,
}

impl PomodoroConfig {
    /// Load config from ~/.config/owlry/config.toml
    ///
    /// Reads from [plugins.pomodoro] section, with fallback to [providers] for compatibility.
    fn load() -> Self {
        let config_path = dirs::config_dir()
            .map(|d| d.join("owlry").join("config.toml"));

        let config_content = config_path
            .and_then(|p| fs::read_to_string(p).ok());

        if let Some(content) = config_content
            && let Ok(toml) = content.parse::<toml::Table>()
        {
            // Try [plugins.pomodoro] first (new format)
            if let Some(plugins) = toml.get("plugins").and_then(|v| v.as_table())
                && let Some(pomodoro) = plugins.get("pomodoro").and_then(|v| v.as_table())
            {
                return Self::from_toml_table(pomodoro);
            }

            // Fallback to [providers] section (old format)
            if let Some(providers) = toml.get("providers").and_then(|v| v.as_table()) {
                let work_mins = providers
                    .get("pomodoro_work_mins")
                    .and_then(|v| v.as_integer())
                    .map(|v| v as u32)
                    .unwrap_or(DEFAULT_WORK_MINS);

                let break_mins = providers
                    .get("pomodoro_break_mins")
                    .and_then(|v| v.as_integer())
                    .map(|v| v as u32)
                    .unwrap_or(DEFAULT_BREAK_MINS);

                return Self { work_mins, break_mins };
            }
        }

        // Default config
        Self {
            work_mins: DEFAULT_WORK_MINS,
            break_mins: DEFAULT_BREAK_MINS,
        }
    }

    /// Parse config from a TOML table
    fn from_toml_table(table: &toml::Table) -> Self {
        let work_mins = table
            .get("work_mins")
            .and_then(|v| v.as_integer())
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_WORK_MINS);

        let break_mins = table
            .get("break_mins")
            .and_then(|v| v.as_integer())
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_BREAK_MINS);

        Self { work_mins, break_mins }
    }
}

/// Timer phase
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
enum PomodoroPhase {
    #[default]
    Idle,
    Working,
    WorkPaused,
    Break,
    BreakPaused,
}

/// Persistent state (saved to disk)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PomodoroState {
    phase: PomodoroPhase,
    remaining_secs: u32,
    sessions: u32,
    last_update: u64,
}

/// Pomodoro provider state
struct PomodoroProviderState {
    items: Vec<PluginItem>,
    state: PomodoroState,
    work_mins: u32,
    break_mins: u32,
}

impl PomodoroProviderState {
    fn new() -> Self {
        let config = PomodoroConfig::load();

        let state = Self::load_state().unwrap_or_else(|| PomodoroState {
            phase: PomodoroPhase::Idle,
            remaining_secs: config.work_mins * 60,
            sessions: 0,
            last_update: Self::now_secs(),
        });

        let mut provider = Self {
            items: Vec::new(),
            state,
            work_mins: config.work_mins,
            break_mins: config.break_mins,
        };

        provider.update_elapsed_time();
        provider.generate_items();
        provider
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn data_dir() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("owlry"))
    }

    fn load_state() -> Option<PomodoroState> {
        let path = Self::data_dir()?.join("pomodoro.json");
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_state(&self) {
        if let Some(data_dir) = Self::data_dir() {
            let path = data_dir.join("pomodoro.json");
            if fs::create_dir_all(&data_dir).is_err() {
                return;
            }
            let mut state = self.state.clone();
            state.last_update = Self::now_secs();
            if let Ok(json) = serde_json::to_string_pretty(&state) {
                let _ = fs::write(&path, json);
            }
        }
    }

    fn update_elapsed_time(&mut self) {
        let now = Self::now_secs();
        let elapsed = now.saturating_sub(self.state.last_update);

        match self.state.phase {
            PomodoroPhase::Working | PomodoroPhase::Break => {
                if elapsed >= self.state.remaining_secs as u64 {
                    self.complete_phase();
                } else {
                    self.state.remaining_secs -= elapsed as u32;
                }
            }
            _ => {}
        }
        self.state.last_update = now;
    }

    fn complete_phase(&mut self) {
        match self.state.phase {
            PomodoroPhase::Working => {
                self.state.sessions += 1;
                self.state.phase = PomodoroPhase::Break;
                self.state.remaining_secs = self.break_mins * 60;
                notify_with_urgency(
                    "Pomodoro Complete!",
                    &format!(
                        "Great work! Session {} complete. Time for a {}-minute break.",
                        self.state.sessions, self.break_mins
                    ),
                    "alarm",
                    NotifyUrgency::Normal,
                );
            }
            PomodoroPhase::Break => {
                self.state.phase = PomodoroPhase::Idle;
                self.state.remaining_secs = self.work_mins * 60;
                notify_with_urgency(
                    "Break Complete",
                    "Break time's over! Ready for another work session?",
                    "alarm",
                    NotifyUrgency::Normal,
                );
            }
            _ => {}
        }
        self.save_state();
    }

    fn refresh(&mut self) {
        self.update_elapsed_time();
        self.generate_items();
    }

    fn handle_action(&mut self, action: &str) {
        match action {
            "start" => {
                self.state.phase = PomodoroPhase::Working;
                self.state.remaining_secs = self.work_mins * 60;
                self.state.last_update = Self::now_secs();
            }
            "pause" => match self.state.phase {
                PomodoroPhase::Working => self.state.phase = PomodoroPhase::WorkPaused,
                PomodoroPhase::Break => self.state.phase = PomodoroPhase::BreakPaused,
                _ => {}
            },
            "resume" => {
                self.state.last_update = Self::now_secs();
                match self.state.phase {
                    PomodoroPhase::WorkPaused => self.state.phase = PomodoroPhase::Working,
                    PomodoroPhase::BreakPaused => self.state.phase = PomodoroPhase::Break,
                    _ => {}
                }
            }
            "skip" => self.complete_phase(),
            "reset" => {
                self.state.phase = PomodoroPhase::Idle;
                self.state.remaining_secs = self.work_mins * 60;
                self.state.sessions = 0;
            }
            _ => {}
        }
        self.save_state();
        self.generate_items();
    }

    fn format_time(secs: u32) -> String {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    /// Generate single main item with submenu for controls
    fn generate_items(&mut self) {
        self.items.clear();

        let (phase_name, _is_running) = match self.state.phase {
            PomodoroPhase::Idle => ("Ready", false),
            PomodoroPhase::Working => ("Work", true),
            PomodoroPhase::WorkPaused => ("Paused", false),
            PomodoroPhase::Break => ("Break", true),
            PomodoroPhase::BreakPaused => ("Paused", false),
        };

        let time_str = Self::format_time(self.state.remaining_secs);
        let name = format!("{}: {}", phase_name, time_str);

        let description = if self.state.sessions > 0 {
            format!(
                "Sessions: {} | {}min work / {}min break",
                self.state.sessions, self.work_mins, self.break_mins
            )
        } else {
            format!("{}min work / {}min break", self.work_mins, self.break_mins)
        };

        // Single item that opens submenu with controls
        self.items.push(
            PluginItem::new("pomo-timer", name, "SUBMENU:pomodoro:controls")
                .with_description(description)
                .with_icon("/org/owlry/launcher/icons/pomodoro/tomato.svg")
                .with_keywords(vec![
                    "pomodoro".to_string(),
                    "widget".to_string(),
                    "timer".to_string(),
                ]),
        );
    }

    /// Generate submenu items for controls
    fn generate_submenu_items(&self) -> Vec<PluginItem> {
        let mut items = Vec::new();
        let is_running = matches!(
            self.state.phase,
            PomodoroPhase::Working | PomodoroPhase::Break
        );

        // Primary control: Start/Pause/Resume
        if is_running {
            items.push(
                PluginItem::new("pomo-pause", "Pause", "POMODORO:pause")
                    .with_description("Pause the timer")
                    .with_icon("media-playback-pause"),
            );
        } else {
            match self.state.phase {
                PomodoroPhase::Idle => {
                    items.push(
                        PluginItem::new("pomo-start", "Start Work", "POMODORO:start")
                            .with_description("Start a new work session")
                            .with_icon("media-playback-start"),
                    );
                }
                _ => {
                    items.push(
                        PluginItem::new("pomo-resume", "Resume", "POMODORO:resume")
                            .with_description("Resume the timer")
                            .with_icon("media-playback-start"),
                    );
                }
            }
        }

        // Skip (only when not idle)
        if self.state.phase != PomodoroPhase::Idle {
            items.push(
                PluginItem::new("pomo-skip", "Skip", "POMODORO:skip")
                    .with_description("Skip to next phase")
                    .with_icon("media-skip-forward"),
            );
        }

        // Reset
        items.push(
            PluginItem::new("pomo-reset", "Reset", "POMODORO:reset")
                .with_description("Reset timer and sessions")
                .with_icon("view-refresh"),
        );

        items
    }
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
        priority: 11500, // Widget: pomodoro timer
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(PomodoroProviderState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let state = unsafe { &mut *(handle.ptr as *mut PomodoroProviderState) };
    state.refresh();
    state.items.clone().into()
}

extern "C" fn provider_query(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let query_str = query.as_str();
    let state = unsafe { &mut *(handle.ptr as *mut PomodoroProviderState) };

    // Handle submenu request
    if query_str == "?SUBMENU:controls" {
        return state.generate_submenu_items().into();
    }

    // Handle action commands
    if let Some(action) = query_str.strip_prefix("!POMODORO:") {
        state.handle_action(action);
    }

    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        let state = unsafe { &*(handle.ptr as *const PomodoroProviderState) };
        state.save_state();
        unsafe {
            handle.drop_as::<PomodoroProviderState>();
        }
    }
}

owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(PomodoroProviderState::format_time(0), "00:00");
        assert_eq!(PomodoroProviderState::format_time(60), "01:00");
        assert_eq!(PomodoroProviderState::format_time(90), "01:30");
        assert_eq!(PomodoroProviderState::format_time(1500), "25:00");
        assert_eq!(PomodoroProviderState::format_time(3599), "59:59");
    }

    #[test]
    fn test_default_phase() {
        let phase: PomodoroPhase = Default::default();
        assert_eq!(phase, PomodoroPhase::Idle);
    }
}
