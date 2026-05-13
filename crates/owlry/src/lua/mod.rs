//! Lua configuration layer (Phase 3).
//!
//! The user configuration surface lives at `~/.config/owlry/owlry.lua` (D23).
//! In 2.1 this is an opt-in preview behind the `lua` cargo feature; the
//! existing TOML loader in [`crate::config`] remains the source of truth
//! when no `owlry.lua` is present. See `docs/lua-api.md` for the full
//! design (D1–D24) and roadmap to 3.0.
//!
//! Phase 3.1 only scaffolds the module — every submodule is a stub with no
//! call sites yet. Wiring happens in 3.2+.

pub mod api;
pub mod config;
pub mod error;
pub mod provider;
pub mod runtime;
pub mod util;
pub mod watcher;

pub use config::{LuaConfig, LuaProviderSpec};
pub use error::LuaConfigError;
pub use provider::LuaProvider;
pub use runtime::LuaContext;
pub use watcher::ConfigWatcher;
