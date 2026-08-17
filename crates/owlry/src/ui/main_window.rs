use crate::backend::SearchBackend;
use crate::config::Config;
use crate::filter::ProviderFilter;
use crate::ipc::ProviderDesc;
use crate::providers::{LaunchItem, ProviderType};
use crate::ui::ResultRow;
use crate::ui::provider_meta;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Entry, Label, ListBox, Orientation,
    ScrolledWindow, SelectionMode, ToggleButton,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod launch;
mod navigation;
mod results;
mod signals;

use results::LazyLoadState;

/// Context for an active "Open with…" application picker: the file being
/// opened and its resolved MIME type (used when persisting a default).
#[derive(Debug, Clone)]
struct OpenWithContext {
    path: String,
    mime: String,
}

/// Tracks submenu state for services that have action submenus
#[derive(Debug, Clone, Default)]
struct SubmenuState {
    /// Whether we're currently in a submenu
    active: bool,
    /// The service name being viewed
    service_name: String,
    /// Display name for the header
    display_name: String,
    /// The submenu items (actions)
    items: Vec<LaunchItem>,
    /// Saved search text to restore on exit
    saved_search: String,
    /// When `Some`, the submenu is an "Open with…" application picker for the
    /// given file; selecting a row opens/associates that file rather than
    /// launching the app on its own. `None` for regular action submenus.
    open_with: Option<OpenWithContext>,
}
pub struct MainWindow {
    window: ApplicationWindow,
    search_entry: Entry,
    results_list: ListBox,
    scrolled: ScrolledWindow,
    config: Rc<RefCell<Config>>,
    backend: Rc<RefCell<SearchBackend>>,
    current_results: Rc<RefCell<Vec<LaunchItem>>>,
    filter: Rc<RefCell<ProviderFilter>>,
    mode_label: Label,
    hints_label: Label,
    filter_buttons: Rc<RefCell<HashMap<ProviderType, ToggleButton>>>,
    submenu_state: Rc<RefCell<SubmenuState>>,
    /// Parsed tab config (ProviderTypes for cycling)
    tab_order: Rc<Vec<ProviderType>>,
    /// Custom prompt text (overrides dynamic placeholder when set)
    #[allow(dead_code)]
    custom_prompt: Option<String>,
    /// Lazy loading state
    lazy_state: Rc<RefCell<LazyLoadState>>,
    /// Debounce source ID for cancelling pending searches
    debounce_source: Rc<RefCell<Option<gtk4::glib::SourceId>>>,
    /// Whether we're in dmenu mode (stdin pipe input)
    is_dmenu_mode: bool,
    /// Provider display metadata from daemon (keyed by provider id)
    provider_descs: Rc<HashMap<String, ProviderDesc>>,
}

