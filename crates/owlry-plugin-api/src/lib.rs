//! # Owlry Plugin API
//!
//! This crate provides the ABI-stable interface for owlry native plugins.
//! Plugins are compiled as dynamic libraries (.so) and loaded at runtime.
//!
//! ## Creating a Plugin
//!
//! ```ignore
//! use owlry_plugin_api::*;
//!
//! // Define your plugin's vtable
//! static VTABLE: PluginVTable = PluginVTable {
//!     info: plugin_info,
//!     providers: plugin_providers,
//!     provider_init: my_provider_init,
//!     provider_refresh: my_provider_refresh,
//!     provider_query: my_provider_query,
//!     provider_drop: my_provider_drop,
//! };
//!
//! // Export the vtable
//! #[no_mangle]
//! pub extern "C" fn owlry_plugin_vtable() -> &'static PluginVTable {
//!     &VTABLE
//! }
//! ```

use abi_stable::StableAbi;

// Re-export abi_stable types for use by consumers (runtime loader, plugins)
pub use abi_stable::std_types::{ROption, RStr, RString, RVec};

/// Current plugin API version - plugins must match this
/// v2: Added ProviderPosition for widget support
/// v3: Added priority field for plugin-declared result ordering
/// v4: Added get_config_string/int/bool to HostAPI for plugin config access
pub const API_VERSION: u32 = 4;

/// Plugin metadata returned by the info function
#[repr(C)]
#[derive(StableAbi, Clone, Debug)]
pub struct PluginInfo {
    /// Unique plugin identifier (e.g., "calculator", "weather")
    pub id: RString,
    /// Human-readable plugin name
    pub name: RString,
    /// Plugin version string
    pub version: RString,
    /// Short description of what the plugin provides
    pub description: RString,
    /// Plugin API version (must match API_VERSION)
    pub api_version: u32,
}

/// Information about a provider offered by a plugin
#[repr(C)]
#[derive(StableAbi, Clone, Debug)]
pub struct ProviderInfo {
    /// Unique provider identifier within the plugin
    pub id: RString,
    /// Human-readable provider name
    pub name: RString,
    /// Optional prefix that activates this provider (e.g., "=" for calculator)
    pub prefix: ROption<RString>,
    /// Default icon name for results from this provider
    pub icon: RString,
    /// Provider type (static or dynamic)
    pub provider_type: ProviderKind,
    /// Short type identifier for UI badges (e.g., "calc", "web")
    pub type_id: RString,
    /// Display position (Normal or Widget)
    pub position: ProviderPosition,
    /// Priority for result ordering (higher values appear first)
    /// Suggested ranges:
    /// - Widgets: 10000-12000
    /// - Dynamic providers: 7000-10000
    /// - Static providers: 0-5000 (use 0 for frecency-based ordering)
    pub priority: i32,
}

/// Provider behavior type
#[repr(C)]
#[derive(StableAbi, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    /// Static providers load items once at startup via refresh()
    Static,
    /// Dynamic providers evaluate queries in real-time via query()
    Dynamic,
}

/// Provider display position
///
/// Controls where in the result list this provider's items appear.
#[repr(C)]
#[derive(StableAbi, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProviderPosition {
    /// Standard position in results (sorted by score/frecency)
    #[default]
    Normal,
    /// Widget position - appears at top of results when query is empty
    /// Widgets are always visible regardless of filter settings
    Widget,
}

/// A single searchable/launchable item returned by providers
#[repr(C)]
#[derive(StableAbi, Clone, Debug)]
pub struct PluginItem {
    /// Unique item identifier
    pub id: RString,
    /// Display name
    pub name: RString,
    /// Optional description shown below the name
    pub description: ROption<RString>,
    /// Optional icon name or path
    pub icon: ROption<RString>,
    /// Command to execute when selected
    pub command: RString,
    /// Whether to run in a terminal
    pub terminal: bool,
    /// Search keywords/tags for filtering
    pub keywords: RVec<RString>,
    /// Score boost for frecency (higher = more prominent)
    pub score_boost: i32,
}

impl PluginItem {
    /// Create a new plugin item with required fields
    pub fn new(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: RString::from(id.into()),
            name: RString::from(name.into()),
            description: ROption::RNone,
            icon: ROption::RNone,
            command: RString::from(command.into()),
            terminal: false,
            keywords: RVec::new(),
            score_boost: 0,
        }
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = ROption::RSome(RString::from(desc.into()));
        self
    }

    /// Set the icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = ROption::RSome(RString::from(icon.into()));
        self
    }

    /// Set terminal mode
    pub fn with_terminal(mut self, terminal: bool) -> Self {
        self.terminal = terminal;
        self
    }

    /// Add keywords
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords.into_iter().map(RString::from).collect();
        self
    }

    /// Set score boost
    pub fn with_score_boost(mut self, boost: i32) -> Self {
        self.score_boost = boost;
        self
    }
}

