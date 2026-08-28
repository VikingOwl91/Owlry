//! Application desktop-entry freshness.
//!
//! Watches the XDG application directories used by `ApplicationProvider` and
//! refreshes only that provider after filesystem event bursts settle.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, mpsc};
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

/// The set of directories the watcher is tracking, plus the handle needed to
/// start watching a bootstrap target once it appears.
struct WatchPlan {
    watcher: Arc<Mutex<RecommendedWatcher>>,
    /// Directories currently watched recursively; events under these matter.
    dirs: Vec<PathBuf>,
    /// Effective XDG application dirs that did not exist at startup. Their
    /// parent is watched non-recursively so their creation is observable.
    pending: Vec<PathBuf>,
}

impl WatchPlan {
    /// Does this event concern a watched application directory, or the
    /// appearance of a bootstrap target?
    fn is_relevant(&self, event: &Event) -> bool {
        event_touches_applications(event, &self.dirs)
            || event
                .paths
                .iter()
                .any(|path| self.pending.iter().any(|target| path.starts_with(target)))
    }

    /// Promote any bootstrap target that now exists to a real recursive watch.
    fn promote_ready(&mut self) {
        let mut still_pending = Vec::with_capacity(self.pending.len());
        for target in std::mem::take(&mut self.pending) {
            if !target.is_dir() {
                still_pending.push(target);
                continue;
            }
            let resolved = std::fs::canonicalize(&target).unwrap_or_else(|_| target.clone());
            let watched = match self.watcher.lock() {
                Ok(mut watcher) => watcher.watch(&resolved, RecursiveMode::Recursive),
                Err(_) => {
                    warn!("application refresh: watcher lock poisoned");
                    still_pending.push(target);
                    continue;
                }
            };
            match watched {
                Ok(()) => {
                    info!(
                        "application refresh: {} appeared; now watching it",
                        resolved.display()
                    );
                    self.dirs.push(resolved);
                }
                Err(error) => {
                    warn!(
                        "application refresh: cannot watch {}: {error}",
                        resolved.display()
                    );
                    still_pending.push(target);
                }
            }
        }
        self.pending = still_pending;
    }
}

/// Keeps application-directory watches alive for the daemon lifetime.
pub struct ApplicationWatcher {
    _watcher: Arc<Mutex<RecommendedWatcher>>,
    _thread: thread::JoinHandle<()>,
}

impl ApplicationWatcher {
    /// `dirs` is every directory the application provider scans. `bootstrap`
    /// is the subset whose creation should be watched for when absent — the
    /// effective XDG locations only, per `paths::xdg_application_dirs`.
    pub fn spawn(
        dirs: Vec<PathBuf>,
        bootstrap: Vec<PathBuf>,
        pm: Arc<RwLock<ProviderManager>>,
    ) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;
        let watcher = Arc::new(Mutex::new(watcher));

        let bootstrap: HashSet<PathBuf> = bootstrap.into_iter().collect();
        let mut plan = WatchPlan {
            watcher: Arc::clone(&watcher),
            dirs: Vec::new(),
            pending: Vec::new(),
        };
        let mut seen = HashSet::new();
        let mut seen_parents = HashSet::new();

        for dir in dirs {
            if dir.is_dir() {
                // Canonicalise before both watching and matching: notify
                // reports resolved paths, so storing the unresolved form would
                // make every `starts_with` check below fail on systems where a
                // component is a symlink.
                let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
                if !seen.insert(dir.clone()) {
                    continue;
                }
                match plan.watcher.lock() {
                    Ok(mut watcher) => match watcher.watch(&dir, RecursiveMode::Recursive) {
                        Ok(()) => plan.dirs.push(dir),
                        // Best effort: one unwatchable directory must not
                        // disable watching for the others.
                        Err(error) => warn!(
                            "application refresh: cannot watch {}: {error}",
                            dir.display()
                        ),
                    },
                    Err(_) => return Err(notify::Error::generic("watcher lock poisoned")),
                }
                continue;
            }

            // Absent. Only effective XDG locations get a bootstrap watch, and
            // only on their *immediate* parent — never further up. The
            // Flatpak/Snap/Nix fallbacks are skipped here on purpose: their
            // parents are absent too, so bootstrapping them would mean
            // recursively watching /var/lib, $HOME or /run.
            if !bootstrap.contains(&dir) {
                continue;
            }
            let Some(parent) = dir.parent() else {
                continue;
            };
            if !parent.is_dir() {
                continue;
            }
            let Some(name) = dir.file_name() else {
                continue;
            };
            let parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
            let target = parent.join(name);
            if seen_parents.insert(parent.clone()) {
                match plan.watcher.lock() {
                    // NonRecursive: this level only, enough to see the
                    // directory being created and nothing else.
                    Ok(mut watcher) => {
                        if let Err(error) = watcher.watch(&parent, RecursiveMode::NonRecursive) {
                            warn!(
                                "application refresh: cannot watch {} for {} appearing: {error}",
                                parent.display(),
                                target.display()
                            );
                            continue;
                        }
                    }
                    Err(_) => return Err(notify::Error::generic("watcher lock poisoned")),
                }
            }
            plan.pending.push(target);
        }

