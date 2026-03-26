use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use owlry_core::ipc::{ProviderDesc, Request, Response, ResultItem};

/// IPC client that connects to the owlry-core daemon Unix socket
/// and provides typed methods for all IPC operations.
pub struct CoreClient {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl CoreClient {
    /// Connect to a running daemon at the given socket path.
    ///
    /// Sets a 5-second read timeout so the client doesn't hang indefinitely
    /// if the daemon stops responding.
    pub fn connect(socket_path: &Path) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    /// Try connecting to the daemon. If the socket isn't available, attempt
    /// to start the daemon via systemd and retry with exponential backoff.
    ///
    /// Backoff schedule: 100ms, 200ms, 400ms.
    pub fn connect_or_start() -> io::Result<Self> {
        let path = Self::socket_path();

        // First attempt: just try connecting.
        if let Ok(client) = Self::connect(&path) {
            return Ok(client);
        }

        // Socket not available — try to start the daemon.
        let status = std::process::Command::new("systemctl")
            .args(["--user", "start", "owlry-core"])
            .status()
            .map_err(|e| {
                io::Error::other(format!("failed to start owlry-core via systemd: {e}"))
            })?;

        if !status.success() {
            return Err(io::Error::other(format!(
                "systemctl --user start owlry-core exited with status {}",
                status
            )));
        }

        // Retry with exponential backoff.
        let delays = [100, 200, 400];
        for (i, ms) in delays.iter().enumerate() {
            std::thread::sleep(Duration::from_millis(*ms));
            match Self::connect(&path) {
                Ok(client) => return Ok(client),
                Err(e) if i == delays.len() - 1 => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        format!("daemon started but socket not available after retries: {e}"),
                    ));
                }
                Err(_) => continue,
            }
        }

        unreachable!()
    }

    /// Default socket path: `$XDG_RUNTIME_DIR/owlry/owlry.sock`.
    ///
    /// Delegates to `owlry_core::paths::socket_path()` to keep a single
    /// source of truth.
    pub fn socket_path() -> PathBuf {
        owlry_core::paths::socket_path()
    }

    /// Send a search query and return matching results.
    pub fn query(&mut self, text: &str, modes: Option<Vec<String>>) -> io::Result<Vec<ResultItem>> {
        self.send(&Request::Query {
            text: text.to_string(),
            modes,
        })?;

        match self.receive()? {
            Response::Results { items } => Ok(items),
            Response::Error { message } => Err(io::Error::other(message)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected response to Query: {other:?}"),
            )),
        }
    }

    /// Record a launch event for frecency tracking.
    pub fn launch(&mut self, item_id: &str, provider: &str) -> io::Result<()> {
        self.send(&Request::Launch {
            item_id: item_id.to_string(),
            provider: provider.to_string(),
        })?;

        match self.receive()? {
            Response::Ack => Ok(()),
            Response::Error { message } => Err(io::Error::other(message)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected response to Launch: {other:?}"),
            )),
        }
    }

    /// List all available providers from the daemon.
    pub fn providers(&mut self) -> io::Result<Vec<ProviderDesc>> {
        self.send(&Request::Providers)?;

        match self.receive()? {
            Response::Providers { list } => Ok(list),
            Response::Error { message } => Err(io::Error::other(message)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected response to Providers: {other:?}"),
            )),
        }
    }

    /// Toggle the launcher window visibility.
    pub fn toggle(&mut self) -> io::Result<()> {
        self.send(&Request::Toggle)?;

        match self.receive()? {
            Response::Ack => Ok(()),
            Response::Error { message } => Err(io::Error::other(message)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected response to Toggle: {other:?}"),
            )),
        }
    }

    /// Execute a plugin action command (e.g., "POMODORO:start").
    /// Returns Ok(true) if the plugin handled the action, Ok(false) if not.
    pub fn plugin_action(&mut self, command: &str) -> io::Result<bool> {
        self.send(&Request::PluginAction {
            command: command.to_string(),
        })?;

        match self.receive()? {
            Response::Ack => Ok(true),
            Response::Error { .. } => Ok(false),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected response to PluginAction: {other:?}"),
            )),
        }
    }

    /// Query a plugin's submenu actions.
    pub fn submenu(&mut self, plugin_id: &str, data: &str) -> io::Result<Vec<ResultItem>> {
        self.send(&Request::Submenu {
            plugin_id: plugin_id.to_string(),
            data: data.to_string(),
        })?;

        match self.receive()? {
            Response::SubmenuItems { items } => Ok(items),
            Response::Error { message } => Err(io::Error::other(message)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected response to Submenu: {other:?}"),
            )),
        }
    }

    // =========================================================================
    // Internal helpers
    // =========================================================================

    fn send(&mut self, request: &Request) -> io::Result<()> {
        let json = serde_json::to_string(request)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        writeln!(self.stream, "{json}")?;
        self.stream.flush()
    }

    fn receive(&mut self) -> io::Result<Response> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        if line.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "daemon closed the connection",
            ));
        }
        serde_json::from_str(line.trim()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::thread;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Spawn a mock server that accepts one connection, reads one request,
    /// and replies with the given canned response. Each call gets a unique
    /// socket path to avoid collisions when tests run in parallel.
    fn mock_server(response: Response) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("owlry-test-{}-{}", std::process::id(), n));
        let _ = std::fs::create_dir_all(&dir);
        let sock = dir.join("test.sock");
        let _ = std::fs::remove_file(&sock);

        let listener = UnixListener::bind(&sock).expect("bind mock socket");
        let sock_clone = sock.clone();

        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;

            // Read one request line (we don't care about contents).
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request");

            // Send canned response.
            let mut json = serde_json::to_string(&response).unwrap();
            json.push('\n');
            writer.write_all(json.as_bytes()).unwrap();
            writer.flush().unwrap();

            // Clean up socket after test.
            let _ = std::fs::remove_file(&sock_clone);
            let _ = std::fs::remove_dir(dir);
        });

        sock
    }

    #[test]
    fn connect_and_query_returns_results() {
        let canned = Response::Results {
            items: vec![ResultItem {
                id: "firefox".into(),
                title: "Firefox".into(),
                description: "Web Browser".into(),
                icon: "firefox".into(),
                provider: "app".into(),
                score: 100,
                command: Some("firefox".into()),
                terminal: false,
                tags: vec![],
            }],
        };

        let sock = mock_server(canned);
        // Give the listener thread a moment to start.
        thread::sleep(Duration::from_millis(50));

        let mut client = CoreClient::connect(&sock).expect("connect");
        let results = client.query("fire", None).expect("query");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "firefox");
        assert_eq!(results[0].title, "Firefox");
        assert_eq!(results[0].score, 100);
    }

    #[test]
    fn toggle_returns_ack() {
        let sock = mock_server(Response::Ack);
        thread::sleep(Duration::from_millis(50));

        let mut client = CoreClient::connect(&sock).expect("connect");
        client.toggle().expect("toggle should succeed");
    }

    #[test]
    fn launch_returns_ack() {
        let sock = mock_server(Response::Ack);
        thread::sleep(Duration::from_millis(50));

        let mut client = CoreClient::connect(&sock).expect("connect");
        client
            .launch("firefox", "app")
            .expect("launch should succeed");
    }

    #[test]
    fn providers_returns_list() {
        let canned = Response::Providers {
            list: vec![ProviderDesc {
                id: "app".into(),
                name: "Applications".into(),
                prefix: Some(":app".into()),
                icon: "application-x-executable".into(),
                position: "normal".into(),
            }],
        };

        let sock = mock_server(canned);
        thread::sleep(Duration::from_millis(50));

        let mut client = CoreClient::connect(&sock).expect("connect");
        let providers = client.providers().expect("providers");

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "app");
    }

    #[test]
    fn submenu_returns_items() {
        let canned = Response::SubmenuItems {
            items: vec![ResultItem {
                id: "start".into(),
                title: "Start Service".into(),
                description: String::new(),
                icon: "media-playback-start".into(),
                provider: "systemd".into(),
                score: 0,
                command: Some("systemctl --user start foo".into()),
                terminal: false,
                tags: vec![],
            }],
        };

        let sock = mock_server(canned);
        thread::sleep(Duration::from_millis(50));

        let mut client = CoreClient::connect(&sock).expect("connect");
        let items = client.submenu("systemd", "foo.service").expect("submenu");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "start");
    }

    #[test]
    fn error_response_is_propagated() {
        let canned = Response::Error {
            message: "something went wrong".into(),
        };

        let sock = mock_server(canned);
        thread::sleep(Duration::from_millis(50));

        let mut client = CoreClient::connect(&sock).expect("connect");
        let err = client.query("test", None).unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("something went wrong"),
            "error message should contain the server error, got: {msg}"
        );
    }

    #[test]
    fn socket_path_delegates_to_core() {
        let path = CoreClient::socket_path();
        assert!(path.ends_with("owlry/owlry.sock"));
    }
}