/// Plugin function table - defines the interface between owlry and plugins
///
/// Every native plugin must export a function `owlry_plugin_vtable` that returns
/// a static reference to this structure.
#[repr(C)]
#[derive(StableAbi)]
pub struct PluginVTable {
    /// Return plugin metadata
    pub info: extern "C" fn() -> PluginInfo,

    /// Return list of providers this plugin offers
    pub providers: extern "C" fn() -> RVec<ProviderInfo>,

    /// Initialize a provider by ID, returns an opaque handle
    /// The handle is passed to refresh/query/drop functions
    pub provider_init: extern "C" fn(provider_id: RStr<'_>) -> ProviderHandle,

    /// Refresh a static provider's items
    /// Called once at startup and when user requests refresh
    pub provider_refresh: extern "C" fn(handle: ProviderHandle) -> RVec<PluginItem>,

    /// Query a dynamic provider
    /// Called on each keystroke for dynamic providers
    pub provider_query: extern "C" fn(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem>,

    /// Clean up a provider handle
    pub provider_drop: extern "C" fn(handle: ProviderHandle),
}

/// Opaque handle to a provider instance
/// Plugins can use this to store state between calls
#[repr(C)]
#[derive(StableAbi, Clone, Copy, Debug)]
pub struct ProviderHandle {
    /// Opaque pointer to provider state
    pub ptr: *mut (),
}

impl ProviderHandle {
    /// Create a null handle
    pub fn null() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
        }
    }

    /// Create a handle from a boxed value
    /// The caller is responsible for calling drop to free the memory
    pub fn from_box<T>(value: Box<T>) -> Self {
        Self {
            ptr: Box::into_raw(value) as *mut (),
        }
    }

    /// Convert handle back to a reference (unsafe)
    ///
    /// # Safety
    /// The handle must have been created from a Box<T> of the same type
    pub unsafe fn as_ref<T>(&self) -> Option<&T> {
        // SAFETY: Caller guarantees the pointer was created from Box<T>
        unsafe { (self.ptr as *const T).as_ref() }
    }

    /// Convert handle back to a mutable reference (unsafe)
    ///
    /// # Safety
    /// The handle must have been created from a Box<T> of the same type
    pub unsafe fn as_mut<T>(&mut self) -> Option<&mut T> {
        // SAFETY: Caller guarantees the pointer was created from Box<T>
        unsafe { (self.ptr as *mut T).as_mut() }
    }

    /// Drop the handle and free its memory (unsafe)
    ///
    /// # Safety
    /// The handle must have been created from a Box<T> of the same type
    /// and must not be used after this call
    pub unsafe fn drop_as<T>(self) {
        if !self.ptr.is_null() {
            // SAFETY: Caller guarantees the pointer was created from Box<T>
            unsafe { drop(Box::from_raw(self.ptr as *mut T)) };
        }
    }
}

// ProviderHandle contains a raw pointer but we manage it carefully
unsafe impl Send for ProviderHandle {}
unsafe impl Sync for ProviderHandle {}

// ============================================================================
// Host API - Functions the host provides to plugins
// ============================================================================

/// Notification urgency level
#[repr(C)]
#[derive(StableAbi, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NotifyUrgency {
    /// Low priority notification
    Low = 0,
    /// Normal priority notification (default)
    #[default]
    Normal = 1,
    /// Critical/urgent notification
    Critical = 2,
}

