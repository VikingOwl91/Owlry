use crate::backend::SearchBackend;
use crate::config::Config;
use crate::filter::ProviderFilter;
use crate::ipc::ProviderDesc;
use crate::providers::{ItemSource, LaunchItem, ProviderType};
use crate::ui::ResultRow;
use crate::ui::provider_meta;
use crate::ui::submenu;
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Entry, EventControllerKey, Label, ListBox,
    ListBoxRow, Orientation, ScrolledWindow, SelectionMode, ToggleButton,
};
use log::info;

#[cfg(feature = "dev-logging")]
use log::debug;

use std::cell::RefCell;
use std::collections::HashMap;
use std::process::Command;
use std::rc::Rc;

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
}

/// State for lazy loading results
#[derive(Default)]
struct LazyLoadState {
    /// All matching results (may be more than displayed)
    all_results: Vec<LaunchItem>,
    /// Number of items currently displayed
    displayed_count: usize,
    /// The query that produced these results (for highlighting in lazy-loaded batches)
    query: String,
}

/// Number of items to display initially and per batch
const INITIAL_RESULTS: usize = 15;
const LOAD_MORE_BATCH: usize = 10;
/// Debounce delay for search input (milliseconds)
const SEARCH_DEBOUNCE_MS: u64 = 50;

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

    /// Scroll the given row into view within the scrolled window
    fn scroll_to_row(
        scrolled: &ScrolledWindow,
        results_list: &ListBox,
        row: &ListBoxRow,
        lazy_state: &Rc<RefCell<LazyLoadState>>,
    ) {
        let vadj = scrolled.vadjustment();

        let row_index = row.index();
        if row_index < 0 {
            return;
        }

        let visible_height = vadj.page_size();
        let current_scroll = vadj.value();

        let list_height = results_list.height() as f64;
        let row_count = lazy_state.borrow().displayed_count.max(1) as f64;

        let row_height = list_height / row_count;
        let row_top = row_index as f64 * row_height;
        let row_bottom = row_top + row_height;

        if row_top < current_scroll {
            vadj.set_value(row_top);
        } else if row_bottom > current_scroll + visible_height {
            vadj.set_value(row_bottom - visible_height);
        }
    }

    /// Enter submenu mode for an item with actions
    #[allow(clippy::too_many_arguments)]
    fn enter_submenu(
        submenu_state: &Rc<RefCell<SubmenuState>>,
        results_list: &ListBox,
        current_results: &Rc<RefCell<Vec<LaunchItem>>>,
        mode_label: &Label,
        hints_label: &Label,
        search_entry: &Entry,
        display_name: String,
        actions: Vec<LaunchItem>,
    ) {
        #[cfg(feature = "dev-logging")]
        debug!(
            "[UI] Entering submenu: {} ({} actions)",
            display_name,
            actions.len()
        );

        // Save current state
        {
            let mut state = submenu_state.borrow_mut();
            state.active = true;
            state.service_name = String::new(); // No longer specific to services
            state.display_name = display_name.clone();
            state.items = actions.clone();
            state.saved_search = search_entry.text().to_string();
        }

        // Update UI
        mode_label.set_label(&format!("← {}", display_name));
        hints_label.set_label("Enter: execute  Esc/Backspace: back  ↑↓: navigate");
        search_entry.set_text("");
        search_entry.set_placeholder_text(Some(&format!("Filter {} actions...", display_name)));

        // Display actions
        results_list.remove_all();

        for item in &actions {
            let row = ResultRow::new(item, "");
            results_list.append(&row);
        }

        if let Some(first_row) = results_list.row_at_index(0) {
            results_list.select_row(Some(&first_row));
        }

        *current_results.borrow_mut() = actions;
    }

    /// Exit submenu mode
    fn exit_submenu(
        submenu_state: &Rc<RefCell<SubmenuState>>,
        mode_label: &Label,
        hints_label: &Label,
        search_entry: &Entry,
        filter: &Rc<RefCell<ProviderFilter>>,
        config: &Rc<RefCell<Config>>,
        descs: &HashMap<String, ProviderDesc>,
    ) {
        #[cfg(feature = "dev-logging")]
        debug!("[UI] Exiting submenu");

        let saved_search = {
            let mut state = submenu_state.borrow_mut();
            state.active = false;
            state.items.clear();
            state.saved_search.clone()
        };

        // Restore UI
        mode_label.set_label(&Self::mode_display_name(&filter.borrow(), descs));
        hints_label.set_label(&Self::build_hints(&config.borrow().providers));
        search_entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow(), descs)));
        search_entry.set_text(&saved_search);

        // Trigger refresh by emitting changed signal
        search_entry.emit_by_name::<()>("changed", &[]);
    }

    fn setup_signals(&self) {
        // Search input handling with prefix detection and debouncing
        let backend = self.backend.clone();
        let results_list = self.results_list.clone();
        let config = self.config.clone();
        let current_results = self.current_results.clone();
        let filter = self.filter.clone();
        let mode_label = self.mode_label.clone();
        let search_entry_for_change = self.search_entry.clone();
        let submenu_state = self.submenu_state.clone();
        let lazy_state = self.lazy_state.clone();
        let debounce_source = self.debounce_source.clone();
        let descs = self.provider_descs.clone();

        self.search_entry.connect_changed(move |entry| {
            let raw_query = entry.text();

            // If in submenu, filter immediately (no debounce needed for small local lists)
            if submenu_state.borrow().active {
                let state = submenu_state.borrow();
                let query = raw_query.to_lowercase();

                let filtered: Vec<LaunchItem> = state
                    .items
                    .iter()
                    .filter(|item| {
                        query.is_empty()
                            || item.name.to_lowercase().contains(&query)
                            || item
                                .description
                                .as_ref()
                                .map(|d| d.to_lowercase().contains(&query))
                                .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                // Clear and repopulate
                results_list.remove_all();

                for item in &filtered {
                    let row = ResultRow::new(item, "");
                    results_list.append(&row);
                }

                if let Some(first_row) = results_list.row_at_index(0) {
                    results_list.select_row(Some(&first_row));
                }

                *current_results.borrow_mut() = filtered;
                return;
            }

            // Normal mode: update prefix/UI immediately for responsiveness
            let parsed = ProviderFilter::parse_query(&raw_query);

            {
                let mut f = filter.borrow_mut();
                f.set_prefix(parsed.prefix.clone());
            }

            mode_label.set_label(&Self::mode_display_name(&filter.borrow(), &descs));

            if let Some(ref prefix) = parsed.prefix {
                let prefix_name = match prefix {
                    ProviderType::Application => "applications",
                    ProviderType::Command => "commands",
                    ProviderType::Dmenu => "options",
                    ProviderType::Plugin(type_id) => match type_id.as_str() {
                        "bookmarks" => "bookmarks",
                        "calc" => "calculator",
                        "clipboard" => "clipboard",
                        "emoji" => "emoji",
                        "filesearch" => "files",
                        "media" => "media",
                        "pomodoro" => "pomodoro",
                        "scripts" => "scripts",
                        "ssh" => "SSH hosts",
                        "power" | "system" => "power actions",
                        "uuctl" => "uuctl units",
                        "weather" => "weather",
                        "websearch" => "web",
                        _ => "plugins",
                    },
                };
                search_entry_for_change
                    .set_placeholder_text(Some(&format!("Search {}...", prefix_name)));
            }

            // Cancel any pending debounced search
            if let Some(source_id) = debounce_source.borrow_mut().take() {
                source_id.remove();
            }

            // Clone references for the debounced closure
            let backend = backend.clone();
            let results_list = results_list.clone();
            let config = config.clone();
            let current_results = current_results.clone();
            let filter = filter.clone();
            let lazy_state = lazy_state.clone();
            let debounce_source_for_closure = debounce_source.clone();
            let query_str = parsed.query.clone();
            let tag = parsed.tag_filter.clone();
            // Capture the raw entry text at dispatch time for staleness detection.
            let raw_text_at_dispatch = entry.text().to_string();
            let search_entry_for_stale = search_entry_for_change.clone();

            // Schedule debounced search
            let source_id = gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS),
                move || {
                    // Clear the source ID since we're now executing
                    *debounce_source_for_closure.borrow_mut() = None;

                    // Search latency timing (keystroke path). The 50ms debounce
                    // precedes this closure; `dispatch_t` measures from query
                    // dispatch through IPC return to painted rows.
                    let dispatch_t = std::time::Instant::now();

                    let cfg = config.borrow();
                    let max_results = cfg.general.max_results;
                    drop(cfg);

                    // Try async path (daemon mode)
                    let receiver = {
                        let be = backend.borrow();
                        let f = filter.borrow();
                        let c = config.borrow();
                        be.query_async(&query_str, max_results, &f, &c, tag.as_deref())
                    };

                    if let Some(rx) = receiver {
                        // Daemon mode: results arrive asynchronously on the main loop.
                        // spawn_future_local runs the async block on the GTK main
                        // thread, so non-Send types (Rc, GTK widgets) are fine.
                        let results_list_cb = results_list.clone();
                        let current_results_cb = current_results.clone();
                        let lazy_state_cb = lazy_state.clone();
                        let query_for_highlight = query_str.clone();

                        gtk4::glib::spawn_future_local(async move {
                            if let Ok(result) = rx.await {
                                log::debug!(
                                    "search: dispatch→IPC return +{:.1?} ({} items)",
                                    dispatch_t.elapsed(),
                                    result.items.len()
                                );
                                // Discard stale results: the user has typed something new
                                // since this query was dispatched.
                                if search_entry_for_stale.text().as_str() != raw_text_at_dispatch {
                                    return;
                                }
                                let paint_t = std::time::Instant::now();
                                results_list_cb.remove_all();

                                let items = result.items;
                                let initial_count = INITIAL_RESULTS.min(items.len());

                                for item in items.iter().take(initial_count) {
                                    let row = ResultRow::new(item, &query_for_highlight);
                                    results_list_cb.append(&row);
                                }

                                if let Some(first_row) = results_list_cb.row_at_index(0) {
                                    results_list_cb.select_row(Some(&first_row));
                                }

                                *current_results_cb.borrow_mut() = items[..initial_count].to_vec();
                                let mut lazy = lazy_state_cb.borrow_mut();
                                lazy.all_results = items;
                                lazy.displayed_count = initial_count;
                                lazy.query = query_for_highlight;
                                log::debug!(
                                    "search: paint {} rows +{:.1?} | total dispatch→paint +{:.1?}",
                                    initial_count,
                                    paint_t.elapsed(),
                                    dispatch_t.elapsed()
                                );
                            }
                        });
                    } else {
                        // Local mode (dmenu): synchronous search
                        let results = backend.borrow_mut().search_with_tag(
                            &query_str,
                            max_results,
                            &filter.borrow(),
                            &config.borrow(),
                            tag.as_deref(),
                        );

                        results_list.remove_all();

                        let initial_count = INITIAL_RESULTS.min(results.len());

                        for item in results.iter().take(initial_count) {
                            let row = ResultRow::new(item, &query_str);
                            results_list.append(&row);
                        }

                        if let Some(first_row) = results_list.row_at_index(0) {
                            results_list.select_row(Some(&first_row));
                        }

                        *current_results.borrow_mut() = results[..initial_count].to_vec();
                        let mut lazy = lazy_state.borrow_mut();
                        lazy.all_results = results;
                        lazy.query = query_str;
                        lazy.displayed_count = initial_count;
                    }
                },
            );

            *debounce_source.borrow_mut() = Some(source_id);
        });

        // Entry activate signal (Enter key in search entry)
        let results_list_for_activate = self.results_list.clone();
        let current_results_for_activate = self.current_results.clone();
        let config_for_activate = self.config.clone();
        let backend_for_activate = self.backend.clone();
        let window_for_activate = self.window.clone();
        let submenu_state_for_activate = self.submenu_state.clone();
        let mode_label_for_activate = self.mode_label.clone();
        let hints_label_for_activate = self.hints_label.clone();
        let search_entry_for_activate = self.search_entry.clone();
        let is_dmenu_mode_for_activate = self.is_dmenu_mode;

        self.search_entry.connect_activate(move |entry| {
            let selected = results_list_for_activate
                .selected_row()
                .or_else(|| results_list_for_activate.row_at_index(0));

            // Handle the case where we have a selected item
            if let Some(row) = selected {
                let index = row.index() as usize;
                let results = current_results_for_activate.borrow();
                if let Some(item) = results.get(index) {
                    // Check if this is a submenu item and query the plugin for actions
                    let submenu_result = if submenu::is_submenu_item(item) {
                        if let Some((plugin_id, data)) =
                            submenu::parse_submenu_command(&item.command)
                        {
                            // Clone values before dropping borrow
                            let plugin_id = plugin_id.to_string();
                            let data = data.to_string();
                            let display_name = item.name.clone();
                            drop(results); // Release borrow before querying
                            backend_for_activate.borrow_mut().query_submenu_actions(
                                &plugin_id,
                                &data,
                                &display_name,
                            )
                        } else {
                            drop(results);
                            None
                        }
                    } else {
                        None
                    };

                    if let Some((display_name, actions)) = submenu_result {
                        Self::enter_submenu(
                            &submenu_state_for_activate,
                            &results_list_for_activate,
                            &current_results_for_activate,
                            &mode_label_for_activate,
                            &hints_label_for_activate,
                            &search_entry_for_activate,
                            display_name,
                            actions,
                        );
                    } else {
                        // Not a submenu item - execute the command
                        let results = current_results_for_activate.borrow();
                        if let Some(item) = results.get(index).cloned() {
                            drop(results);
                            let should_close = Self::handle_item_action(
                                &item,
                                &config_for_activate.borrow(),
                                &backend_for_activate,
                            );
                            if should_close {
                                // In dmenu mode, exit with success code
                                if is_dmenu_mode_for_activate {
                                    std::process::exit(0);
                                }
                                window_for_activate.close();
                            } else {
                                // Trigger search refresh for updated widget state
                                entry.emit_by_name::<()>("changed", &[]);
                            }
                        }
                    }
                    return;
                }
            }

            // No item selected/matched - in dmenu mode, output the typed text
            if is_dmenu_mode_for_activate {
                let text = entry.text();
                if !text.is_empty() {
                    println!("{}", text);
                    std::process::exit(0);
                }
            }
        });

        // Filter button signals
        for (provider_type, button) in self.filter_buttons.borrow().iter() {
            let filter = self.filter.clone();
            let search_entry = self.search_entry.clone();
            let mode_label = self.mode_label.clone();
            let ptype = provider_type.clone();
            let descs = self.provider_descs.clone();

            button.connect_toggled(move |btn| {
                {
                    let mut f = filter.borrow_mut();
                    if btn.is_active() {
                        f.enable(ptype.clone());
                    } else {
                        f.disable(ptype.clone());
                    }
                }
                mode_label.set_label(&Self::mode_display_name(&filter.borrow(), &descs));
                search_entry
                    .set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow(), &descs)));
                search_entry.emit_by_name::<()>("changed", &[]);
            });
        }

        // Keyboard navigation
        let key_controller = EventControllerKey::new();
        let window = self.window.clone();
        let results_list = self.results_list.clone();
        let scrolled = self.scrolled.clone();
        let search_entry = self.search_entry.clone();
        let _current_results = self.current_results.clone();
        let config = self.config.clone();
        let filter = self.filter.clone();
        let filter_buttons = self.filter_buttons.clone();
        let mode_label = self.mode_label.clone();
        let hints_label = self.hints_label.clone();
        let submenu_state = self.submenu_state.clone();
        let tab_order = self.tab_order.clone();
        let is_dmenu_mode = self.is_dmenu_mode;
        let lazy_state_for_keys = self.lazy_state.clone();
        let provider_descs = self.provider_descs.clone();

        key_controller.connect_key_pressed(move |_, key, _, modifiers| {
            let ctrl = modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(gtk4::gdk::ModifierType::SHIFT_MASK);

            #[cfg(feature = "dev-logging")]
            debug!(
                "[UI] Key pressed: {:?} (ctrl={}, shift={})",
                key, ctrl, shift
            );

            match key {
                Key::Escape => {
                    // If in submenu, exit submenu first
                    if submenu_state.borrow().active {
                        Self::exit_submenu(
                            &submenu_state,
                            &mode_label,
                            &hints_label,
                            &search_entry,
                            &filter,
                            &config,
                            &provider_descs,
                        );
                        gtk4::glib::Propagation::Stop
                    } else {
                        // In dmenu mode, exit with cancel code (1)
                        if is_dmenu_mode {
                            std::process::exit(1);
                        }
                        window.close();
                        gtk4::glib::Propagation::Stop
                    }
                }
                Key::BackSpace if search_entry.text().is_empty() => {
                    // If in submenu with empty search, exit submenu
                    if submenu_state.borrow().active {
                        Self::exit_submenu(
                            &submenu_state,
                            &mode_label,
                            &hints_label,
                            &search_entry,
                            &filter,
                            &config,
                            &provider_descs,
                        );
                        gtk4::glib::Propagation::Stop
                    } else {
                        // In dmenu mode, exit with cancel code (1)
                        if is_dmenu_mode {
                            std::process::exit(1);
                        }
                        window.close();
                        gtk4::glib::Propagation::Stop
                    }
                }
                Key::Down => {
                    let selected = results_list
                        .selected_row()
                        .or_else(|| results_list.row_at_index(0));

                    if let Some(current) = selected {
                        let next_index = current.index() + 1;
                        if let Some(next_row) = results_list.row_at_index(next_index) {
                            results_list.select_row(Some(&next_row));
                            Self::scroll_to_row(
                                &scrolled,
                                &results_list,
                                &next_row,
                                &lazy_state_for_keys,
                            );
                        }
                    }
                    gtk4::glib::Propagation::Stop
                }
                Key::Up => {
                    if let Some(selected) = results_list.selected_row() {
                        let prev_index = selected.index() - 1;
                        if prev_index >= 0
                            && let Some(prev_row) = results_list.row_at_index(prev_index)
                        {
                            results_list.select_row(Some(&prev_row));
                            Self::scroll_to_row(
                                &scrolled,
                                &results_list,
                                &prev_row,
                                &lazy_state_for_keys,
                            );
                        }
                    }
                    gtk4::glib::Propagation::Stop
                }
                // Tab cycles through filter modes (only when not in submenu)
                Key::Tab if !ctrl => {
                    if !submenu_state.borrow().active {
                        Self::cycle_filter_mode(
                            &filter,
                            &filter_buttons,
                            &search_entry,
                            &mode_label,
                            &tab_order,
                            !shift,
                            &provider_descs,
                        );
                    }
                    gtk4::glib::Propagation::Stop
                }
                Key::ISO_Left_Tab => {
                    if !submenu_state.borrow().active {
                        Self::cycle_filter_mode(
                            &filter,
                            &filter_buttons,
                            &search_entry,
                            &mode_label,
                            &tab_order,
                            false,
                            &provider_descs,
                        );
                    }
                    gtk4::glib::Propagation::Stop
                }
                // Ctrl+1-9 toggle specific providers based on tab order (only when not in submenu)
                Key::_1
                | Key::_2
                | Key::_3
                | Key::_4
                | Key::_5
                | Key::_6
                | Key::_7
                | Key::_8
                | Key::_9
                    if ctrl =>
                {
                    info!("[UI] Ctrl+number detected: {:?}", key);
                    if !submenu_state.borrow().active {
                        let idx = match key {
                            Key::_1 => 0,
                            Key::_2 => 1,
                            Key::_3 => 2,
                            Key::_4 => 3,
                            Key::_5 => 4,
                            Key::_6 => 5,
                            Key::_7 => 6,
                            Key::_8 => 7,
                            Key::_9 => 8,
                            _ => return gtk4::glib::Propagation::Proceed,
                        };
                        info!("[UI] Toggling tab at index {}", idx);
                        if let Some(provider) = tab_order.get(idx) {
                            info!("[UI] Found provider: {:?}", provider);
                            Self::toggle_provider_button(
                                provider.clone(),
                                &filter,
                                &filter_buttons,
                                &search_entry,
                                &mode_label,
                                &provider_descs,
                            );
                        } else {
                            info!(
                                "[UI] No provider at index {}, tab_order len={}",
                                idx,
                                tab_order.len()
                            );
                        }
                    }
                    gtk4::glib::Propagation::Stop
                }
                _ => gtk4::glib::Propagation::Proceed,
            }
        });

        self.window.add_controller(key_controller);

        // Double-click to launch
        let current_results = self.current_results.clone();
        let config = self.config.clone();
        let backend = self.backend.clone();
        let window = self.window.clone();
        let submenu_state = self.submenu_state.clone();
        let results_list_for_click = self.results_list.clone();
        let mode_label = self.mode_label.clone();
        let hints_label = self.hints_label.clone();
        let search_entry = self.search_entry.clone();

        self.results_list.connect_row_activated(move |_list, row| {
            let index = row.index() as usize;
            let results = current_results.borrow();
            if let Some(item) = results.get(index) {
                // Check if this is a submenu item and query the plugin for actions
                let submenu_result = if submenu::is_submenu_item(item) {
                    if let Some((plugin_id, data)) = submenu::parse_submenu_command(&item.command) {
                        // Clone values before dropping borrow
                        let plugin_id = plugin_id.to_string();
                        let data = data.to_string();
                        let display_name = item.name.clone();
                        drop(results);
                        backend
                            .borrow_mut()
                            .query_submenu_actions(&plugin_id, &data, &display_name)
                    } else {
                        drop(results);
                        None
                    }
                } else {
                    None
                };

                if let Some((display_name, actions)) = submenu_result {
                    Self::enter_submenu(
                        &submenu_state,
                        &results_list_for_click,
                        &current_results,
                        &mode_label,
                        &hints_label,
                        &search_entry,
                        display_name,
                        actions,
                    );
                } else {
                    // Not a submenu item - execute the command
                    let results = current_results.borrow();
                    if let Some(item) = results.get(index).cloned() {
                        drop(results);
                        let should_close =
                            Self::handle_item_action(&item, &config.borrow(), &backend);
                        if should_close {
                            window.close();
                        } else {
                            // Trigger search refresh for updated widget state
                            search_entry.emit_by_name::<()>("changed", &[]);
                        }
                    }
                }
            }
        });
    }

    fn cycle_filter_mode(
        filter: &Rc<RefCell<ProviderFilter>>,
        buttons: &Rc<RefCell<HashMap<ProviderType, ToggleButton>>>,
        entry: &Entry,
        mode_label: &Label,
        tab_order: &[ProviderType],
        forward: bool,
        descs: &HashMap<String, ProviderDesc>,
    ) {
        if tab_order.is_empty() {
            return;
        }

        let current = filter.borrow().enabled_providers();
        let all_enabled = current.len() == tab_order.len();

        // Cycle: All -> Provider1 -> Provider2 -> ... -> ProviderN -> All
        // In "All" mode (all providers enabled), we go to first provider (forward) or last (backward)
        // In single-provider mode, we go to next provider or back to All at the boundary
        if all_enabled {
            // Currently showing all, go to first (forward) or last (backward) single provider
            let next = if forward {
                tab_order[0].clone()
            } else {
                tab_order[tab_order.len() - 1].clone()
            };
            {
                let mut f = filter.borrow_mut();
                f.set_single_mode(next.clone());
            }
            for (ptype, button) in buttons.borrow().iter() {
                button.set_active(ptype == &next);
            }
        } else if current.len() == 1 {
            let idx = tab_order.iter().position(|p| p == &current[0]).unwrap_or(0);
            let at_boundary = if forward {
                idx == tab_order.len() - 1
            } else {
                idx == 0
            };

            if at_boundary {
                // At boundary, go back to "All" mode
                {
                    let mut f = filter.borrow_mut();
                    for provider in tab_order {
                        f.enable(provider.clone());
                    }
                    f.restore_all_mode();
                }
                for (_, button) in buttons.borrow().iter() {
                    button.set_active(true);
                }
            } else {
                // Move to next/previous provider
                let next = if forward {
                    tab_order[idx + 1].clone()
                } else {
                    tab_order[idx - 1].clone()
                };
                {
                    let mut f = filter.borrow_mut();
                    f.set_single_mode(next.clone());
                }
                for (ptype, button) in buttons.borrow().iter() {
                    button.set_active(ptype == &next);
                }
            }
        } else {
            // Some but not all providers enabled - go to first provider
            let next = tab_order[0].clone();
            {
                let mut f = filter.borrow_mut();
                f.set_single_mode(next.clone());
            }
            for (ptype, button) in buttons.borrow().iter() {
                button.set_active(ptype == &next);
            }
        }

        mode_label.set_label(&Self::mode_display_name(&filter.borrow(), descs));
        entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow(), descs)));
        entry.emit_by_name::<()>("changed", &[]);
    }

    fn toggle_provider_button(
        provider: ProviderType,
        filter: &Rc<RefCell<ProviderFilter>>,
        buttons: &Rc<RefCell<HashMap<ProviderType, ToggleButton>>>,
        entry: &Entry,
        mode_label: &Label,
        descs: &HashMap<String, ProviderDesc>,
    ) {
        {
            let mut f = filter.borrow_mut();
            f.toggle(provider.clone());
        }

        if let Some(button) = buttons.borrow().get(&provider) {
            button.set_active(filter.borrow().is_enabled(provider));
        }

        mode_label.set_label(&Self::mode_display_name(&filter.borrow(), descs));
        entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow(), descs)));
        entry.emit_by_name::<()>("changed", &[]);
    }

    /// Schedule initial results population via idle callback.
    /// Call this AFTER `window.present()` so the window appears immediately.
    pub fn schedule_initial_results(&self) {
        let backend = self.backend.clone();
        let results_list = self.results_list.clone();
        let config = self.config.clone();
        let filter = self.filter.clone();
        let current_results = self.current_results.clone();
        let lazy_state = self.lazy_state.clone();

        let scheduled_at = std::time::Instant::now();
        gtk4::glib::idle_add_local_once(move || {
            log::debug!(
                "startup: initial-results idle fired +{:.1?} after schedule",
                scheduled_at.elapsed()
            );
            let cfg = config.borrow();
            let max_results = cfg.general.max_results;
            drop(cfg);

            let query_start = std::time::Instant::now();
            let results =
                backend
                    .borrow_mut()
                    .search("", max_results, &filter.borrow(), &config.borrow());
            log::debug!(
                "startup: initial daemon query returned {} items in {:.1?}",
                results.len(),
                query_start.elapsed()
            );

            // Clear existing results
            results_list.remove_all();

            let initial_count = INITIAL_RESULTS.min(results.len());

            for item in results.iter().take(initial_count) {
                let row = ResultRow::new(item, "");
                results_list.append(&row);
            }

            if let Some(first_row) = results_list.row_at_index(0) {
                results_list.select_row(Some(&first_row));
            }

            *current_results.borrow_mut() = results[..initial_count].to_vec();
            let mut lazy = lazy_state.borrow_mut();
            lazy.all_results = results;
            lazy.displayed_count = initial_count;
        });
    }

    /// Set up lazy loading scroll detection
    fn setup_lazy_loading(&self) {
        let vadj = self.scrolled.vadjustment();
        let results_list = self.results_list.clone();
        let lazy_state = self.lazy_state.clone();
        let current_results = self.current_results.clone();

        // Load more on scroll
        vadj.connect_value_changed(move |adj| {
            let value = adj.value();
            let upper = adj.upper();
            let page_size = adj.page_size();

            // Load more when near bottom (within 50px)
            let near_bottom = upper > page_size && (value + page_size >= upper - 50.0);
            if near_bottom {
                Self::load_more_items(&lazy_state, &results_list, &current_results);
            }
        });

        // Also load more when selecting rows near the end (keyboard navigation)
        let lazy_state2 = self.lazy_state.clone();
        let results_list2 = self.results_list.clone();
        let current_results2 = self.current_results.clone();

        self.results_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let index = row.index();
                let lazy = lazy_state2.borrow();
                let displayed = lazy.displayed_count;
                let all_count = lazy.all_results.len();
                drop(lazy);

                // Load more if within 3 items of the end
                if displayed < all_count && (index as usize) >= displayed.saturating_sub(3) {
                    Self::load_more_items(&lazy_state2, &results_list2, &current_results2);
                }
            }
        });
    }

    /// Load more items from lazy state
    fn load_more_items(
        lazy_state: &Rc<RefCell<LazyLoadState>>,
        results_list: &ListBox,
        current_results: &Rc<RefCell<Vec<LaunchItem>>>,
    ) {
        let mut lazy = lazy_state.borrow_mut();
        let all_count = lazy.all_results.len();
        let displayed = lazy.displayed_count;

        if displayed < all_count {
            // Load next batch
            let new_end = (displayed + LOAD_MORE_BATCH).min(all_count);
            let query = lazy.query.clone();
            for item in lazy.all_results[displayed..new_end].iter() {
                let row = ResultRow::new(item, &query);
                results_list.append(&row);
            }
            lazy.displayed_count = new_end;

            // Update current_results
            let mut current = current_results.borrow_mut();
            current.extend(lazy.all_results[displayed..new_end].iter().cloned());
        }
    }

    /// Handle item activation - returns true if window should close
    fn handle_item_action(
        item: &LaunchItem,
        config: &Config,
        backend: &Rc<RefCell<SearchBackend>>,
    ) -> bool {
        // Check for plugin internal commands (format: PLUGIN_ID:action)
        // These are handled by the plugin itself, not launched as shell commands
        if backend.borrow_mut().execute_plugin_action(&item.command) {
            // Plugin handled the action - don't close window
            // User might want to see updated state (e.g., pomodoro timer)
            return false;
        }

        // Regular item launch
        Self::launch_item(item, config, backend);
        true
    }

    fn launch_item(item: &LaunchItem, config: &Config, backend: &Rc<RefCell<SearchBackend>>) {
        // dmenu mode: print selection to stdout instead of executing
        if matches!(item.provider, ProviderType::Dmenu) {
            println!("{}", item.name);
            return;
        }

        // Record this launch for frecency tracking (via backend)
        if config.providers.frecency {
            let provider_str = item.provider.to_string();
            backend.borrow_mut().record_launch(&item.id, &provider_str);
            #[cfg(feature = "dev-logging")]
            debug!("[UI] Recorded frecency launch for: {}", item.id);
        }

        info!("Launching: {} ({})", item.name, item.command);

        #[cfg(feature = "dev-logging")]
        debug!(
            "[UI] Launch details: terminal={}, provider={:?}, id={}",
            item.terminal, item.provider, item.id
        );

        // Reject script plugin commands that don't match the known-safe allowlist.
        // Script plugins (Lua/Rune user plugins) are untrusted code; only allow
        // patterns that can't escalate privileges or exfiltrate data.
        if item.source == ItemSource::ScriptPlugin {
            let cmd = &item.command;
            let allowed = cmd.is_empty()
                || cmd.starts_with("xdg-open ")
                || cmd.starts_with("wl-copy")
                || cmd.starts_with("wl-paste")
                || cmd.starts_with("SUBMENU:")
                || cmd.starts_with('!');
            if !allowed {
                let msg = format!(
                    "Blocked untrusted script plugin command from '{}': {}",
                    item.name, cmd
                );
                log::warn!("{}", msg);
                crate::notify::notify("Command blocked", &msg);
                return;
            }
        }

        // Reject items with no command — nothing to execute.
        if item.command.is_empty() && !matches!(item.provider, ProviderType::Application) {
            let msg = format!("Item '{}' has no command; cannot launch", item.name);
            log::warn!("{}", msg);
            crate::notify::notify("Launch failed", &msg);
            return;
        }

        // Check if this is a desktop application (has .desktop file as ID)
        let is_desktop_app =
            matches!(item.provider, ProviderType::Application) && item.id.ends_with(".desktop");

        // Desktop files should be launched via proper launchers that implement the
        // freedesktop Desktop Entry spec (D-Bus activation, field codes, env vars, etc.)
        // We delegate to: uwsm (if configured), gio launch, or gtk-launch as fallback.
        //
        // Non-desktop items (commands, plugins) use sh -c for shell execution.
        // File-search results resolve their default handler first so that
        // terminal-based handlers (nvim, vim, …) open in a real terminal
        // instead of being launched windowless via xdg-open, and so that
        // files with no associated application surface a notification.
        let result = if let Some(path) = Self::filesearch_open_path(item) {
            Self::launch_file_open(&path, config)
        } else if is_desktop_app {
            Self::launch_desktop_file(&item.id, config)
        } else {
            Self::launch_command(&item.command, item.terminal, config)
        };

        if let Err(e) = result {
            let msg = format!("Failed to launch '{}': {}", item.name, e);
            log::error!("{}", msg);
            crate::notify::notify("Launch failed", &msg);
        }
    }

    /// Launch a .desktop file.
    ///
    /// When `use_uwsm` is enabled in config, launches via `uwsm app -- <file>`
    /// which starts the app in a proper systemd user session.
    ///
    /// Otherwise, uses `gio launch` which is always available (part of glib2/GTK4)
    /// and handles D-Bus activation, field codes, Terminal flag, etc.
    fn launch_desktop_file(
        desktop_path: &str,
        config: &Config,
    ) -> std::io::Result<std::process::Child> {
        use std::path::Path;

        // Check if desktop file exists
        if !Path::new(desktop_path).exists() {
            let msg = format!("Desktop file not found: {}", desktop_path);
            log::error!("{}", msg);
            crate::notify::notify("Launch failed", &msg);
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, msg));
        }

        if config.general.use_uwsm {
            // Check if uwsm is available
            let uwsm_available = Command::new("which")
                .arg("uwsm")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if !uwsm_available {
                let msg = "uwsm is enabled in config but not installed";
                log::error!("{}", msg);
                crate::notify::notify("Launch failed", msg);
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, msg));
            }

            info!("Launching via uwsm: {}", desktop_path);
            Command::new("uwsm")
                .args(["app", "--", desktop_path])
                .spawn()
        } else {
            info!("Launching via gio: {}", desktop_path);
            Command::new("gio").args(["launch", desktop_path]).spawn()
        }
    }

    /// Launch a shell command (for non-desktop items like PATH commands, plugins, etc.)
    fn launch_command(
        command: &str,
        terminal: bool,
        config: &Config,
    ) -> std::io::Result<std::process::Child> {
        let cmd = if terminal {
            let terminal_cmd = config
                .general
                .terminal_command
                .as_deref()
                .unwrap_or("xterm");
            format!("{} -e {}", terminal_cmd, command)
        } else {
            command.to_string()
        };

        // Shell/system commands run directly without uwsm wrapper
        // (they're typically short-lived or system utilities)
        let is_system_command = cmd.starts_with("playerctl ")
            || cmd.starts_with("dbus-send ")
            || cmd.starts_with("systemctl ")
            || cmd.starts_with("journalctl ")
            || cmd.contains(" | ")
            || cmd.contains(" && ")
            || cmd.contains(" || ")
            || cmd.contains(" > ")
            || cmd.contains(" < ");

        // Use uwsm for regular commands if enabled (and not a system command)
        if config.general.use_uwsm && !is_system_command {
            info!("Launching command via uwsm: {}", cmd);
            Command::new("uwsm")
                .args(["app", "--", "sh", "-c", &cmd])
                .spawn()
        } else {
            Command::new("sh").arg("-c").arg(&cmd).spawn()
        }
    }

    /// If `item` is a file-search result, return the absolute path it points
    /// at (decoded from its `file:<path>` id). Returns `None` for every other
    /// provider, so callers fall through to the generic launch path.
    fn filesearch_open_path(item: &LaunchItem) -> Option<String> {
        match &item.provider {
            ProviderType::Plugin(id) if id == "filesearch" => {
                item.id.strip_prefix("file:").map(str::to_string)
            }
            _ => None,
        }
    }

    /// Open a file via its default desktop handler.
    ///
    /// Plain `xdg-open` ignores the handler's `Terminal=true` flag, so a file
    /// whose default app is a terminal program (nvim, vim, less, …) launches
    /// windowless and appears to do nothing. When the resolved handler wants a
    /// terminal we reconstruct its `Exec` line and run it through the
    /// configured terminal (reusing [`Self::launch_command`]'s terminal
    /// handling, including its `-e` convention).
    ///
    /// When the file's type has no associated application, the user is
    /// notified instead of silently getting nothing. GUI handlers and
    /// indeterminate resolutions fall back to plain `xdg-open`, preserving the
    /// prior behavior (Zed, browsers, image viewers, …).
    fn launch_file_open(path: &str, config: &Config) -> std::io::Result<std::process::Child> {
        match Self::resolve_default_handler(path) {
            HandlerResolution::Found(handler) if handler.terminal => {
                let exec = expand_exec_for_file(&handler.exec, path);
                info!(
                    "File handler is terminal-based; opening in terminal: {}",
                    exec
                );
                Self::launch_command(&exec, true, config)
            }
            HandlerResolution::NoAssociation => {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string());
                let msg = format!("No application is associated with '{}'", name);
                info!("{}", msg);
                crate::notify::notify("Cannot open file", &msg);
                // Best-effort: hand it to xdg-open anyway in case the desktop
                // environment offers its own "open with…" fallback.
                Self::launch_command(&format!("xdg-open {}", shell_quote(path)), false, config)
            }
            // GUI handler or indeterminate resolution: plain xdg-open.
            _ => Self::launch_command(&format!("xdg-open {}", shell_quote(path)), false, config),
        }
    }

    /// Resolve the default desktop handler for `path` by MIME type.
    ///
    /// Distinguishes "no application is associated" (worth informing the user)
    /// from "resolution tooling unavailable" (stay silent, fall back to
    /// `xdg-open`).
    fn resolve_default_handler(path: &str) -> HandlerResolution {
        // MIME type — `xdg-mime` missing/erroring leaves us indeterminate.
        let mime = match Self::xdg_mime_query(&["query", "filetype", path]) {
            Some(m) => m,
            None => return HandlerResolution::Indeterminate,
        };
        if mime.is_empty() {
            return HandlerResolution::NoAssociation;
        }

        // Default application for that type — empty means nothing is associated.
        let desktop_id = match Self::xdg_mime_query(&["query", "default", &mime]) {
            Some(d) => d,
            None => return HandlerResolution::Indeterminate,
        };
        if desktop_id.is_empty() {
            return HandlerResolution::NoAssociation;
        }

        // Locate and parse the named .desktop file.
        let desktop_path = match Self::locate_desktop_file(&desktop_id) {
            Some(p) => p,
            None => return HandlerResolution::Indeterminate,
        };
        match std::fs::read_to_string(&desktop_path)
            .ok()
            .and_then(|c| parse_desktop_handler(&c))
        {
            Some(handler) => HandlerResolution::Found(handler),
            None => HandlerResolution::Indeterminate,
        }
    }

    /// Run `xdg-mime` with `args`, returning trimmed stdout on success.
    fn xdg_mime_query(args: &[&str]) -> Option<String> {
        let out = Command::new("xdg-mime").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Locate a `.desktop` file by id across the XDG application directories.
    fn locate_desktop_file(desktop_id: &str) -> Option<std::path::PathBuf> {
        use std::path::PathBuf;

        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(data_home).join("applications"));
        } else if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".local/share/applications"));
        }
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
            dirs.push(PathBuf::from(d).join("applications"));
        }

        // Direct match first; then the freedesktop nested-id convention where
        // `subdir-name.desktop` lives at `subdir/name.desktop`.
        for dir in &dirs {
            let direct = dir.join(desktop_id);
            if direct.is_file() {
                return Some(direct);
            }
        }
        if desktop_id.contains('-') {
            let nested = desktop_id.replace('-', "/");
            for dir in &dirs {
                let p = dir.join(&nested);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        None
    }
}

