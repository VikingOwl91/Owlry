use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
#[cfg(feature = "lua")]
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// Maximum allowed size for a single IPC request line (1 MiB).
const MAX_REQUEST_SIZE: usize = 1_048_576;

/// Maximum number of concurrently active client connections.
const MAX_CONNECTIONS: usize = 16;

/// Tracks active connection count across all handler threads.
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

/// RAII guard that increments the connection counter on creation and decrements on drop.
struct ConnectionGuard;

impl ConnectionGuard {
    /// Try to acquire a connection slot. Returns `None` if at capacity.
    fn try_acquire() -> Option<Self> {
        let prev = ACTIVE_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
        if prev >= MAX_CONNECTIONS {
            ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(ConnectionGuard)
        }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Read a newline-terminated line from `reader` without allocating beyond `max` bytes.
///
/// Unlike `BufRead::read_line`, this checks the size limit incrementally against
/// the internal buffer rather than after the full allocation. Returns `Ok(None)`
/// on clean EOF, `Err(InvalidData)` when `max` is exceeded before finding `\n`.
fn read_bounded_line(reader: &mut BufReader<UnixStream>, max: usize) -> io::Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(None)
            } else {
                Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
            };
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            if buf.len() + pos > max {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("request too large (exceeded {} bytes)", max),
                ));
            }
            buf.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
        }
        let len = available.len();
        if buf.len() + len > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("request too large (exceeded {} bytes)", max),
            ));
        }
        buf.extend_from_slice(available);
        reader.consume(len);
    }
}

use log::{error, info, warn};

use crate::config::Config;
#[cfg(feature = "lua")]
use crate::config::LoadedConfig;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::ipc::{ProviderDesc, Request, Response, ResultItem};
use crate::providers::{LaunchItem, Provider, ProviderManager};

/// IPC server that listens on a Unix domain socket and dispatches
/// requests to the provider system.
pub struct Server {
    listener: UnixListener,
    socket_path: PathBuf,
    socket_identity: crate::paths::SocketIdentity,
    /// True when this process created the socket and therefore owns cleanup.
    /// A socket inherited from systemd must be left in place.
    socket_owned: bool,
    /// Held for the daemon's lifetime so a second daemon cannot race the
    /// stale-socket check and bind the same path.
    _socket_lock: File,
    provider_manager: Arc<RwLock<ProviderManager>>,
    frecency: Arc<RwLock<FrecencyStore>>,
    config: Arc<RwLock<Config>>,
    /// Keeps the Lua runtime alive for the daemon's lifetime so user
    /// providers (their `items` closures) remain callable. Wrapped so the
    /// hot-reload watcher (Phase 3.7) can atomically replace the context
    /// when `owlry.lua` is saved.
    #[cfg(feature = "lua")]
    _lua_ctx: Arc<Mutex<Option<crate::lua::LuaContext>>>,
    /// Filesystem watcher driving hot reload. Dropping this stops the
    /// watcher thread; kept alive for the daemon's lifetime.
    #[cfg(feature = "lua")]
    _lua_watcher: Option<crate::lua::ConfigWatcher>,
    /// Filesystem watcher that refreshes application entries when watched XDG
    /// directories receive `.desktop` file changes.
    _application_watcher: Option<crate::application_watcher::ApplicationWatcher>,
}

fn validate_socket_path(socket_path: &Path) -> io::Result<()> {
    if !socket_path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("socket path '{}' must be absolute", socket_path.display()),
        ));
    }
    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "socket path '{}' has no parent directory",
                socket_path.display()
            ),
        )
    })?;
    crate::paths::validate_socket_parent(parent, false)
}

