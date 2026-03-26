//! Plugin system error types

use thiserror::Error;

/// Errors that can occur in the plugin system
#[derive(Error, Debug)]
#[allow(dead_code)] // Some variants are for future use
pub enum PluginError {
    #[error("Plugin '{0}' not found")]
    NotFound(String),

    #[error("Invalid plugin manifest in '{plugin}': {message}")]
    InvalidManifest { plugin: String, message: String },

    #[error("Plugin '{plugin}' requires owlry {required}, but current version is {current}")]
    VersionMismatch {
        plugin: String,
        required: String,
        current: String,
    },

    #[error("Lua error in plugin '{plugin}': {message}")]
    LuaError { plugin: String, message: String },

    #[error("Plugin '{plugin}' timed out after {timeout_ms}ms")]
    Timeout { plugin: String, timeout_ms: u64 },

    #[error("Plugin '{plugin}' attempted forbidden operation: {operation}")]
    SandboxViolation { plugin: String, operation: String },

    #[error("Plugin '{0}' is already loaded")]
    AlreadyLoaded(String),

    #[error("Plugin '{0}' is disabled")]
    Disabled(String),

    #[error("Failed to load native plugin: {0}")]
    LoadError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for plugin operations
pub type PluginResult<T> = Result<T, PluginError>;
