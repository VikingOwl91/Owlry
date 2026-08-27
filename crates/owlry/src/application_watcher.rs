//! Application desktop-entry freshness.
//!
//! Watches the XDG application directories used by `ApplicationProvider` and
//! refreshes only that provider after filesystem event bursts settle.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use log::{info, warn};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::providers::ProviderManager;

const DEBOUNCE: Duration = Duration::from_millis(200);

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

        let mut watched = Vec::new();
        let mut application_dirs = Vec::new();
        let mut seen = HashSet::new();
        for dir in dirs {
            application_dirs.push(dir.clone());
            let watch_path = nearest_existing_parent(&dir)
                .ok_or_else(|| notify::Error::generic("no application directory parent exists"))?;
            let watch_path = std::fs::canonicalize(&watch_path).unwrap_or(watch_path);
            if seen.insert(watch_path.clone()) {
                watcher.watch(&watch_path, RecursiveMode::Recursive)?;
                watched.push(watch_path);
            }
        }

        if watched.is_empty() {
            return Err(notify::Error::generic(
                "no application directories exist to watch",
            ));
        }

        info!(
            "application refresh: watching {} directories",
            watched.len()
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
    run_watch_loop_with_refresh(rx, application_dirs, || match pm.write() {
        Ok(mut providers) => {
            providers.refresh_provider("app");
            info!("application refresh: desktop entry change detected");
        }
        Err(_) => warn!("application refresh: provider lock poisoned"),
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

        let deadline = Instant::now() + DEBOUNCE;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(remaining) {
                Ok(Ok(_)) | Ok(Err(_)) => continue,
                Err(_) => break,
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

fn nearest_existing_parent(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|parent| parent.exists())
        .map(PathBuf::from)
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
