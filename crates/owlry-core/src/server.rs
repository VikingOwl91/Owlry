use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use log::{error, info, warn};

use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::ipc::{ProviderDesc, Request, Response, ResultItem};
use crate::providers::{LaunchItem, ProviderManager};

/// IPC server that listens on a Unix domain socket and dispatches
/// requests to the provider system.
pub struct Server {
    listener: UnixListener,
    socket_path: PathBuf,
    provider_manager: Arc<Mutex<ProviderManager>>,
    frecency: Arc<Mutex<FrecencyStore>>,
    config: Arc<Config>,
}

impl Server {
    /// Bind to the given socket path, loading config and creating a ProviderManager.
    ///
    /// Removes a stale socket file if one already exists at the path.
    pub fn bind(socket_path: &Path) -> io::Result<Self> {
        // Remove stale socket if present
        if socket_path.exists() {
            info!("Removing stale socket at {:?}", socket_path);
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(socket_path)?;
        info!("IPC server listening on {:?}", socket_path);

        let config = Config::load_or_default();
        let provider_manager = ProviderManager::new_with_config(&config);
        let frecency = FrecencyStore::new();

        Ok(Self {
            listener,
            socket_path: socket_path.to_path_buf(),
            provider_manager: Arc::new(Mutex::new(provider_manager)),
            frecency: Arc::new(Mutex::new(frecency)),
            config: Arc::new(config),
        })
    }

    /// Accept connections in a loop, spawning a thread per client.
    pub fn run(&self) -> io::Result<()> {
        info!("Server entering accept loop");
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let pm = Arc::clone(&self.provider_manager);
                    let frecency = Arc::clone(&self.frecency);
                    let config = Arc::clone(&self.config);
                    thread::spawn(move || {
                        if let Err(e) = Self::handle_client(stream, pm, frecency, config) {
                            warn!("Client handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
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
        pm: Arc<Mutex<ProviderManager>>,
        frecency: Arc<Mutex<FrecencyStore>>,
        config: Arc<Config>,
    ) -> io::Result<()> {
        let reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let request: Request = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
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
    fn handle_request(
        request: &Request,
        pm: &Arc<Mutex<ProviderManager>>,
        frecency: &Arc<Mutex<FrecencyStore>>,
        config: &Arc<Config>,
    ) -> Response {
        match request {
            Request::Query { text, modes } => {
                let filter = match modes {
                    Some(m) => ProviderFilter::from_mode_strings(m),
                    None => ProviderFilter::all(),
                };
                let max = config.general.max_results;
                let weight = config.providers.frecency_weight;

                let pm_guard = pm.lock().unwrap();
                let frecency_guard = frecency.lock().unwrap();
                let results = pm_guard.search_with_frecency(
                    text,
                    max,
                    &filter,
                    &frecency_guard,
                    weight,
                    None,
                );

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
                let mut frecency_guard = frecency.lock().unwrap();
                frecency_guard.record_launch(item_id);
                Response::Ack
            }

            Request::Providers => {
                let pm_guard = pm.lock().unwrap();
                let descs = pm_guard.available_providers();
                Response::Providers {
                    list: descs.into_iter().map(descriptor_to_desc).collect(),
                }
            }

            Request::Refresh { provider } => {
                let mut pm_guard = pm.lock().unwrap();
                pm_guard.refresh_provider(provider);
                Response::Ack
            }

            Request::Toggle => {
                // Toggle visibility is a client-side concern; the daemon just acks.
                Response::Ack
            }

            Request::Submenu { plugin_id, data } => {
                let pm_guard = pm.lock().unwrap();
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
                let pm_guard = pm.lock().unwrap();
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
        // Best-effort cleanup of the socket file
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
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
        tags: item.tags,
    }
}

fn descriptor_to_desc(desc: crate::providers::ProviderDescriptor) -> ProviderDesc {
    ProviderDesc {
        id: desc.id,
        name: desc.name,
        prefix: desc.prefix,
        icon: desc.icon,
        position: desc.position,
    }
}