/// A file's default desktop handler: its `Exec` line (field codes intact) and
/// whether it requests a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopHandler {
    exec: String,
    terminal: bool,
}

/// Outcome of resolving a file's default application.
enum HandlerResolution {
    /// A default handler was found and parsed.
    Found(DesktopHandler),
    /// The type has no associated default application (or no MIME type) — the
    /// user should be told rather than getting a silent no-op.
    NoAssociation,
    /// Resolution couldn't run to completion (tooling absent, handler file
    /// missing, …); stay silent and fall back to `xdg-open`.
    Indeterminate,
}

/// Shell-quote `s` for inclusion in a single-quoted `sh -c` argument.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parse the `[Desktop Entry]` group of a `.desktop` file for `Exec=` and
/// `Terminal=`. Only the first `Exec=` in the group is used, and only that
/// group is consulted (action groups are ignored). Returns `None` when there
/// is no `Exec` line — nothing to launch.
fn parse_desktop_handler(contents: &str) -> Option<DesktopHandler> {
    let mut in_entry = false;
    let mut exec: Option<String> = None;
    let mut terminal = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(value.trim().to_string());
            }
        } else if let Some(value) = line.strip_prefix("Terminal=") {
            terminal = value.trim().eq_ignore_ascii_case("true");
        }
    }

    exec.map(|exec| DesktopHandler { exec, terminal })
}