/// Acquire the per-socket daemon lock. The returned file must stay alive for
/// the daemon's lifetime.
fn acquire_socket_lock(socket_path: &Path) -> io::Result<File> {
    use fs2::FileExt;

    let file_name = socket_path
        .file_name()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("socket path '{}' has no file name", socket_path.display()),
            )
        })?
        .to_string_lossy();
    let lock_path = socket_path.with_file_name(format!("{file_name}.lock"));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&lock_path)
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("failed to open socket lock '{}': {e}", lock_path.display()),
            )
        })?;
    let metadata = lock.metadata()?;
    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } || metadata.nlink() != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("unsafe socket lock file '{}'", lock_path.display()),
        ));
    }
    lock.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    lock.try_lock_exclusive().map_err(|e| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "another owlry daemon owns '{}' (lock '{}': {e})",
                socket_path.display(),
                lock_path.display()
            ),
        )
    })?;
    Ok(lock)
}

/// If systemd passed a listening AF_UNIX socket, take ownership of fd 3.
/// The service unit deliberately supports socket activation, and the socket
/// path already exists in that case.
fn inherited_listener() -> Option<UnixListener> {
    let pid = std::env::var("LISTEN_PID").ok()?.parse::<u32>().ok()?;
    if pid != std::process::id() {
        return None;
    }
    let fds = std::env::var("LISTEN_FDS").ok()?.parse::<u32>().ok()?;
    if fds == 0 {
        return None;
    }

    let fd: RawFd = 3;
    if !fd_is_unix_socket(fd) {
        return None;
    }
    set_cloexec(fd);

    // The packaged unit passes one listener. Close any extra descriptors so
    // custom multi-socket units cannot leak inherited listeners into helpers.
    for extra_fd in 4..=(2 + fds.min(1024) as RawFd) {
        // SAFETY: systemd passed these descriptors to this process.
        unsafe {
            libc::close(extra_fd);
        }
    }
    // Children must not inherit a stale activation environment. The listening
    // fd itself is CLOEXEC, so this closes the remaining footgun.
    // SAFETY: this runs during single-threaded daemon startup, before the
    // accept loop or any helper threads exist.
    unsafe {
        std::env::remove_var("LISTEN_FDS");
        std::env::remove_var("LISTEN_PID");
        std::env::remove_var("LISTEN_FDNAMES");
    }
    // SAFETY: systemd passed this file descriptor to this process and the
    // listener takes ownership of it exactly once.
    Some(unsafe { UnixListener::from_raw_fd(fd) })
}

fn set_cloexec(fd: RawFd) {
    // SAFETY: `fd` is an open descriptor owned by this process.
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
}

fn listener_matches_path(listener: &UnixListener, socket_path: &Path) -> bool {
    listener
        .local_addr()
        .ok()
        .and_then(|addr| addr.as_pathname().map(|path| path == socket_path))
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn getsockopt_int(fd: RawFd, option: libc::c_int) -> Option<libc::c_int> {
    let mut value: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `value` and `len` are valid for writes.
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            option,
            &mut value as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    (ret == 0).then_some(value)
}

#[cfg(target_os = "linux")]
fn fd_is_unix_socket(fd: RawFd) -> bool {
    getsockopt_int(fd, libc::SO_DOMAIN) == Some(libc::AF_UNIX)
        && getsockopt_int(fd, libc::SO_TYPE) == Some(libc::SOCK_STREAM)
        && getsockopt_int(fd, libc::SO_ACCEPTCONN) == Some(1)
}

#[cfg(not(target_os = "linux"))]
fn fd_is_unix_socket(_fd: RawFd) -> bool {
    false
}

/// Finish startup when systemd already owns the listening socket.
fn bind_inherited(
    socket_path: &Path,
    listener: UnixListener,
) -> io::Result<(File, UnixListener, crate::paths::SocketIdentity)> {
    validate_socket_path(socket_path)?;
    let lock = acquire_socket_lock(socket_path)?;

    if !listener_matches_path(&listener, socket_path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "systemd passed a listener that is not bound to '{}'",
                socket_path.display()
            ),
        ));
    }

    let metadata = std::fs::symlink_metadata(socket_path)?;
    let ours = unsafe { libc::geteuid() };
    if !metadata.file_type().is_socket() || metadata.uid() != ours {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "socket-activated path '{}' is not a socket owned by uid {ours}",
                socket_path.display()
            ),
        ));
    }
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    let identity = crate::paths::SocketIdentity::from_path(socket_path)?;
    Ok((lock, listener, identity))
}