impl MainWindow {
    pub fn new(
        app: &Application,
        config: Rc<RefCell<Config>>,
        backend: Rc<RefCell<SearchBackend>>,
        filter: Rc<RefCell<ProviderFilter>>,
        custom_prompt: Option<String>,
    ) -> Self {
        let cfg = config.borrow();

        let window = ApplicationWindow::builder()
            .application(app)
            .title("Owlry")
            .default_width(cfg.appearance.width)
            .default_height(cfg.appearance.height)
            .resizable(false)
            .decorated(false)
            .build();

        window.add_css_class("owlry-window");

        // Main container
        let main_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        main_box.add_css_class("owlry-main");

        // Header with mode indicator and filter tabs
        let header_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .margin_bottom(8)
            .build();
        header_box.add_css_class("owlry-header");

        // Mode indicator label
        let mode_label = Label::builder()
            .label(filter.borrow().mode_display_name())
            .halign(gtk4::Align::Start)
            .hexpand(false)
            .build();
        mode_label.add_css_class("owlry-mode-indicator");

        // Filter tabs container
        let filter_tabs = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(4)
            .halign(gtk4::Align::End)
            .hexpand(true)
            .build();
        filter_tabs.add_css_class("owlry-filter-tabs");

        // Get enabled providers from filter (which respects CLI --mode/--providers or config)
        // This makes tabs dynamic based on what's actually enabled
        let enabled = filter.borrow().enabled_providers();
        let tab_strings: Vec<String> = enabled.iter().map(|p| p.to_string()).collect();
        let tab_order = Rc::new(enabled);

        // Fetch provider display metadata from daemon/local backend
        let provider_descs: HashMap<String, ProviderDesc> = backend
            .borrow_mut()
            .available_providers()
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect();
        let provider_descs = Rc::new(provider_descs);

        // Update mode label with resolved plugin name (if applicable)
        mode_label.set_label(&Self::mode_display_name(&filter.borrow(), &provider_descs));

        // Create toggle buttons for each enabled provider
        let filter_buttons =
            Self::create_filter_buttons(&filter_tabs, &filter, &tab_strings, &provider_descs);
        let filter_buttons = Rc::new(RefCell::new(filter_buttons));

        header_box.append(&mode_label);
        header_box.append(&filter_tabs);

        // Search entry with dynamic placeholder (or custom prompt if provided)
        let placeholder = custom_prompt
            .clone()
            .unwrap_or_else(|| Self::build_placeholder(&filter.borrow(), &provider_descs));
        let search_entry = Entry::builder()
            .placeholder_text(&placeholder)
            .hexpand(true)
            .build();

        search_entry.add_css_class("owlry-search");

        // Results list in scrolled window
        let results_list = ListBox::builder()
            .selection_mode(SelectionMode::Single)
            .vexpand(true)
            .build();

        results_list.add_css_class("owlry-results");

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .child(&results_list)
            .build();

        // Hints bar at bottom
        let hints_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .margin_top(8)
            .build();
        hints_box.add_css_class("owlry-hints");

        let hints_label = Label::builder()
            .label(Self::build_hints(&cfg.providers))
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .build();
        hints_label.add_css_class("owlry-hints-label");
        hints_box.append(&hints_label);

        // Assemble layout
        main_box.append(&header_box);
        main_box.append(&search_entry);
        main_box.append(&scrolled.clone());
        main_box.append(&hints_box);
        window.set_child(Some(&main_box));

        drop(cfg);

        let lazy_state = Rc::new(RefCell::new(LazyLoadState::default()));

        // Check if we're in dmenu mode
        let is_dmenu_mode = backend.borrow().is_dmenu_mode();

        let main_window = Self {
            window,
            search_entry,
            results_list,
            scrolled,
            config,
            backend,
            current_results: Rc::new(RefCell::new(Vec::new())),
            filter,
            mode_label,
            hints_label,
            filter_buttons,
            submenu_state: Rc::new(RefCell::new(SubmenuState::default())),
            tab_order,
            custom_prompt,
            lazy_state,
            debounce_source: Rc::new(RefCell::new(None)),
            is_dmenu_mode,
            provider_descs,
        };

        main_window.setup_signals();
        main_window.setup_lazy_loading();

        // Ensure search entry has focus when window is shown
        main_window.search_entry.grab_focus();

        // Schedule widget refresh after window is shown (only for local backend)
        // Widget providers (weather, media, pomodoro) may make network/dbus calls
        // We defer this to avoid blocking startup, then re-render results
        let backend_for_refresh = main_window.backend.clone();
        let search_entry_for_refresh = main_window.search_entry.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            backend_for_refresh.borrow_mut().refresh_widgets();
            // Trigger UI update by emitting changed signal on search entry
            search_entry_for_refresh.emit_by_name::<()>("changed", &[]);
        });

        // Periodic widget refresh — local backend only.
        // In daemon mode, the daemon handles widget refresh internally;
        // the UI gets updated data on the next user-initiated search.
        // We do NOT re-query in daemon mode because it resets the user's
        // scroll position and selection.
        if !matches!(&*main_window.backend.borrow(), SearchBackend::Daemon(_)) {
            let backend_for_auto = main_window.backend.clone();
            let debounce_for_auto = main_window.debounce_source.clone();
            gtk4::glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
                // Skip widget refresh while the user is actively typing.
                if debounce_for_auto.borrow().is_some() {
                    return gtk4::glib::ControlFlow::Continue;
                }
                backend_for_auto.borrow_mut().refresh_widgets();
                gtk4::glib::ControlFlow::Continue
            });
        }

        main_window
    }

    fn create_filter_buttons(
        container: &GtkBox,
        filter: &Rc<RefCell<ProviderFilter>>,
        tabs: &[String],
        descs: &HashMap<String, ProviderDesc>,
    ) -> HashMap<ProviderType, ToggleButton> {
        let mut buttons = HashMap::new();

        // Parse tab strings to ProviderType and create buttons
        for (idx, tab_str) in tabs.iter().enumerate() {
            let provider_type: ProviderType = match tab_str.parse() {
                Ok(pt) => pt,
                Err(e) => {
                    log::warn!("Invalid tab config '{}': {}", tab_str, e);
                    continue;
                }
            };

            let meta = Self::provider_meta(&provider_type, descs);
            // Show number hint in the label for first 9 tabs (using superscript)
            let label = if idx < 9 {
                let superscript = match idx + 1 {
                    1 => "¹",
                    2 => "²",
                    3 => "³",
                    4 => "⁴",
                    5 => "⁵",
                    6 => "⁶",
                    7 => "⁷",
                    8 => "⁸",
                    9 => "⁹",
                    _ => "",
                };
                format!("{}{}", meta.tab_label, superscript)
            } else {
                meta.tab_label.clone()
            };
            let shortcut = format!("Ctrl+{}", idx + 1);

            let button = ToggleButton::builder()
                .label(&label)
                .tooltip_text(&shortcut)
                .active(filter.borrow().is_enabled(provider_type.clone()))
                .build();

            button.add_css_class("owlry-filter-button");
            button.add_css_class(&meta.css_class);

            container.append(&button);
            buttons.insert(provider_type, button);
        }

        buttons
    }

    /// Get the mode display name, resolving plugin names via IPC metadata.
    fn mode_display_name(filter: &ProviderFilter, descs: &HashMap<String, ProviderDesc>) -> String {
        let base = filter.mode_display_name();
        // "Plugin" is the generic fallback — resolve it to the actual name
        if base == "Plugin" {
            let enabled = filter.enabled_providers();
            if enabled.len() == 1
                && let ProviderType::Plugin(_) = &enabled[0]
            {
                return Self::provider_meta(&enabled[0], descs).tab_label;
            }
        }
        base.to_string()
    }

    /// Resolve display metadata for a provider, checking IPC data first.
    fn provider_meta(
        provider: &ProviderType,
        descs: &HashMap<String, ProviderDesc>,
    ) -> provider_meta::ProviderMeta {
        let type_id = provider.to_string();
        let desc = descs.get(&type_id);
        provider_meta::resolve(provider, desc)
    }

    fn build_placeholder(filter: &ProviderFilter, descs: &HashMap<String, ProviderDesc>) -> String {
        let active: Vec<String> = filter
            .enabled_providers()
            .iter()
            .map(|p| Self::provider_meta(p, descs).search_noun)
            .collect();

        format!("Search {}...", active.join(", "))
    }

    /// Build hints string for the status bar based on enabled built-in providers.
    /// Plugin trigger hints (? web, / files, etc.) are not included here since
    /// plugin availability is not tracked in ProvidersConfig.
    fn build_hints(config: &crate::config::ProvidersConfig) -> String {
        let mut parts: Vec<String> = vec![
            "Tab: cycle".to_string(),
            "↑↓: nav".to_string(),
            "Enter: launch".to_string(),
            "→: actions".to_string(),
            "Esc: close".to_string(),
        ];

        if config.calculator {
            parts.push("= calc".to_string());
        }
        if config.converter {
            parts.push("> conv".to_string());
        }
        if config.power {
            parts.push(":power".to_string());
        }

        parts.join("  ")
    }
}

impl std::ops::Deref for MainWindow {
    type Target = ApplicationWindow;

    fn deref(&self) -> &Self::Target {
        &self.window
    }
}