/// Expand a desktop `Exec` line's field codes against a single `path`.
///
/// `%f %F %u %U` are replaced with the shell-quoted path; `%%` becomes a
/// literal `%`; every other code (`%i %c %k`, …) is dropped per the
/// freedesktop spec. If the line carries no file field code, the path is
/// appended so the file is still passed to the handler.
fn expand_exec_for_file(exec: &str, path: &str) -> String {
    let quoted = shell_quote(path);
    let mut out = String::new();
    let mut substituted = false;
    let mut chars = exec.chars();

    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('f') | Some('F') | Some('u') | Some('U') => {
                out.push_str(&quoted);
                substituted = true;
            }
            Some('%') => out.push('%'),
            _ => {} // drop %i %c %k and unknown/trailing codes
        }
    }

    let mut out = out.trim().to_string();
    if !substituted {
        out.push(' ');
        out.push_str(&quoted);
    }
    out
}

impl std::ops::Deref for MainWindow {
    type Target = ApplicationWindow;

    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(provider: ProviderType, id: &str) -> LaunchItem {
        LaunchItem {
            id: id.to_string(),
            name: "x".into(),
            description: None,
            icon: None,
            provider,
            command: "xdg-open '/tmp/x'".into(),
            terminal: false,
            tags: Vec::new(),
            source: ItemSource::Core,
        }
    }

