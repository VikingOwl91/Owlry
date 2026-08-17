use crate::backend::SearchBackend;
use crate::config::Config;
use crate::filter::ProviderFilter;
use crate::ipc::ProviderDesc;
use crate::providers::{CommandProvider, ItemActivation, LaunchItem, ProviderType};
use crate::ui::ResultRow;
use crate::ui::actions;
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
use std::rc::Rc;

mod launch;
mod results;

use launch::HandlerResolution;
use results::{INITIAL_RESULTS, LazyLoadState};

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
            state.open_with = None; // regular action submenu, not a picker
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
            state.open_with = None;
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
            // A typed command with arguments takes precedence over a fuzzy
            // match for the bare executable. For example, entering
            // `mpv VIDEO` must launch the complete command, not just `mpv`.
            // Keep this out of submenu/open-with state, where Enter has a
            // different meaning.
            if !is_dmenu_mode_for_activate
                && !submenu_state_for_activate.borrow().active
                && config_for_activate.borrow().providers.commands
                && let Some(command) = Self::direct_command_text(&entry.text())
            {
                let launch_result =
                    Self::launch_command(&command, false, &config_for_activate.borrow());
                if let Err(e) = launch_result {
                    let msg = format!("Failed to launch '{}': {}", command, e);
                    log::error!("{}", msg);
                    crate::notify::notify("Launch failed", &msg);
                }
                window_for_activate.close();
                return;
            }

            let selected = results_list_for_activate
                .selected_row()
                .or_else(|| results_list_for_activate.row_at_index(0));

            // Handle the case where we have a selected item
            if let Some(row) = selected {
                let index = row.index() as usize;
                let results = current_results_for_activate.borrow();
                if let Some(item) = results.get(index) {
                    // "Open with…" picker is active: the selected row is an
                    // application. Plain Enter opens the file once (no persist).
                    if let Some(ctx) = submenu_state_for_activate.borrow().open_with.clone() {
                        let app = item.clone();
                        drop(results);
                        Self::pick_application(&app, &ctx, false, &config_for_activate.borrow());
                        if is_dmenu_mode_for_activate {
                            std::process::exit(0);
                        }
                        window_for_activate.close();
                        return;
                    }

                    // A file whose type has no associated application: offer the
                    // picker instead of the silent notify-only fallback.
                    if let Some(path) = Self::filesearch_open_path(item)
                        && matches!(
                            Self::resolve_default_handler(&path),
                            HandlerResolution::NoAssociation
                        )
                    {
                        drop(results);
                        Self::enter_open_with_picker(
                            &backend_for_activate,
                            &config_for_activate.borrow(),
                            &submenu_state_for_activate,
                            &results_list_for_activate,
                            &current_results_for_activate,
                            &mode_label_for_activate,
                            &hints_label_for_activate,
                            &search_entry_for_activate,
                            &path,
                        );
                        return;
                    }

                    let submenu_result = if let Some(request) = submenu::request_for_item(item) {
                        let display_name = item.name.clone();
                        drop(results);
                        backend_for_activate.borrow_mut().query_submenu_actions(
                            &request.provider_id,
                            &request.data,
                            &display_name,
                        )
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

        // Keyboard navigation. Capture phase so the window sees keys before the
        // focused entry — required to intercept Shift+Enter before the entry's
        // `activate` signal fires. Keys we don't act on are passed through with
        // Propagation::Proceed, so normal typing/Enter reach the entry as usual.
        let key_controller = EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let window = self.window.clone();
        let results_list = self.results_list.clone();
        let scrolled = self.scrolled.clone();
        let search_entry = self.search_entry.clone();
        let current_results = self.current_results.clone();
        let config = self.config.clone();
        let backend = self.backend.clone();
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
                Key::Right if !submenu_state.borrow().active => {
                    let selected = results_list
                        .selected_row()
                        .or_else(|| results_list.row_at_index(0));
                    let Some(row) = selected else {
                        return gtk4::glib::Propagation::Proceed;
                    };
                    let Some(item) = current_results.borrow().get(row.index() as usize).cloned()
                    else {
                        return gtk4::glib::Propagation::Proceed;
                    };
                    let item_actions = actions::for_item(&item);
                    if item_actions.is_empty() {
                        return gtk4::glib::Propagation::Proceed;
                    }
                    Self::enter_submenu(
                        &submenu_state,
                        &results_list,
                        &current_results,
                        &mode_label,
                        &hints_label,
                        &search_entry,
                        format!("{} — Actions", item.name),
                        item_actions,
                    );
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
                // Shift+Enter: "Open with…". On a file result, open the app
                // picker. Inside an active picker, set the chosen app as the
                // default for the file's type, then open.
                Key::Return | Key::KP_Enter if shift => {
                    let selected = results_list
                        .selected_row()
                        .or_else(|| results_list.row_at_index(0));
                    let Some(row) = selected else {
                        return gtk4::glib::Propagation::Proceed;
                    };
                    let index = row.index() as usize;

                    // Inside the picker: persist the chosen app, then open.
                    let ctx = submenu_state.borrow().open_with.clone();
                    if let Some(ctx) = ctx {
                        let Some(app) = current_results.borrow().get(index).cloned() else {
                            return gtk4::glib::Propagation::Stop;
                        };
                        Self::pick_application(&app, &ctx, true, &config.borrow());
                        if is_dmenu_mode {
                            std::process::exit(0);
                        }
                        window.close();
                        return gtk4::glib::Propagation::Stop;
                    }

                    // On a file result: open the "Open with…" picker.
                    let path = current_results
                        .borrow()
                        .get(index)
                        .and_then(Self::filesearch_open_path);
                    if let Some(path) = path {
                        Self::enter_open_with_picker(
                            &backend,
                            &config.borrow(),
                            &submenu_state,
                            &results_list,
                            &current_results,
                            &mode_label,
                            &hints_label,
                            &search_entry,
                            &path,
                        );
                        return gtk4::glib::Propagation::Stop;
                    }

                    // Non-file result: let the entry's activate handler run.
                    gtk4::glib::Propagation::Proceed
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
                // "Open with…" picker active: the row is an application; a
                // double-click opens the file with it once (no persistence),
                // mirroring plain Enter.
                if let Some(ctx) = submenu_state.borrow().open_with.clone() {
                    let app = item.clone();
                    drop(results);
                    Self::pick_application(&app, &ctx, false, &config.borrow());
                    window.close();
                    return;
                }

                // A file with no associated application: open the picker rather
                // than the notify-only fallback, matching the Enter path.
                if let Some(path) = Self::filesearch_open_path(item)
                    && matches!(
                        Self::resolve_default_handler(&path),
                        HandlerResolution::NoAssociation
                    )
                {
                    drop(results);
                    Self::enter_open_with_picker(
                        &backend,
                        &config.borrow(),
                        &submenu_state,
                        &results_list_for_click,
                        &current_results,
                        &mode_label,
                        &hints_label,
                        &search_entry,
                        &path,
                    );
                    return;
                }

                let submenu_result = if let Some(request) = submenu::request_for_item(item) {
                    let display_name = item.name.clone();
                    drop(results);
                    backend.borrow_mut().query_submenu_actions(
                        &request.provider_id,
                        &request.data,
                        &display_name,
                    )
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

    /// Return the command payload for a user-entered direct command.
    ///
    /// Bare queries remain normal fuzzy searches unless their first token is
    /// an executable and there is at least one argument. `:cmd` is accepted as
    /// an explicit equivalent; other prefixes keep their existing behavior.
    fn direct_command_text(raw: &str) -> Option<String> {
        let parsed = ProviderFilter::parse_query(raw);
        let allowed_scope = parsed.prefix.is_none() || parsed.prefix == Some(ProviderType::Command);
        if !allowed_scope {
            return None;
        }

        let command = parsed.query.trim();
        (!command.is_empty() && CommandProvider::is_direct_command(command))
            .then(|| command.to_string())
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
                for button in buttons.borrow().values() {
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

    #[test]
    fn direct_command_text_accepts_bare_command_with_arguments() {
        assert_eq!(
            MainWindow::direct_command_text("printf hello").as_deref(),
            Some("printf hello")
        );
    }

    #[test]
    fn direct_command_text_accepts_cmd_prefix() {
        assert_eq!(
            MainWindow::direct_command_text(":cmd printf hello").as_deref(),
            Some("printf hello")
        );
    }

    #[test]
    fn direct_command_text_keeps_other_prefixes_as_searches() {
        assert!(MainWindow::direct_command_text(":app printf hello").is_none());
        assert!(MainWindow::direct_command_text("unknown-command hello").is_none());
        assert!(MainWindow::direct_command_text("printf").is_none());
    }
}