/// Host API function table
///
/// This structure contains functions that the host (owlry) provides to plugins.
/// Plugins can call these functions to interact with the system.
#[repr(C)]
#[derive(StableAbi, Clone, Copy)]
pub struct HostAPI {
    /// Send a notification to the user
    /// Parameters: summary, body, icon (optional, empty string for none), urgency
    pub notify:
        extern "C" fn(summary: RStr<'_>, body: RStr<'_>, icon: RStr<'_>, urgency: NotifyUrgency),

    /// Log a message at info level
    pub log_info: extern "C" fn(message: RStr<'_>),

    /// Log a message at warning level
    pub log_warn: extern "C" fn(message: RStr<'_>),

    /// Log a message at error level
    pub log_error: extern "C" fn(message: RStr<'_>),

    /// Read a string value from this plugin's config section.
    /// Parameters: plugin_id (the calling plugin's ID), key
    /// Returns RSome(value) if set, RNone otherwise.
    pub get_config_string:
        extern "C" fn(plugin_id: RStr<'_>, key: RStr<'_>) -> ROption<RString>,

    /// Read an integer value from this plugin's config section.
    pub get_config_int: extern "C" fn(plugin_id: RStr<'_>, key: RStr<'_>) -> ROption<i64>,

    /// Read a boolean value from this plugin's config section.
    pub get_config_bool: extern "C" fn(plugin_id: RStr<'_>, key: RStr<'_>) -> ROption<bool>,
}

use std::sync::OnceLock;

// Global host API pointer - set by the host when loading plugins
static HOST_API: OnceLock<&'static HostAPI> = OnceLock::new();

/// Initialize the host API (called by the host)
///
/// # Safety
/// Must only be called once by the host before any plugins use the API
pub unsafe fn init_host_api(api: &'static HostAPI) {
    let _ = HOST_API.set(api);
}

/// Get the host API
///
/// Returns None if the host hasn't initialized the API yet
pub fn host_api() -> Option<&'static HostAPI> {
    HOST_API.get().copied()
}

// ============================================================================
// Convenience functions for plugins
// ============================================================================

/// Send a notification (convenience wrapper)
pub fn notify(summary: &str, body: &str) {
    if let Some(api) = host_api() {
        (api.notify)(
            RStr::from_str(summary),
            RStr::from_str(body),
            RStr::from_str(""),
            NotifyUrgency::Normal,
        );
    }
}

/// Send a notification with an icon (convenience wrapper)
pub fn notify_with_icon(summary: &str, body: &str, icon: &str) {
    if let Some(api) = host_api() {
        (api.notify)(
            RStr::from_str(summary),
            RStr::from_str(body),
            RStr::from_str(icon),
            NotifyUrgency::Normal,
        );
    }
}

/// Send a notification with full options (convenience wrapper)
pub fn notify_with_urgency(summary: &str, body: &str, icon: &str, urgency: NotifyUrgency) {
    if let Some(api) = host_api() {
        (api.notify)(
            RStr::from_str(summary),
            RStr::from_str(body),
            RStr::from_str(icon),
            urgency,
        );
    }
}

/// Log an info message (convenience wrapper)
pub fn log_info(message: &str) {
    if let Some(api) = host_api() {
        (api.log_info)(RStr::from_str(message));
    }
}

/// Log a warning message (convenience wrapper)
pub fn log_warn(message: &str) {
    if let Some(api) = host_api() {
        (api.log_warn)(RStr::from_str(message));
    }
}

/// Log an error message (convenience wrapper)
pub fn log_error(message: &str) {
    if let Some(api) = host_api() {
        (api.log_error)(RStr::from_str(message));
    }
}

/// Read a string value from this plugin's config section (convenience wrapper).
/// `plugin_id` must match the ID the plugin declares in its `PluginInfo`.
pub fn get_config_string(plugin_id: &str, key: &str) -> Option<String> {
    host_api().and_then(|api| {
        (api.get_config_string)(RStr::from_str(plugin_id), RStr::from_str(key))
            .into_option()
            .map(|s| s.into_string())
    })
}

/// Read an integer value from this plugin's config section (convenience wrapper).
pub fn get_config_int(plugin_id: &str, key: &str) -> Option<i64> {
    host_api().and_then(|api| {
        (api.get_config_int)(RStr::from_str(plugin_id), RStr::from_str(key)).into_option()
    })
}

/// Read a boolean value from this plugin's config section (convenience wrapper).
pub fn get_config_bool(plugin_id: &str, key: &str) -> Option<bool> {
    host_api().and_then(|api| {
        (api.get_config_bool)(RStr::from_str(plugin_id), RStr::from_str(key)).into_option()
    })
}

/// Helper macro for defining plugin vtables
///
/// Usage:
/// ```ignore
/// owlry_plugin! {
///     info: my_plugin_info,
///     providers: my_providers,
///     init: my_init,
///     refresh: my_refresh,
///     query: my_query,
///     drop: my_drop,
/// }
/// ```
#[macro_export]
macro_rules! owlry_plugin {
    (
        info: $info:expr,
        providers: $providers:expr,
        init: $init:expr,
        refresh: $refresh:expr,
        query: $query:expr,
        drop: $drop:expr $(,)?
    ) => {
        static OWLRY_PLUGIN_VTABLE: $crate::PluginVTable = $crate::PluginVTable {
            info: $info,
            providers: $providers,
            provider_init: $init,
            provider_refresh: $refresh,
            provider_query: $query,
            provider_drop: $drop,
        };

        #[unsafe(no_mangle)]
        pub extern "C" fn owlry_plugin_vtable() -> &'static $crate::PluginVTable {
            &OWLRY_PLUGIN_VTABLE
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_item_builder() {
        let item = PluginItem::new("test-id", "Test Item", "echo hello")
            .with_description("A test item")
            .with_icon("test-icon")
            .with_terminal(true)
            .with_keywords(vec!["test".to_string(), "example".to_string()])
            .with_score_boost(100);

        assert_eq!(item.id.as_str(), "test-id");
        assert_eq!(item.name.as_str(), "Test Item");
        assert_eq!(item.command.as_str(), "echo hello");
        assert!(item.terminal);
        assert_eq!(item.score_boost, 100);
    }

    #[test]
    fn test_provider_handle() {
        let value = Box::new(42i32);
        let handle = ProviderHandle::from_box(value);

        unsafe {
            assert_eq!(*handle.as_ref::<i32>().unwrap(), 42);
            handle.drop_as::<i32>();
        }
    }
}