    #[test]
    fn filesearch_open_path_extracts_path_from_id() {
        let it = item(
            ProviderType::Plugin("filesearch".into()),
            "file:/home/u/a.rs",
        );
        assert_eq!(
            MainWindow::filesearch_open_path(&it).as_deref(),
            Some("/home/u/a.rs")
        );
    }

    #[test]
    fn filesearch_open_path_none_for_other_providers() {
        let app = item(ProviderType::Application, "file:/home/u/a.rs");
        assert!(MainWindow::filesearch_open_path(&app).is_none());
        let calc = item(ProviderType::Plugin("calc".into()), "file:/home/u/a.rs");
        assert!(MainWindow::filesearch_open_path(&calc).is_none());
    }

    #[test]
    fn parse_handler_reads_command_and_terminal_true() {
        let contents = "[Desktop Entry]\nName=Neovim\nExec=nvim %F\nTerminal=true\n";
        let h = parse_desktop_handler(contents).expect("has command");
        assert_eq!(h.exec, "nvim %F");
        assert!(h.terminal);
    }

    #[test]
    fn parse_handler_absent_terminal_is_false() {
        // Regression guard: GUI handlers (no Terminal= line) must NOT be
        // treated as terminal apps, so they keep using xdg-open.
        let contents = "[Desktop Entry]\nName=Zed\nExec=zeditor %U\n";
        let h = parse_desktop_handler(contents).expect("has command");
        assert!(!h.terminal);
    }