/// Bind the IPC socket without ever deleting a file that is not our own stale
/// socket. The returned lock file must be kept alive for the daemon's
/// lifetime.
fn bind_socket(
    socket_path: &Path,
) -> io::Result<(File, UnixListener, crate::paths::SocketIdentity)> {
    validate_socket_path(socket_path)?;
    let ours = unsafe { libc::geteuid() };
    let lock = acquire_socket_lock(socket_path)?;

    match std::fs::symlink_metadata(socket_path) {
        Ok(existing) => {
            if !existing.file_type().is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "refusing to replace '{}': it is not a socket",
                        socket_path.display()
                    ),
                ));
            }
            if existing.uid() != ours {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "refusing to replace '{}': socket is owned by uid {}, not current uid {ours}",
                        socket_path.display(),
                        existing.uid()
                    ),
                ));
            }
            match UnixStream::connect(socket_path) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "a daemon is already listening on '{}'",
                            socket_path.display()
                        ),
                    ));
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) =>
                {
                    info!("Removing stale socket at {:?}", socket_path);
                    std::fs::remove_file(socket_path)?;
                }
                Err(e) => {
                    return Err(io::Error::new(
                        e.kind(),
                        format!(
                            "refusing to replace '{}': cannot prove it is stale: {e}",
                            socket_path.display()
                        ),
                    ));
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    let listener = UnixListener::bind(socket_path)?;
    if let Err(e) = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600)) {
        let _ = std::fs::remove_file(socket_path);
        return Err(e);
    }
    let identity = crate::paths::SocketIdentity::from_path(socket_path)?;
    Ok((lock, listener, identity))
}

