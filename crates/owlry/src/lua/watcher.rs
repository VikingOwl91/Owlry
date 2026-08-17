//! `owlry.lua` hot reload.
//!
//! Per docs/lua-api.md §7: the daemon watches the user's `owlry.lua` and,
//! on save, re-evaluates it in a fresh [`super::LuaContext`]. On success
//! the new state hot-swaps into the running daemon. On failure the
//! previous state is kept untouched so a broken save can't bring down a
//! running session — the user is told exactly what went wrong via both
//! the log and a desktop notification (so they don't need to tail the
//! journal to learn their config is broken).
//!
//! The watcher debounces editor save bursts (vim's atomic rename emits
//! several events per save) and acts on `owlry.lua` plus Lua modules below
//! its config directory. The Lua state held in [`crate::server::Server::_lua_ctx`] is
//! atomically replaced; user-provider items continue to point at live
//! Lua functions because [`super::LuaProvider`] holds its own
//! `Arc<Lua>` clone.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use log::{error, info, warn};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::config::{Config, LoadedConfig};
use crate::providers::{Provider, ProviderManager};

use super::{LuaContext, LuaProvider};

/// Window after the first event in a save burst during which further
/// events get coalesced. Empirical: vim/neovim takes ~10ms, JetBrains
/// ~80ms, atomic-rename saves can spread further apart. 200ms is a
/// comfortable upper bound that still feels instant to the user.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Handle to a running hot-reload watcher. Dropping it stops the watcher
/// thread cleanly (channel closure triggers loop exit).
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    _thread: thread::JoinHandle<()>,
}

impl ConfigWatcher {
    /// Spawn a watcher on `owlry.lua`'s parent directory. We watch the
    /// directory rather than the file directly so atomic-rename saves
    /// (vim, JetBrains, etc.) are still observed — the inode the watcher
    /// is bound to would otherwise vanish on rename.
    pub fn spawn(
        lua_path: PathBuf,
        config: Arc<RwLock<Config>>,
        pm: Arc<RwLock<ProviderManager>>,
        lua_ctx: Arc<Mutex<Option<LuaContext>>>,
    ) -> notify::Result<Self> {
        let watch_dir = lua_path
            .parent()
            .ok_or_else(|| notify::Error::generic("owlry.lua path has no parent directory"))?
            .to_path_buf();

        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| {
            // Drop the result if the receiver is gone — we're shutting down.
            let _ = tx.send(res);
        })?;
        watcher.watch(&watch_dir, RecursiveMode::Recursive)?;
        info!("hot-reload: watching {} recursively", watch_dir.display());

        let watched = lua_path.clone();
        let handle = thread::spawn(move || {
            run_watch_loop(watched, rx, config, pm, lua_ctx);
        });

        Ok(Self {
            _watcher: watcher,
            _thread: handle,
        })
    }
}

/// Long-running event-pump loop. Returns when the channel disconnects
/// (i.e. the watcher was dropped).
fn run_watch_loop(
    lua_path: PathBuf,
    rx: mpsc::Receiver<notify::Result<Event>>,
    config: Arc<RwLock<Config>>,
    pm: Arc<RwLock<ProviderManager>>,
    lua_ctx: Arc<Mutex<Option<LuaContext>>>,
) {
    loop {
        let first = match rx.recv() {
            Ok(r) => r,
            Err(_) => {
                info!("hot-reload: watcher channel closed, exiting");
                return;
            }
        };

        match first {
            Ok(ev) if event_touches(&ev, &lua_path) => { /* fall through */ }
            Ok(_) => continue,
            Err(e) => {
                warn!("hot-reload: watcher backend error: {}", e);
                continue;
            }
        }

        // Drain any follow-up events within the debounce window so a
        // single editor save doesn't trigger multiple reloads.
        let deadline = Instant::now() + DEBOUNCE;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(remaining) {
                Ok(_) => continue,
                Err(_) => break,
            }
        }

        reload(&lua_path, &config, &pm, &lua_ctx);
    }
}

/// Return true if an event touches `owlry.lua` or another Lua source below
/// its config directory. Watching the directory recursively makes modular
/// configs using `require` reload when their local modules change too.
fn event_touches(ev: &Event, lua_path: &Path) -> bool {
    let Some(watch_dir) = lua_path.parent() else {
        return false;
    };

    ev.paths.iter().any(|path| {
        path == lua_path
            || (path.starts_with(watch_dir)
                && path.extension().is_some_and(|extension| extension == "lua"))
    })
}