    #[test]
    fn parse_handler_terminal_value_is_case_insensitive() {
        let contents = "[Desktop Entry]\nExec=nvim %F\nTerminal=True\n";
        assert!(parse_desktop_handler(contents).unwrap().terminal);
    }

    #[test]
    fn parse_handler_ignores_action_groups() {
        // Terminal=true in an action group must not leak into the main entry.
        let contents = "[Desktop Entry]\nExec=zeditor %U\nTerminal=false\n\n\
                        [Desktop Action new]\nExec=zeditor --new\nTerminal=true\n";
        let h = parse_desktop_handler(contents).expect("has command");
        assert_eq!(h.exec, "zeditor %U");
        assert!(!h.terminal);
    }

    #[test]
    fn parse_handler_none_without_command_line() {
        let contents = "[Desktop Entry]\nName=Broken\nTerminal=true\n";
        assert!(parse_desktop_handler(contents).is_none());
    }

    #[test]
    fn expand_substitutes_file_field_codes() {
        assert_eq!(
            expand_exec_for_file("nvim %F", "/home/u/a.rs"),
            "nvim '/home/u/a.rs'"
        );
        assert_eq!(expand_exec_for_file("vim %f", "/tmp/x"), "vim '/tmp/x'");
        assert_eq!(expand_exec_for_file("app %u", "/tmp/x"), "app '/tmp/x'");
        assert_eq!(expand_exec_for_file("app %U", "/tmp/x"), "app '/tmp/x'");
    }

    #[test]
    fn expand_drops_unknown_codes_and_keeps_literal_percent() {
        assert_eq!(
            expand_exec_for_file("app %i %c %F 100%%", "/tmp/x"),
            "app   '/tmp/x' 100%"
        );
    }

    #[test]
    fn expand_appends_path_when_no_field_code() {
        assert_eq!(expand_exec_for_file("nvim", "/tmp/x"), "nvim '/tmp/x'");
    }

    #[test]
    fn expand_quotes_embedded_single_quotes() {
        assert_eq!(
            expand_exec_for_file("nvim %F", "/tmp/it's a file"),
            "nvim '/tmp/it'\\''s a file'"
        );
    }
}
