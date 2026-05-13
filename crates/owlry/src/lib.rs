//! Owlry — Wayland application launcher.
//!
//! Single-crate layout (v2): the daemon, IPC layer, providers, and GTK4 UI
//! all live here. The binary entry point in `main.rs` selects between UI
//! launch (default) and daemon mode (`-d` / `--daemon`).

pub mod app;
pub mod backend;
pub mod cli;
pub mod client;
pub mod commands;
pub mod config;
pub mod data;
pub mod filter;
pub mod ipc;
#[cfg(feature = "lua")]
pub mod lua;
pub mod notify;
pub mod paths;
pub mod providers;
pub mod server;
pub mod theme;
pub mod ui;
