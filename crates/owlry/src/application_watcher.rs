//! Application desktop-entry freshness.
//!
//! Watches the XDG application directories used by `ApplicationProvider` and
//! refreshes only that provider after filesystem event bursts settle.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use log::{info, warn};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::providers::{ApplicationProvider, ProviderManager};

/// Quiet period after the last event before refreshing. Sliding: each new
/// event restarts it, so a package transaction that writes `.desktop` files
/// over several seconds still produces one rescan.
const DEBOUNCE: Duration = Duration::from_millis(500);

/// Upper bound on how long a continuous event stream can defer the refresh.
const MAX_DEBOUNCE: Duration = Duration::from_secs(5);

/// Keeps application-directory watches alive for the daemon lifetime.
pub struct ApplicationWatcher {
    _watcher: RecommendedWatcher,
    _thread: thread::JoinHandle<()>,
}

impl ApplicationWatcher {
    pub fn spawn(dirs: Vec<PathBuf>, pm: Arc<RwLock<ProviderManager>>) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        let mut application_dirs = Vec::new();
        let mut seen = HashSet::new();
        for dir in dirs {
            // Only ever watch a directory that exists. Walking up to the
            // nearest existing ancestor would put a *recursive* watch on
            // $HOME, /var/lib or /run for the nix/flatpak/snap entries that
            // `paths::system_data_dirs` lists unconditionally and that are
            // absent on most systems.
            if !dir.is_dir() {
                continue;
            }
            // Canonicalise before both watching and matching: notify reports
            // resolved paths, so storing the unresolved form here would make
            // every `starts_with` check in `event_touches_applications` fail
            // on systems where a component is a symlink.
            let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
            if !seen.insert(dir.clone()) {
                continue;
            }
            match watcher.watch(&dir, RecursiveMode::Recursive) {
                Ok(()) => application_dirs.push(dir),
                // Best effort: one unwatchable directory must not disable
                // watching for the others.
                Err(error) => warn!(
                    "application refresh: cannot watch {}: {error}",
                    dir.display()
                ),
            }
        }

        if application_dirs.is_empty() {
            return Err(notify::Error::generic(
                "no application directories could be watched",
            ));
        }

        info!(
            "application refresh: watching {} directories",
            application_dirs.len()
        );
        let handle = thread::spawn(move || run_watch_loop(rx, pm, application_dirs));
        Ok(Self {
            _watcher: watcher,
            _thread: handle,
        })
    }
}

fn run_watch_loop(
    rx: mpsc::Receiver<notify::Result<Event>>,
    pm: Arc<RwLock<ProviderManager>>,
    application_dirs: Vec<PathBuf>,
) {
    run_watch_loop_with_refresh(rx, application_dirs, || {
        // Walk the desktop entries before taking the lock — the scan is the
        // slow part and holding the write lock across it would block every
        // query for its duration.
        let items = ApplicationProvider::scan();
        match pm.write() {
            Ok(mut providers) => {
                if providers.install_items("app", items) {
                    info!("application refresh: desktop entry change applied");
                } else {
                    warn!("application refresh: application provider is not registered; skipping");
                }
            }
            Err(_) => warn!("application refresh: provider lock poisoned"),
        }
    });
}

fn run_watch_loop_with_refresh(
    rx: mpsc::Receiver<notify::Result<Event>>,
    application_dirs: Vec<PathBuf>,
    mut refresh: impl FnMut(),
) {
    loop {
        let first = match rx.recv() {
            Ok(result) => result,
            Err(_) => return,
        };

        match first {
            Ok(event) if event_touches_applications(&event, &application_dirs) => {}
            Ok(_) => continue,
            Err(error) => {
                warn!("application refresh: watcher backend error: {error}");
                continue;
            }
        }

        // Slide the quiet window forward on every further event, but never
        // past MAX_DEBOUNCE, so a continuous stream still gets serviced.
        let hard_deadline = Instant::now() + MAX_DEBOUNCE;
        loop {
            let deadline = (Instant::now() + DEBOUNCE).min(hard_deadline);
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(_) if Instant::now() < hard_deadline => continue,
                _ => break,
            }
        }

        refresh();
    }
}

fn event_touches_applications(event: &Event, application_dirs: &[PathBuf]) -> bool {
    event.paths.iter().any(|path| {
        application_dirs.iter().any(|dir| path.starts_with(dir))
            && (path
                .extension()
                .is_some_and(|extension| extension == "desktop")
                || application_dirs.iter().any(|dir| path == dir))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_events_are_selected() {
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![PathBuf::from("/tmp/applications/example.desktop")],
            attrs: Default::default(),
        };
        assert!(event_touches_applications(
            &event,
            &[PathBuf::from("/tmp/applications")]
        ));
    }

    #[test]
    fn unrelated_events_are_ignored() {
        let event = Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::Any),
            paths: vec![PathBuf::from("/tmp/applications/example.txt")],
            attrs: Default::default(),
        };
        assert!(!event_touches_applications(
            &event,
            &[PathBuf::from("/tmp/applications")]
        ));
    }

    #[test]
    fn application_directory_creation_is_selected() {
        let event = Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::Folder),
            paths: vec![PathBuf::from("/tmp/data/applications")],
            attrs: Default::default(),
        };
        assert!(event_touches_applications(
            &event,
            &[PathBuf::from("/tmp/data/applications")]
        ));
    }

    #[test]
    fn desktop_event_burst_triggers_one_refresh() {
        let (tx, rx) = mpsc::channel();
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![PathBuf::from("/tmp/applications/example.desktop")],
            attrs: Default::default(),
        };
        tx.send(Ok(event.clone())).unwrap();
        tx.send(Ok(event)).unwrap();
        drop(tx);

        let mut refreshes = 0;
        run_watch_loop_with_refresh(rx, vec![PathBuf::from("/tmp/applications")], || {
            refreshes += 1
        });

        assert_eq!(refreshes, 1);
    }
}
