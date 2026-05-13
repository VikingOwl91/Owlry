use crate::providers::{ItemSource, LaunchItem, Provider, ProviderType};
use log::debug;
use std::io::{self, BufRead};

/// Provider for dmenu-style input from stdin
pub struct DmenuProvider {
    items: Vec<LaunchItem>,
    enabled: bool,
}

impl Default for DmenuProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DmenuProvider {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            enabled: false,
        }
    }

    /// Check if stdin has data (non-blocking check)
    /// Returns true only if stdin is a pipe or regular file with data available.
    /// Returns false for TTYs, /dev/null, and other character devices.
    pub fn has_stdin_data() -> bool {
        use std::os::unix::io::AsRawFd;

        let stdin_fd = io::stdin().as_raw_fd();

        // First check if stdin is a pipe or regular file (valid dmenu input sources)
        // Character devices (TTY, /dev/null) should NOT trigger dmenu mode
        let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
        let stat_result = unsafe { libc::fstat(stdin_fd, &mut stat_buf) };
        if stat_result != 0 {
            return false;
        }

        let mode = stat_buf.st_mode;
        let is_pipe = (mode & libc::S_IFMT) == libc::S_IFIFO;
        let is_file = (mode & libc::S_IFMT) == libc::S_IFREG;

        // Only check for data if stdin is a pipe or file
        if !is_pipe && !is_file {
            return false;
        }

        // Non-blocking poll to check if data is available
        let mut poll_fd = libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        };

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
                tags: Vec::new(),
                source: ItemSource::Core,
            };

            self.items.push(item);
        }

        debug!("Read {} items from stdin", self.items.len());
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}