impl Server {
    /// Bind to the given socket path, loading config and creating a ProviderManager.
    ///
    /// A file at the socket path is only replaced when it is a socket owned by
    /// the current user and no process is listening on it.
    pub fn bind(socket_path: &Path) -> io::Result<Self> {
        let inherited = inherited_listener();
        let socket_owned = inherited.is_none();
        let (socket_lock, listener, socket_identity) = match inherited {
            Some(listener) => bind_inherited(socket_path, listener)?,
            None => bind_socket(socket_path)?,
        };
        info!("IPC server listening on {:?}", socket_path);

        // Resolve config + any Lua-defined user providers in a single pass.
        // When the `lua` feature is off the daemon falls back to the
        // TOML-only loader used pre-Phase-3.
        #[cfg(feature = "lua")]
        let (config, user_providers, lua_ctx, lua_path) = {
            let loaded = LoadedConfig::load_or_default();
            let user_providers: Vec<Box<dyn Provider>> = if let Some(ctx) = &loaded.lua {
                loaded
                    .user_providers
                    .iter()
                    .cloned()
                    .map(|spec| {
                        Box::new(crate::lua::LuaProvider::new(spec, ctx.lua_handle()))
                            as Box<dyn Provider>
                    })
                    .collect()
            } else {
                Vec::new()
            };
            (
                Arc::new(RwLock::new(loaded.config)),
                user_providers,
                Arc::new(Mutex::new(loaded.lua)),
                loaded.lua_path,
            )
        };

        #[cfg(not(feature = "lua"))]
        let (config, user_providers) = (
            Arc::new(RwLock::new(Config::load_or_default())),
            Vec::<Box<dyn Provider>>::new(),
        );

        let provider_manager =
            ProviderManager::new_with_config(Arc::clone(&config), user_providers);
        let frecency =
            FrecencyStore::with_config(config.read().unwrap().providers.frecency_prune_config());
        let provider_manager = Arc::new(RwLock::new(provider_manager));

        // Watch unconditionally: `ApplicationProvider` is always registered
        // (see `new_with_provider_config`), `providers.applications` only gates
        // filtering in `filter.rs`, and SIGHUP / Lua reload can flip that flag
        // long after startup. Deciding the watcher's lifetime from boot config
        // would strand it off for the rest of the daemon's life.
        let _application_watcher = match crate::application_watcher::ApplicationWatcher::spawn(
            crate::paths::system_data_dirs(),
            crate::paths::xdg_application_dirs(),
            Arc::clone(&provider_manager),
        ) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                warn!(
                    "application refresh disabled: failed to watch XDG application directories; use `owlry refresh app` — {error}"
                );
                None
            }
        };

        // Spawn the hot-reload watcher when an owlry.lua was actually loaded.
        // Disabled silently for TOML/defaults — no Lua → nothing to watch.
        #[cfg(feature = "lua")]
        let _lua_watcher = match lua_path {
            Some(path) => match crate::lua::ConfigWatcher::spawn(
                path,
                Arc::clone(&config),
                Arc::clone(&provider_manager),
                Arc::clone(&lua_ctx),
            ) {
                Ok(w) => Some(w),
                Err(e) => {
                    warn!(
                        "hot-reload disabled: failed to start filesystem watcher — {}",
                        e
                    );
                    None
                }
            },
            None => None,
        };

        Ok(Self {
            listener,
            socket_path: socket_path.to_path_buf(),
            socket_identity,
            socket_owned,
            _socket_lock: socket_lock,
            provider_manager,
            frecency: Arc::new(RwLock::new(frecency)),
            config,
            _application_watcher,
            #[cfg(feature = "lua")]
            _lua_ctx: lua_ctx,
            #[cfg(feature = "lua")]
            _lua_watcher,
        })
    }

    /// Accept connections in a loop, spawning a thread per client.
    pub fn run(&self) -> io::Result<()> {
        // SIGHUP reloads the same complete state as the Lua file watcher:
        // effective config, enabled providers, user providers, and Lua context.
        #[cfg(feature = "lua")]
        {
            use signal_hook::consts::SIGHUP;
            use signal_hook::iterator::Signals;
            let config = Arc::clone(&self.config);
            let provider_manager = Arc::clone(&self.provider_manager);
            let lua_ctx = Arc::clone(&self._lua_ctx);
            let mut signals = Signals::new([SIGHUP]).map_err(io::Error::other)?;
            thread::spawn(move || {
                for _sig in signals.forever() {
                    match LoadedConfig::load() {
                        Ok(loaded) => match crate::lua::watcher::apply_loaded_config(
                            loaded,
                            &config,
                            &provider_manager,
                            &lua_ctx,
                        ) {
                            Ok(()) => info!("Config and providers reloaded via SIGHUP"),
                            Err(message) => warn!("SIGHUP: {message}; reload skipped"),
                        },
                        Err(e) => {
                            warn!("SIGHUP: failed to reload config: {}", e);
                        }
                    }
                }
            });
        }

        #[cfg(not(feature = "lua"))]
        {
            use signal_hook::consts::SIGHUP;
            use signal_hook::iterator::Signals;
            let config = Arc::clone(&self.config);
            let provider_manager = Arc::clone(&self.provider_manager);
            let mut signals = Signals::new([SIGHUP]).map_err(io::Error::other)?;
            thread::spawn(move || {
                for _sig in signals.forever() {
                    match Config::load() {
                        Ok(new_cfg) => {
                            let new_pm = ProviderManager::new_with_provider_config(
                                new_cfg.providers.clone(),
                                Vec::new(),
                            );
                            let cfg_guard = config.write();
                            let pm_guard = provider_manager.write();
                            match (cfg_guard, pm_guard) {
                                (Ok(mut cfg), Ok(mut pm)) => {
                                    *cfg = new_cfg;
                                    *pm = new_pm;
                                    info!("Config and providers reloaded via SIGHUP");
                                }
                                _ => warn!("SIGHUP: state lock poisoned; reload skipped"),
                            }
                        }
                        Err(e) => warn!("SIGHUP: failed to reload config: {}", e),
                    }
                }
            });
        }

        // SIGTERM/SIGINT handler: save frecency before exiting.
        // Replaces the ctrlc handler in main.rs so all signal management lives here.
        {
            use signal_hook::consts::{SIGINT, SIGTERM};
            use signal_hook::iterator::Signals;
            let frecency = Arc::clone(&self.frecency);
            let socket_path = self.socket_path.clone();
            let socket_identity = self.socket_identity;
            let socket_owned = self.socket_owned;
            let mut signals = Signals::new([SIGTERM, SIGINT]).map_err(io::Error::other)?;
            thread::spawn(move || {
                // Block until we receive SIGTERM or SIGINT, then save and exit.
                let _ = signals.forever().next();
                match frecency.write() {
                    Ok(mut f) => {
                        if let Err(e) = f.save() {
                            warn!("Shutdown: frecency save failed: {}", e);
                        } else {
                            info!("Shutdown: frecency saved");
                        }
                    }
                    Err(_) => {
                        warn!("Shutdown: frecency lock poisoned; skipping save");
                    }
                }
                if socket_owned {
                    let _ = crate::paths::remove_socket_if_owned(&socket_path, socket_identity);
                }
                std::process::exit(0);
            });
        }

        // Periodic frecency auto-save every 5 minutes.
        {
            let frecency = Arc::clone(&self.frecency);
            thread::spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(300));
                    match frecency.write() {
                        Ok(mut f) => {
                            if let Err(e) = f.save() {
                                warn!("Periodic frecency save failed: {}", e);
                            }
                        }
                        Err(_) => {
                            warn!("Periodic frecency save: lock poisoned; skipping");
                        }
                    }
                }
            });
        }

        info!("Server entering accept loop");
        for stream in self.listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    if let Err(e) = crate::ipc_auth::ensure_same_uid(&stream) {
                        warn!("Rejecting IPC client: {e}");
                        continue;
                    }
                    match ConnectionGuard::try_acquire() {
                        Some(guard) => {
                            let pm = Arc::clone(&self.provider_manager);
                            let frecency = Arc::clone(&self.frecency);
                            let config = Arc::clone(&self.config);
                            thread::spawn(move || {
                                let _guard = guard; // released on thread exit
                                if let Err(e) = Self::handle_client(stream, pm, frecency, config) {
                                    warn!("Client handler error: {}", e);
                                }
                            });
                        }
                        None => {
                            warn!(
                                "Connection limit reached ({} max); rejecting client",
                                MAX_CONNECTIONS
                            );
                            let resp = Response::Error {
                                message: format!(
                                    "server busy: max {} concurrent connections",
                                    MAX_CONNECTIONS
                                ),
                            };
                            let _ = write_response(&mut stream, &resp);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    if matches!(
                        e.kind(),
                        io::ErrorKind::InvalidInput
                            | io::ErrorKind::NotConnected
                            | io::ErrorKind::BrokenPipe
                    ) {
                        break;
                    }
                    // EMFILE/ENFILE and friends are transient. Avoid a tight
                    // loop that burns a core while the condition persists.
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
        Ok(())
    }

    /// Accept one connection and handle all its requests until EOF.
    ///
    /// Intended for integration tests where spawning a full accept loop
    /// is unnecessary.
    pub fn handle_one_for_testing(&self) -> io::Result<()> {
        let (stream, _addr) = self.listener.accept()?;
        crate::ipc_auth::ensure_same_uid(&stream)?;
        Self::handle_client(
            stream,
            Arc::clone(&self.provider_manager),
            Arc::clone(&self.frecency),
            Arc::clone(&self.config),
        )
    }

    /// Read newline-delimited JSON requests from a single client stream,
    /// dispatch each, and write the JSON response back.
    fn handle_client(
        stream: UnixStream,
        pm: Arc<RwLock<ProviderManager>>,
        frecency: Arc<RwLock<FrecencyStore>>,
        config: Arc<RwLock<Config>>,
    ) -> io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;

        loop {
            let line = match read_bounded_line(&mut reader, MAX_REQUEST_SIZE) {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                    let resp = Response::Error {
                        message: format!("request too large (max {} bytes)", MAX_REQUEST_SIZE),
                    };
                    write_response(&mut writer, &resp)?;
                    break;
                }
                Err(e) => return Err(e),
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let request: Request = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
                    warn!("Malformed request from client: {}", e);
                    let resp = Response::Error {
                        message: format!("invalid request JSON: {}", e),
                    };
                    write_response(&mut writer, &resp)?;
                    continue;
                }
            };

            let response = Self::handle_request(&request, &pm, &frecency, &config);
            write_response(&mut writer, &response)?;
        }

        Ok(())
    }

    /// Dispatch a single request to the appropriate subsystem and return
    /// the response.
    ///
    /// Rebuild any provider whose backing program has appeared or disappeared.
    ///
    /// The detection pass runs under a read lock and is a `PATH` stat per gated
    /// provider, rate-limited inside the manager — about a microsecond, and
    /// nothing at all most calls. The write lock is taken only when something
    /// actually changed, which is once per install or removal rather than once
    /// per keystroke.
    fn refresh_stale_gates(pm: &Arc<RwLock<ProviderManager>>) {
        let stale = match pm.read() {
            Ok(guard) => guard.stale_gated_providers(),
            Err(_) => return,
        };
        if stale.is_empty() {
            return;
        }
        if let Ok(mut guard) = pm.write() {
            guard.refresh_gated(&stale);
        }
    }

    fn handle_request(
        request: &Request,
        pm: &Arc<RwLock<ProviderManager>>,
        frecency: &Arc<RwLock<FrecencyStore>>,
        config: &Arc<RwLock<Config>>,
    ) -> Response {
        match request {
            Request::Query {
                text,
                modes,
                prefix,
            } => {
                // `:recent` routes here as prefix="recent" (via parse_query's
                // unknown-prefix fallback). It's not a real provider — handle it
                // by resolving the frecency store instead of fuzzy search.
                let recent_mode = prefix.as_deref() == Some("recent");
                let mut filter = match modes {
                    Some(m) => ProviderFilter::from_mode_strings(m),
                    None => ProviderFilter::all(),
                };
                if let Some(prefix_str) = prefix {
                    // Mirrors UI-side parse_query → set_prefix. Tolerates
                    // unknown ids by emitting Plugin(prefix_str) — that's
                    // what the dynamic-prefix-fallback in parse_query does.
                    use std::str::FromStr;
                    if let Ok(prefix_type) = crate::providers::ProviderType::from_str(prefix_str) {
                        filter.set_prefix(Some(prefix_type));
                    }
                }
                let (max, weight) = {
                    let cfg = match config.read() {
                        Ok(g) => g,
                        Err(_) => {
                            return Response::Error {
                                message: "internal error: config lock poisoned".into(),
                            };
                        }
                    };
                    (cfg.general.max_results, cfg.providers.frecency_weight)
                };

                // Providers gated on an external program get their PATH check
                // re-run here, debounced. Nothing else notices such a program
                // being installed or removed: it need not ship a `.desktop`
                // file, so the application-directory watcher never fires.
                Self::refresh_stale_gates(pm);

                let pm_guard = match pm.read() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: provider lock poisoned".into(),
                        };
                    }
                };
                let frecency_guard = match frecency.read() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: frecency lock poisoned".into(),
                        };
                    }
                };
                let results = if recent_mode {
                    pm_guard.recent_items(&frecency_guard, max)
                } else {
                    pm_guard.search_with_frecency(text, max, &filter, &frecency_guard, weight, None)
                };

                Response::Results {
                    items: results
                        .into_iter()
                        .map(|(item, score)| launch_item_to_result(item, score))
                        .collect(),
                }
            }

            Request::Launch {
                item_id,
                provider: _,
            } => {
                let mut frecency_guard = match frecency.write() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: frecency lock poisoned".into(),
                        };
                    }
                };
                frecency_guard.record_launch(item_id);
                Response::Ack
            }

            Request::Providers => {
                let pm_guard = match pm.read() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: provider lock poisoned".into(),
                        };
                    }
                };
                let descs = pm_guard.available_providers();
                Response::Providers {
                    list: descs.into_iter().map(descriptor_to_desc).collect(),
                }
            }

            Request::Refresh { provider } => {
                let mut pm_guard = match pm.write() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: provider lock poisoned".into(),
                        };
                    }
                };
                match provider {
                    Some(provider) => {
                        if pm_guard.refresh_provider(provider) {
                            Response::Ack
                        } else {
                            Response::Error {
                                message: format!("unknown refreshable provider '{provider}'"),
                            }
                        }
                    }
                    None => {
                        pm_guard.refresh_all();
                        Response::Ack
                    }
                }
            }

            Request::Toggle => {
                // Toggle visibility is a client-side concern; the daemon just acks.
                Response::Ack
            }

            Request::Submenu { plugin_id, data } => {
                let pm_guard = match pm.read() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: provider lock poisoned".into(),
                        };
                    }
                };
                match pm_guard.query_submenu_actions(plugin_id, data, plugin_id) {
                    Some((_name, actions)) => Response::SubmenuItems {
                        items: actions
                            .into_iter()
                            .map(|item| launch_item_to_result(item, 0))
                            .collect(),
                    },
                    None => Response::Error {
                        message: format!("no submenu actions for plugin '{}'", plugin_id),
                    },
                }
            }

            Request::PluginAction { command } => {
                let pm_guard = match pm.read() {
                    Ok(g) => g,
                    Err(_) => {
                        return Response::Error {
                            message: "internal error: provider lock poisoned".into(),
                        };
                    }
                };
                if pm_guard.execute_plugin_action(command) {
                    Response::Ack
                } else {
                    Response::Error {
                        message: format!("no plugin handled action '{}'", command),
                    }
                }
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Best-effort cleanup, but only of the inode this server created and
        // only when systemd did not pass us the listening socket.
        if self.socket_owned {
            let _ = crate::paths::remove_socket_if_owned(&self.socket_path, self.socket_identity);
        }
    }
}