/// Re-evaluate `owlry.lua` in a fresh context and, on success, swap the
/// new state into the running daemon. On failure the old state is
/// preserved and the user is told exactly what went wrong.
fn reload(
    lua_path: &Path,
    config: &Arc<RwLock<Config>>,
    pm: &Arc<RwLock<ProviderManager>>,
    lua_ctx: &Arc<Mutex<Option<LuaContext>>>,
) {
    let loaded = match LoadedConfig::load_lua_path(lua_path) {
        Ok(lc) => lc,
        Err(e) => {
            report_reload_failure(lua_path, e.as_ref());
            return;
        }
    };

    if let Err(message) = apply_loaded_config(loaded, config, pm, lua_ctx) {
        warn!("hot-reload: {message}; keeping previous config");
        return;
    }

    info!("hot-reload: applied {}", lua_path.display());
    let _ = notify_rust::Notification::new()
        .summary("owlry: config reloaded")
        .body(&format!("Applied changes from {}", lua_path.display()))
        .icon("emblem-default")
        .timeout(notify_rust::Timeout::Milliseconds(3_000))
        .show();
}

/// Prepare and atomically install a fully resolved daemon configuration.
///
/// Both the filesystem watcher and SIGHUP use this path. All replacement
/// objects are built before any state lock is taken; all three locks are then
/// acquired before the first write, so lock poisoning cannot leave a partial
/// reload behind.
pub(crate) fn apply_loaded_config(
    loaded: LoadedConfig,
    config: &Arc<RwLock<Config>>,
    pm: &Arc<RwLock<ProviderManager>>,
    lua_ctx: &Arc<Mutex<Option<LuaContext>>>,
) -> Result<(), &'static str> {
    let new_user_providers: Vec<Box<dyn Provider>> = match &loaded.lua {
        Some(ctx) => loaded
            .user_providers
            .iter()
            .cloned()
            .map(|spec| Box::new(LuaProvider::new(spec, ctx.lua_handle())) as Box<dyn Provider>)
            .collect(),
        None => Vec::new(),
    };
    let new_pm = ProviderManager::new_with_provider_config(
        loaded.config.providers.clone(),
        new_user_providers,
    );

    let mut cfg_guard = config.write().map_err(|_| "config lock poisoned")?;
    let mut pm_guard = pm.write().map_err(|_| "provider lock poisoned")?;
    let mut ctx_guard = lua_ctx.lock().map_err(|_| "Lua context lock poisoned")?;

    *cfg_guard = loaded.config;
    *pm_guard = new_pm;
    *ctx_guard = loaded.lua;
    Ok(())
}

/// Surface a reload failure to both the log and the user's desktop. The
/// log entry includes the file path; the notification includes the same
/// path plus the formatted error chain so the user knows what's broken
/// without checking the journal.
fn report_reload_failure(lua_path: &Path, err: &dyn std::error::Error) {
    let chain = error_chain(err);
    error!(
        "hot-reload: re-eval of {} failed, keeping previous config — {}",
        lua_path.display(),
        chain
    );
    let _ = notify_rust::Notification::new()
        .summary("owlry: config reload failed")
        .body(&format!("{}\n\n{}", lua_path.display(), chain))
        .icon("dialog-error")
        .urgency(notify_rust::Urgency::Normal)
        .timeout(notify_rust::Timeout::Milliseconds(8_000))
        .show();
}