        if plan.dirs.is_empty() && plan.pending.is_empty() {
            return Err(notify::Error::generic(
                "no application directories could be watched",
            ));
        }

        info!(
            "application refresh: watching {} directories ({} awaiting creation)",
            plan.dirs.len(),
            plan.pending.len()
        );
        let handle = thread::spawn(move || run_watch_loop(rx, pm, plan));
        Ok(Self {
            _watcher: watcher,
            _thread: handle,
        })
    }
}

fn run_watch_loop(
    rx: mpsc::Receiver<notify::Result<Event>>,
    pm: Arc<RwLock<ProviderManager>>,
    plan: WatchPlan,
) {
    run_watch_loop_with_refresh(rx, plan, || {
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
    mut plan: WatchPlan,
    mut refresh: impl FnMut(),
) {
    loop {
        let first = match rx.recv() {
            Ok(result) => result,
            Err(_) => return,
        };

        match first {
            Ok(event) if plan.is_relevant(&event) => {}
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

        // A bootstrap target may have just been created — start watching it
        // before the rescan, so entries written into it afterwards are seen.
        plan.promote_ready();
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

    /// End-to-end: a real `.desktop` write in a really-watched directory has to
    /// reach the refresh callback. The predicate tests above only prove which
    /// events *would* be selected, not that any event is delivered at all.
    #[test]
    fn writing_a_desktop_file_in_a_watched_dir_reaches_the_refresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let apps = dir.path().join("applications");
        std::fs::create_dir(&apps).expect("create applications dir");

        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .expect("watcher");
        let apps = std::fs::canonicalize(&apps).unwrap_or(apps);
        watcher
            .watch(&apps, RecursiveMode::Recursive)
            .expect("watch applications dir");

        let (done_tx, done_rx) = mpsc::channel();
        let plan = WatchPlan {
            watcher: Arc::new(Mutex::new(watcher)),
            dirs: vec![apps.clone()],
            pending: Vec::new(),
        };
        let handle = thread::spawn(move || {
            run_watch_loop_with_refresh(rx, plan, || {
                let _ = done_tx.send(());
            });
        });

        std::fs::write(
            apps.join("example.desktop"),
            "[Desktop Entry]\nType=Application\nName=Example\nExec=/bin/true\n",
        )
        .expect("write desktop entry");

        done_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("refresh must fire for a .desktop write in a watched directory");

        drop(handle);
    }

    /// The reported case: `$XDG_DATA_HOME/applications` does not exist when the
    /// daemon starts. Only the parent is watched, non-recursively; creating the
    /// directory has to promote it to a real watch so the first `.desktop`
    /// written into it is seen without a manual refresh or a restart.
    #[test]
    fn target_created_after_start_is_promoted_and_delivers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_home = std::fs::canonicalize(dir.path()).expect("canonicalize");
        let apps = data_home.join("applications");
        assert!(!apps.exists(), "target must be absent at start");

        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .expect("watcher");
        let watcher = Arc::new(Mutex::new(watcher));
        watcher
            .lock()
            .unwrap()
            .watch(&data_home, RecursiveMode::NonRecursive)
            .expect("bootstrap watch on the parent");

        let plan = WatchPlan {
            watcher: Arc::clone(&watcher),
            dirs: Vec::new(),
            pending: vec![apps.clone()],
        };

        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            run_watch_loop_with_refresh(rx, plan, || {
                let _ = done_tx.send(());
            });
        });

        std::fs::create_dir(&apps).expect("create the application directory");
        done_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("creating the target must wake the watcher and promote it");

        // Promoted: an entry written inside it now reaches the refresh too.
        std::fs::write(
            apps.join("example.desktop"),
            "[Desktop Entry]\nType=Application\nName=Example\nExec=/bin/true\n",
        )
        .expect("write desktop entry");
        done_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("a .desktop written into the promoted directory must refresh");

        drop(handle);
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
        let plan = WatchPlan {
            watcher: Arc::new(Mutex::new(
                notify::recommended_watcher(|_| {}).expect("watcher"),
            )),
            dirs: vec![PathBuf::from("/tmp/applications")],
            pending: Vec::new(),
        };
        run_watch_loop_with_refresh(rx, plan, || refreshes += 1);

        assert_eq!(refreshes, 1);
    }
}
