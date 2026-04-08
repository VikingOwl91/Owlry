//! Filesystem watcher for user plugin hot-reload
//!
//! Watches `~/.config/owlry/plugins/` for changes and triggers
//! runtime reload when plugin files are modified.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use log::{info, warn};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::providers::ProviderManager;

/// Start watching the user plugins directory for changes.
///
/// Spawns a background thread that monitors the directory and triggers
/// a full runtime reload on any file change. Returns immediately.
///
/// Respects `OWLRY_SKIP_RUNTIMES=1` — returns early if set.
pub fn start_watching(pm: Arc<RwLock<ProviderManager>>) {
    if std::env::var("OWLRY_SKIP_RUNTIMES").is_ok() {
        info!("OWLRY_SKIP_RUNTIMES set, skipping file watcher");
        return;
    }

    let plugins_dir = match crate::paths::plugins_dir() {
        Some(d) => d,
        None => {
            info!("No plugins directory configured, skipping file watcher");
            return;
        }
    };

    if !plugins_dir.exists()
        && std::fs::create_dir_all(&plugins_dir).is_err()
    {
        warn!(
            "Failed to create plugins directory: {}",
            plugins_dir.display()
        );
        return;
    }

    info!(
        "Plugin file watcher started for {}",
        plugins_dir.display()
    );

    thread::spawn(move || {
        if let Err(e) = watch_loop(&plugins_dir, &pm) {
            warn!("Plugin watcher stopped: {}", e);
        }
    });
}

fn watch_loop(
    plugins_dir: &PathBuf,
    pm: &Arc<RwLock<ProviderManager>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;

    debouncer
        .watcher()
        .watch(plugins_dir.as_ref(), notify::RecursiveMode::Recursive)?;

    info!("Watching {} for plugin changes", plugins_dir.display());

    // Skip events during initial startup grace period (watcher setup triggers events)
    let startup = std::time::Instant::now();
    let grace_period = Duration::from_secs(2);

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                if startup.elapsed() < grace_period {
                    continue;
                }

                let has_relevant_change = events.iter().any(|e| {
                    matches!(
                        e.kind,
                        DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous
                    )
                });

                if has_relevant_change {
                    info!("Plugin file change detected, reloading runtimes...");
                    match pm.write() {
                        Ok(mut pm_guard) => pm_guard.reload_runtimes(),
                        Err(_) => {
                            log::error!("Plugin watcher: provider lock poisoned; stopping watcher");
                            return Err(Box::from("provider lock poisoned"));
                        }
                    }
                }
            }
            Ok(Err(error)) => {
                warn!("File watcher error: {}", error);
            }
            Err(e) => {
                return Err(Box::new(e));
            }
        }
    }
}
