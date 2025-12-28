use super::{LaunchItem, Provider, ProviderType};
use log::debug;
use std::io::{self, BufRead};

/// Provider for dmenu-style input from stdin
pub struct DmenuProvider {
    items: Vec<LaunchItem>,
    enabled: bool,
}

impl DmenuProvider {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            enabled: false,
        }
    }

    /// Check if stdin has data (non-blocking check)
    pub fn has_stdin_data() -> bool {
        use std::os::unix::io::AsRawFd;

        let stdin_fd = io::stdin().as_raw_fd();
        let mut poll_fd = libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        };

        // Non-blocking poll with 0 timeout
        let result = unsafe { libc::poll(&mut poll_fd, 1, 0) };
        result > 0 && (poll_fd.revents & libc::POLLIN) != 0
    }

    /// Enable dmenu mode (called when stdin has data)
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Provider for DmenuProvider {
    fn name(&self) -> &str {
        "dmenu"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Dmenu
    }

    fn refresh(&mut self) {
        self.items.clear();

        if !self.enabled {
            return;
        }

        debug!("Reading dmenu items from stdin");

        let stdin = io::stdin();
        for (idx, line) in stdin.lock().lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let item = LaunchItem {
                id: format!("dmenu:{}", idx),
                name: line.to_string(),
                description: None,
                icon: None,
                provider: ProviderType::Dmenu,
                command: line.to_string(),
                terminal: false,
            };

            self.items.push(item);
        }

        debug!("Read {} items from stdin", self.items.len());
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}
