//! Error type for Lua configuration loading and evaluation.
//!
//! Distinct from [`crate::config::ConfigError`] so that the daemon can report
//! "your Lua config failed" separately from "your TOML config failed" and
//! pinpoint the offending file path.

use std::path::PathBuf;
use thiserror::Error;

// Display strings here intentionally OMIT `{source}` — the wrapping is done
// elsewhere by walking `.source()`. Embedding it twice (once in Display,
// once via the chain) produced duplicated text in hot-reload error
// notifications (Phase 3.7).

#[derive(Debug, Error)]
pub enum LuaConfigError {
    #[error("failed to read Lua config at {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Lua evaluation error in {path}")]
    Eval {
        path: PathBuf,
        #[source]
        source: mlua::Error,
    },

    #[error("invalid value for `{key}` in {path}: {message}")]
    InvalidValue {
        path: PathBuf,
        key: String,
        message: String,
    },

    #[error("Lua runtime error: {0}")]
    Runtime(#[from] mlua::Error),
}