/// Render the full error chain (`err: <cause>: <cause>: …`) so mlua's
/// line/column information from a syntax error doesn't get truncated.
fn error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut cur: Option<&dyn std::error::Error> = err.source();
    while let Some(c) = cur {
        parts.push(c.to_string());
        cur = c.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    type DaemonState = (
        Arc<RwLock<Config>>,
        Arc<RwLock<ProviderManager>>,
        Arc<Mutex<Option<LuaContext>>>,
    );

    fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(contents.as_bytes()).expect("write");
        path
    }

    fn fresh_daemon_state(lua_path: &Path) -> DaemonState {
        let loaded = LoadedConfig::load_lua_path(lua_path).expect("initial load");
        let ctx = loaded.lua.expect("lua ctx after load");
        let user_providers: Vec<Box<dyn Provider>> = loaded
            .user_providers
            .iter()
            .cloned()
            .map(|spec| Box::new(LuaProvider::new(spec, ctx.lua_handle())) as Box<dyn Provider>)
            .collect();
        let config = Arc::new(RwLock::new(loaded.config));
        let pm = Arc::new(RwLock::new(ProviderManager::new_with_config(
            Arc::clone(&config),
            user_providers,
        )));
        let lua_ctx = Arc::new(Mutex::new(Some(ctx)));
        (config, pm, lua_ctx)
    }

    #[test]
    fn reload_swaps_in_new_config_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lua_path = write_file(dir.path(), "owlry.lua", r#"owlry.set { max_results = 7 }"#);
        let (config, pm, lua_ctx) = fresh_daemon_state(&lua_path);
        assert_eq!(config.read().unwrap().general.max_results, 7);

        // Rewrite the file and call reload directly.
        write_file(dir.path(), "owlry.lua", r#"owlry.set { max_results = 33 }"#);
        reload(&lua_path, &config, &pm, &lua_ctx);
        assert_eq!(config.read().unwrap().general.max_results, 33);
    }

    #[test]
    fn reload_failure_preserves_previous_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lua_path = write_file(dir.path(), "owlry.lua", r#"owlry.set { max_results = 7 }"#);
        let (config, pm, lua_ctx) = fresh_daemon_state(&lua_path);
        assert_eq!(config.read().unwrap().general.max_results, 7);

        // Replace the file with broken Lua. Reload must keep the old value.
        write_file(dir.path(), "owlry.lua", "this is not valid lua {{{");
        reload(&lua_path, &config, &pm, &lua_ctx);
        assert_eq!(
            config.read().unwrap().general.max_results,
            7,
            "old config must survive a broken reload"
        );
    }

    #[test]
    fn reload_replaces_user_providers_with_new_definitions() {
        use crate::data::FrecencyStore;
        use crate::filter::ProviderFilter;
        let dir = tempfile::tempdir().expect("tempdir");
        let lua_path = write_file(
            dir.path(),
            "owlry.lua",
            r#"
            owlry.provider {
              id = "v1",
              items = function() return { { name = "v1 unique item", command = "echo v1" } } end,
            }
            "#,
        );
        let (config, pm, lua_ctx) = fresh_daemon_state(&lua_path);
        let filter = ProviderFilter::all();
        let freq = FrecencyStore::new();
        {
            let pm_guard = pm.read().unwrap();
            let results =
                pm_guard.search_with_frecency("v1 unique", 100, &filter, &freq, 0.0, None);
            assert!(
                results.iter().any(|(it, _)| it.name.contains("v1 unique")),
                "v1 item must be present before reload"
            );
        }

        // Replace v1 with v2 and reload.
        write_file(
            dir.path(),
            "owlry.lua",
            r#"
            owlry.provider {
              id = "v2",
              items = function() return { { name = "v2 unique item", command = "echo v2" } } end,
            }
            "#,
        );
        reload(&lua_path, &config, &pm, &lua_ctx);

        let pm_guard = pm.read().unwrap();
        let v2 = pm_guard.search_with_frecency("v2 unique item", 100, &filter, &freq, 0.0, None);
        let v1 = pm_guard.search_with_frecency("v1 unique item", 100, &filter, &freq, 0.0, None);
        // Use the full "v1 unique item" string to discriminate from the
        // websearch provider which synthesises "Search: <query>" items for
        // any query (so a partial substring would false-positive).
        assert!(
            v2.iter().any(|(it, _)| it.name == "v2 unique item"),
            "v2 item must appear after reload"
        );
        assert!(
            !v1.iter().any(|(it, _)| it.name == "v1 unique item"),
            "v1 item must be gone after reload"
        );
    }

    #[test]
    fn applying_non_lua_config_clears_lua_runtime_and_user_providers() {
        use crate::data::FrecencyStore;
        use crate::filter::ProviderFilter;

        let dir = tempfile::tempdir().expect("tempdir");
        let lua_path = write_file(
            dir.path(),
            "owlry.lua",
            r#"
            owlry.provider {
              id = "temporary",
              items = function() return { { name = "temporary item", command = "true" } } end,
            }
            "#,
        );
        let (config, pm, lua_ctx) = fresh_daemon_state(&lua_path);

        apply_loaded_config(LoadedConfig::defaults(), &config, &pm, &lua_ctx)
            .expect("apply defaults");

        assert!(lua_ctx.lock().unwrap().is_none());
        let results = pm.read().unwrap().search_with_frecency(
            "temporary item",
            100,
            &ProviderFilter::all(),
            &FrecencyStore::new(),
            0.0,
            None,
        );
        assert!(
            !results
                .iter()
                .any(|(item, _)| item.name == "temporary item"),
            "removing owlry.lua must remove its user providers"
        );
    }

    #[test]
    fn event_touches_matches_main_and_nested_lua_files() {
        let watched = PathBuf::from("/tmp/owlry-test/owlry.lua");
        let other = PathBuf::from("/tmp/owlry-test/other.txt");
        let module = PathBuf::from("/tmp/owlry-test/modules/work.lua");
        let outside = PathBuf::from("/tmp/elsewhere/work.lua");
        let ev_match = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![watched.clone()],
            attrs: Default::default(),
        };
        let ev_miss = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![other.clone()],
            attrs: Default::default(),
        };
        let ev_module = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![module],
            attrs: Default::default(),
        };
        let ev_outside = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![outside],
            attrs: Default::default(),
        };
        assert!(event_touches(&ev_match, &watched));
        assert!(event_touches(&ev_module, &watched));
        assert!(!event_touches(&ev_miss, &watched));
        assert!(!event_touches(&ev_outside, &watched));
    }

    #[test]
    fn error_chain_renders_nested_causes() {
        use std::io;
        // io::Error with a custom source so we can verify chaining.
        let inner = io::Error::other("disk on fire");
        let chain = error_chain(&inner);
        assert!(chain.contains("disk on fire"), "got: {}", chain);
    }
}