/// Serialize a response as a single JSON line terminated by newline.
fn write_response(writer: &mut UnixStream, response: &Response) -> io::Result<()> {
    let mut json = serde_json::to_string(response)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    json.push('\n');
    writer.write_all(json.as_bytes())?;
    writer.flush()
}

fn launch_item_to_result(item: LaunchItem, score: i64) -> ResultItem {
    ResultItem {
        id: item.id,
        title: item.name,
        description: item.description.unwrap_or_default(),
        icon: item.icon.unwrap_or_default(),
        provider: format!("{}", item.provider),
        score,
        command: Some(item.command),
        terminal: item.terminal,
        activation: item.activation,
        tags: item.tags,
        source: item.source,
    }
}

fn descriptor_to_desc(desc: crate::providers::ProviderDescriptor) -> ProviderDesc {
    ProviderDesc {
        id: desc.id,
        name: desc.name,
        prefix: desc.prefix,
        icon: desc.icon,
        position: desc.position,
        tab_label: desc.tab_label,
        search_noun: desc.search_noun,
        kind: desc.kind.as_str().to_string(),
        item_count: desc.item_count,
        backing_program: desc.backing_program,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    // Wrap a Cursor in a BufReader backed by a UnixStream-like interface.
    // Since read_bounded_line takes BufReader<UnixStream>, we test it indirectly
    // through an in-memory byte slice via a helper.
    fn bounded_line_from_bytes(data: &[u8], max: usize) -> io::Result<Option<String>> {
        // Use a pipe to simulate UnixStream I/O.
        use std::os::unix::net::UnixStream;
        let (mut write_end, read_end) = UnixStream::pair()?;
        write_end.write_all(data)?;
        drop(write_end); // Signal EOF to reader
        let mut reader = BufReader::new(read_end);
        read_bounded_line(&mut reader, max)
    }

    #[test]
    fn normal_line_within_limit() {
        let result = bounded_line_from_bytes(b"hello world\n", 100).unwrap();
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn line_at_exactly_max_succeeds() {
        // "aaa...a\n" where content is exactly max bytes
        let mut data = vec![b'a'; 100];
        data.push(b'\n');
        let result = bounded_line_from_bytes(&data, 100).unwrap();
        assert_eq!(result, Some("a".repeat(100)));
    }

    #[test]
    fn line_exceeding_max_errors() {
        let mut data = vec![b'a'; 101];
        data.push(b'\n');
        let result = bounded_line_from_bytes(&data, 100);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn empty_input_returns_none() {
        let result = bounded_line_from_bytes(b"", 100).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn no_trailing_newline_returns_content() {
        let result = bounded_line_from_bytes(b"hello", 100).unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("owlry-server-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fd_domain_check_accepts_only_listening_unix_streams() {
        let dir = test_dir("fd-domain");
        let listener = UnixListener::bind(dir.join("owlry.sock")).unwrap();
        assert!(fd_is_unix_socket(listener.as_raw_fd()));

        let (connected, _peer) = UnixStream::pair().unwrap();
        assert!(!fd_is_unix_socket(connected.as_raw_fd()));

        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(!fd_is_unix_socket(tcp.as_raw_fd()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inherited_listener_must_match_the_configured_path() {
        let dir = test_dir("listener-path");
        let path = dir.join("owlry.sock");
        let listener = UnixListener::bind(&path).unwrap();
        assert!(listener_matches_path(&listener, &path));
        assert!(!listener_matches_path(&listener, &dir.join("other.sock")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inherited_listener_gets_cloexec() {
        let (stream, _peer) = UnixStream::pair().unwrap();
        set_cloexec(stream.as_raw_fd());
        let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
    }

    #[test]
    fn bind_socket_sets_0600_and_releases_lock_on_drop() {
        let dir = test_dir("bind");
        let path = dir.join("owlry.sock");
        let (lock, listener, identity) = bind_socket(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(
            crate::paths::SocketIdentity::from_path(&path).unwrap(),
            identity
        );
        drop(lock);
        drop(listener);
        crate::paths::remove_socket_if_owned(&path, identity).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bind_socket_refuses_to_replace_a_regular_file() {
        let dir = test_dir("regular");
        let path = dir.join("owlry.sock");
        std::fs::write(&path, b"do not delete me").unwrap();

        let err = bind_socket(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"do not delete me");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bind_socket_refuses_a_second_live_listener() {
        let dir = test_dir("live");
        let path = dir.join("owlry.sock");
        let (_lock, _listener, _identity) = bind_socket(&path).unwrap();

        let err = bind_socket(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bind_socket_recovers_a_stale_socket_owned_by_us() {
        let dir = test_dir("stale");
        let path = dir.join("owlry.sock");
        let (lock, listener, identity) = bind_socket(&path).unwrap();
        drop(listener);
        // The path is left behind, as it would be after a crash.
        assert!(path.exists());

        // The lock is still held, so a second daemon must wait for the first
        // process to exit. Releasing it models a restart.
        drop(lock);
        let (new_lock, new_listener, new_identity) = bind_socket(&path).unwrap();
        assert_ne!(identity, new_identity);
        drop(new_lock);
        drop(new_listener);
        crate::paths::remove_socket_if_owned(&path, new_identity).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bind_socket_refuses_a_hardlinked_lock_file() {
        let dir = test_dir("hardlink-lock");
        let path = dir.join("owlry.sock");
        let lock = dir.join("owlry.sock.lock");
        let target = dir.join("target");
        std::fs::write(&target, b"not a lock").unwrap();
        std::fs::hard_link(&target, &lock).unwrap();

        let err = bind_socket(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read(&target).unwrap(), b"not a lock");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bind_socket_refuses_a_group_writable_parent() {
        let dir = test_dir("unsafe-parent");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let path = dir.join("owlry.sock");

        let err = bind_socket(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_socket_if_owned_skips_a_replacement_inode() {
        let dir = test_dir("replacement");
        let path = dir.join("owlry.sock");
        let (lock, listener, identity) = bind_socket(&path).unwrap();
        drop(lock);
        drop(listener);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"replacement").unwrap();

        assert!(!crate::paths::remove_socket_if_owned(&path, identity).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
