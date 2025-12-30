use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::providers::{LaunchItem, ProviderManager, ProviderType};
use crate::ui::submenu;
use crate::ui::ResultRow;
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

pub struct MainWindow {
    window: ApplicationWindow,
    search_entry: Entry,
    results_list: ListBox,
    scrolled: ScrolledWindow,
    config: Rc<RefCell<Config>>,
    providers: Rc<RefCell<ProviderManager>>,
    frecency: Rc<RefCell<FrecencyStore>>,
    current_results: Rc<RefCell<Vec<LaunchItem>>>,
    filter: Rc<RefCell<ProviderFilter>>,
    mode_label: Label,
    hints_label: Label,
    filter_buttons: Rc<RefCell<HashMap<ProviderType, ToggleButton>>>,
    submenu_state: Rc<RefCell<SubmenuState>>,
    /// Parsed tab config (ProviderTypes for cycling)
    tab_order: Rc<Vec<ProviderType>>,
}

impl MainWindow {
    pub fn new(
        app: &Application,
        config: Rc<RefCell<Config>>,
        providers: Rc<RefCell<ProviderManager>>,
        frecency: Rc<RefCell<FrecencyStore>>,
        filter: Rc<RefCell<ProviderFilter>>,
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

        // Parse tabs config to ProviderTypes
        let tab_order: Vec<ProviderType> = cfg
            .general
            .tabs
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        let tab_order = Rc::new(tab_order);

        // Create toggle buttons for each provider (from config)
        let filter_buttons = Self::create_filter_buttons(&filter_tabs, &filter, &cfg.general.tabs);
        let filter_buttons = Rc::new(RefCell::new(filter_buttons));

        header_box.append(&mode_label);
        header_box.append(&filter_tabs);

        // Search entry with dynamic placeholder
        let placeholder = Self::build_placeholder(&filter.borrow());
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

        let main_window = Self {
            window,
            search_entry,
            results_list,
            scrolled,
            config,
            providers,
            frecency,
            current_results: Rc::new(RefCell::new(Vec::new())),
            filter,
            mode_label,
            hints_label,
            filter_buttons,
            submenu_state: Rc::new(RefCell::new(SubmenuState::default())),
            tab_order,
        };

        main_window.setup_signals();
        main_window.update_results("");

        // Ensure search entry has focus when window is shown
        main_window.search_entry.grab_focus();

        // Schedule widget refresh after window is shown
        // Widget providers (weather, media, pomodoro) may make network/dbus calls
        // We defer this to avoid blocking startup, then re-render results
        let providers_for_refresh = main_window.providers.clone();
        let search_entry_for_refresh = main_window.search_entry.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            providers_for_refresh.borrow_mut().refresh_widgets();
            // Trigger UI update by emitting changed signal on search entry
            search_entry_for_refresh.emit_by_name::<()>("changed", &[]);
        });

        // Set up periodic widget auto-refresh (every 5 seconds)
        // Always refresh widgets (for pomodoro timer/notifications), but only update UI when visible
        let providers_for_auto = main_window.providers.clone();
        let current_results_for_auto = main_window.current_results.clone();
        let submenu_state_for_auto = main_window.submenu_state.clone();
        gtk4::glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
            // Skip UI updates if in submenu, but still refresh providers for notifications
            let in_submenu = submenu_state_for_auto.borrow().active;

            // Always refresh widget providers (pomodoro needs this for timer/notifications)
            providers_for_auto.borrow_mut().refresh_widgets();

            // Only update UI if not in submenu and widgets are visible
            if !in_submenu {
                // Collect widget type_ids first to avoid borrow conflicts
                let widget_ids: Vec<String> = providers_for_auto
                    .borrow()
                    .widget_type_ids()
                    .map(|s| s.to_string())
                    .collect();

                let mut results = current_results_for_auto.borrow_mut();
                for type_id in &widget_ids {
                    if let Some(new_item) = providers_for_auto.borrow().get_widget_item(type_id) {
                        if let Some(existing) = results.iter_mut().find(|i| i.id == new_item.id) {
                            existing.name = new_item.name;
                            existing.description = new_item.description;
                        }
                    }
                }
            }
            gtk4::glib::ControlFlow::Continue
        });

        main_window
    }

    fn create_filter_buttons(
        container: &GtkBox,
        filter: &Rc<RefCell<ProviderFilter>>,
        tabs: &[String],
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

            let label = Self::provider_tab_label(&provider_type);
            let shortcut = format!("Ctrl+{}", idx + 1);

            let button = ToggleButton::builder()
                .label(label)
                .tooltip_text(&shortcut)
                .active(filter.borrow().is_enabled(provider_type.clone()))
                .build();

            button.add_css_class("owlry-filter-button");
            let css_class = Self::provider_css_class(&provider_type);
            button.add_css_class(css_class);

            container.append(&button);
            buttons.insert(provider_type, button);
        }

        buttons
    }

    /// Get display label for a provider tab
    fn provider_tab_label(provider: &ProviderType) -> &'static str {
        match provider {
            ProviderType::Application => "Apps",
            ProviderType::Bookmarks => "Bookmarks",
            ProviderType::Calculator => "Calc",
            ProviderType::Clipboard => "Clip",
            ProviderType::Command => "Cmds",
            ProviderType::Dmenu => "Dmenu",
            ProviderType::Emoji => "Emoji",
            ProviderType::Files => "Files",
            ProviderType::MediaPlayer => "Media",
            ProviderType::Pomodoro => "Pomo",
            ProviderType::Scripts => "Scripts",
            ProviderType::Ssh => "SSH",
            ProviderType::System => "System",
            ProviderType::Uuctl => "uuctl",
            ProviderType::Weather => "Weather",
            ProviderType::WebSearch => "Web",
            ProviderType::Plugin(_) => "Plugin",
        }
    }

    /// Get CSS class for a provider
    fn provider_css_class(provider: &ProviderType) -> &'static str {
        match provider {
            ProviderType::Application => "owlry-filter-app",
            ProviderType::Bookmarks => "owlry-filter-bookmark",
            ProviderType::Calculator => "owlry-filter-calc",
            ProviderType::Clipboard => "owlry-filter-clip",
            ProviderType::Command => "owlry-filter-cmd",
            ProviderType::Dmenu => "owlry-filter-dmenu",
            ProviderType::Emoji => "owlry-filter-emoji",
            ProviderType::Files => "owlry-filter-file",
            ProviderType::MediaPlayer => "owlry-filter-media",
            ProviderType::Pomodoro => "owlry-filter-pomodoro",
            ProviderType::Scripts => "owlry-filter-script",
            ProviderType::Ssh => "owlry-filter-ssh",
            ProviderType::System => "owlry-filter-sys",
            ProviderType::Uuctl => "owlry-filter-uuctl",
            ProviderType::Weather => "owlry-filter-weather",
            ProviderType::WebSearch => "owlry-filter-web",
            ProviderType::Plugin(_) => "owlry-filter-plugin",
        }
    }

    fn build_placeholder(filter: &ProviderFilter) -> String {
        let active: Vec<&str> = filter
            .enabled_providers()
            .iter()
            .map(|p| match p {
                ProviderType::Application => "applications",
                ProviderType::Bookmarks => "bookmarks",
                ProviderType::Calculator => "calculator",
                ProviderType::Clipboard => "clipboard",
                ProviderType::Command => "commands",
                ProviderType::Dmenu => "options",
                ProviderType::Emoji => "emoji",
                ProviderType::Files => "files",
                ProviderType::MediaPlayer => "media",
                ProviderType::Pomodoro => "pomodoro",
                ProviderType::Scripts => "scripts",
                ProviderType::Ssh => "SSH hosts",
                ProviderType::System => "system",
                ProviderType::Uuctl => "uuctl units",
                ProviderType::Weather => "weather",
                ProviderType::WebSearch => "web",
                ProviderType::Plugin(_) => "plugins",
            })
            .collect();

        format!("Search {}...", active.join(", "))
    }

    /// Build dynamic hints based on enabled providers
    fn build_hints(config: &crate::config::ProvidersConfig) -> String {
        let mut parts: Vec<String> = vec![
            "Tab: cycle".to_string(),
            "↑↓: nav".to_string(),
            "Enter: launch".to_string(),
            "Esc: close".to_string(),
        ];

        // Add trigger hints for enabled dynamic providers
        if config.calculator {
            parts.push("= calc".to_string());
        }
        if config.websearch {
            parts.push("? web".to_string());
        }
        if config.files {
            parts.push("/ files".to_string());
        }

        // Add prefix hints for static providers
        let mut prefixes = Vec::new();
        if config.system {
            prefixes.push(":sys");
        }
        if config.emoji {
            prefixes.push(":emoji");
        }
        if config.ssh {
            prefixes.push(":ssh");
        }
        if config.clipboard {
            prefixes.push(":clip");
        }
        if config.bookmarks {
            prefixes.push(":bm");
        }

        // Only show first few prefixes to avoid overflow
        if !prefixes.is_empty() {
            parts.push(prefixes[..prefixes.len().min(4)].join(" "));
        }

        parts.join("  ")
    }

    /// Scroll the given row into view within the scrolled window
    fn scroll_to_row(scrolled: &ScrolledWindow, results_list: &ListBox, row: &ListBoxRow) {
        let vadj = scrolled.vadjustment();

        let row_index = row.index();
        if row_index < 0 {
            return;
        }

        let visible_height = vadj.page_size();
        let current_scroll = vadj.value();

        let list_height = results_list.height() as f64;
        let row_count = {
            let mut count = 0;
            let mut child = results_list.first_child();
            while child.is_some() {
                count += 1;
                child = child.and_then(|c| c.next_sibling());
            }
            count.max(1) as f64
        };

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
        debug!("[UI] Entering submenu: {} ({} actions)", display_name, actions.len());

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
        while let Some(child) = results_list.first_child() {
            results_list.remove(&child);
        }

        for item in &actions {
            let row = ResultRow::new(item);
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
        mode_label.set_label(filter.borrow().mode_display_name());
        hints_label.set_label(&Self::build_hints(&config.borrow().providers));
        search_entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow())));
        search_entry.set_text(&saved_search);

        // Trigger refresh by emitting changed signal
        search_entry.emit_by_name::<()>("changed", &[]);
    }

    fn setup_signals(&self) {
        // Search input handling with prefix detection
        let providers = self.providers.clone();
        let results_list = self.results_list.clone();
        let config = self.config.clone();
        let frecency = self.frecency.clone();
        let current_results = self.current_results.clone();
        let filter = self.filter.clone();
        let mode_label = self.mode_label.clone();
        let search_entry_for_change = self.search_entry.clone();
        let submenu_state = self.submenu_state.clone();

        self.search_entry.connect_changed(move |entry| {
            let raw_query = entry.text();

            // If in submenu, filter the submenu items
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
                while let Some(child) = results_list.first_child() {
                    results_list.remove(&child);
                }

                for item in &filtered {
                    let row = ResultRow::new(item);
                    results_list.append(&row);
                }

                if let Some(first_row) = results_list.row_at_index(0) {
                    results_list.select_row(Some(&first_row));
                }

                *current_results.borrow_mut() = filtered;
                return;
            }

            // Normal mode: parse prefix and search
            let parsed = ProviderFilter::parse_query(&raw_query);

            {
                let mut f = filter.borrow_mut();
                f.set_prefix(parsed.prefix.clone());
            }

            mode_label.set_label(filter.borrow().mode_display_name());

            if let Some(ref prefix) = parsed.prefix {
                let prefix_name = match prefix {
                    ProviderType::Application => "applications",
                    ProviderType::Bookmarks => "bookmarks",
                    ProviderType::Calculator => "calculator",
                    ProviderType::Clipboard => "clipboard",
                    ProviderType::Command => "commands",
                    ProviderType::Dmenu => "options",
                    ProviderType::Emoji => "emoji",
                    ProviderType::Files => "files",
                    ProviderType::MediaPlayer => "media",
                    ProviderType::Pomodoro => "pomodoro",
                    ProviderType::Scripts => "scripts",
                    ProviderType::Ssh => "SSH hosts",
                    ProviderType::System => "system",
                    ProviderType::Uuctl => "uuctl units",
                    ProviderType::Weather => "weather",
                    ProviderType::WebSearch => "web",
                    ProviderType::Plugin(_) => "plugins",
                };
                search_entry_for_change
                    .set_placeholder_text(Some(&format!("Search {}...", prefix_name)));
            }

            let cfg = config.borrow();
            let max_results = cfg.general.max_results;
            let frecency_weight = cfg.providers.frecency_weight;
            let use_frecency = cfg.providers.frecency;
            drop(cfg);

            let results: Vec<LaunchItem> = if use_frecency {
                providers
                    .borrow_mut()
                    .search_with_frecency(&parsed.query, max_results, &filter.borrow(), &frecency.borrow(), frecency_weight, parsed.tag_filter.as_deref())
                    .into_iter()
                    .map(|(item, _)| item)
                    .collect()
            } else {
                providers
                    .borrow()
                    .search_filtered(&parsed.query, max_results, &filter.borrow())
                    .into_iter()
                    .map(|(item, _)| item)
                    .collect()
            };

            while let Some(child) = results_list.first_child() {
                results_list.remove(&child);
            }

            for item in &results {
                let row = ResultRow::new(item);
                results_list.append(&row);
            }

            if let Some(first_row) = results_list.row_at_index(0) {
                results_list.select_row(Some(&first_row));
            }

            *current_results.borrow_mut() = results;
        });

        // Entry activate signal (Enter key in search entry)
        let results_list_for_activate = self.results_list.clone();
        let current_results_for_activate = self.current_results.clone();
        let config_for_activate = self.config.clone();
        let frecency_for_activate = self.frecency.clone();
        let providers_for_activate = self.providers.clone();
        let window_for_activate = self.window.clone();
        let submenu_state_for_activate = self.submenu_state.clone();
        let mode_label_for_activate = self.mode_label.clone();
        let hints_label_for_activate = self.hints_label.clone();
        let search_entry_for_activate = self.search_entry.clone();

        self.search_entry.connect_activate(move |entry| {
            let selected = results_list_for_activate
                .selected_row()
                .or_else(|| results_list_for_activate.row_at_index(0));

            if let Some(row) = selected {
                let index = row.index() as usize;
                let results = current_results_for_activate.borrow();
                if let Some(item) = results.get(index) {
                    // Check if this is a submenu item and query the plugin for actions
                    let submenu_result = if submenu::is_submenu_item(item) {
                        if let Some((plugin_id, data)) = submenu::parse_submenu_command(&item.command) {
                            // Clone values before dropping borrow
                            let plugin_id = plugin_id.to_string();
                            let data = data.to_string();
                            let display_name = item.name.clone();
                            drop(results); // Release borrow before querying
                            providers_for_activate
                                .borrow()
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
                                &frecency_for_activate,
                                &providers_for_activate,
                            );
                            if should_close {
                                window_for_activate.close();
                            } else {
                                // Trigger search refresh for updated widget state
                                entry.emit_by_name::<()>("changed", &[]);
                            }
                        }
                    }
                }
            }
        });

        // Filter button signals
        for (provider_type, button) in self.filter_buttons.borrow().iter() {
            let filter = self.filter.clone();
            let search_entry = self.search_entry.clone();
            let mode_label = self.mode_label.clone();
            let ptype = provider_type.clone();

            button.connect_toggled(move |btn| {
                {
                    let mut f = filter.borrow_mut();
                    if btn.is_active() {
                        f.enable(ptype.clone());
                    } else {
                        f.disable(ptype.clone());
                    }
                }
                mode_label.set_label(filter.borrow().mode_display_name());
                search_entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow())));
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

        key_controller.connect_key_pressed(move |_, key, _, modifiers| {
            let ctrl = modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(gtk4::gdk::ModifierType::SHIFT_MASK);

            #[cfg(feature = "dev-logging")]
            debug!("[UI] Key pressed: {:?} (ctrl={}, shift={})", key, ctrl, shift);

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
                        );
                        gtk4::glib::Propagation::Stop
                    } else {
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
                        );
                        gtk4::glib::Propagation::Stop
                    } else {
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
                            Self::scroll_to_row(&scrolled, &results_list, &next_row);
                        }
                    }
                    gtk4::glib::Propagation::Stop
                }
                Key::Up => {
                    if let Some(selected) = results_list.selected_row() {
                        let prev_index = selected.index() - 1;
                        if prev_index >= 0
                            && let Some(prev_row) = results_list.row_at_index(prev_index) {
                                results_list.select_row(Some(&prev_row));
                                Self::scroll_to_row(&scrolled, &results_list, &prev_row);
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
                        );
                    }
                    gtk4::glib::Propagation::Stop
                }
                // Ctrl+1-9 toggle specific providers based on tab order (only when not in submenu)
                Key::_1 | Key::_2 | Key::_3 | Key::_4 | Key::_5 |
                Key::_6 | Key::_7 | Key::_8 | Key::_9 if ctrl => {
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
                        if let Some(provider) = tab_order.get(idx) {
                            Self::toggle_provider_button(
                                provider.clone(),
                                &filter,
                                &filter_buttons,
                                &search_entry,
                                &mode_label,
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
        let frecency = self.frecency.clone();
        let providers = self.providers.clone();
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
                        providers
                            .borrow()
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
                        let should_close = Self::handle_item_action(&item, &config.borrow(), &frecency, &providers);
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
    ) {
        if tab_order.is_empty() {
            return;
        }

        let current = filter.borrow().enabled_providers();

        let next = if current.len() == 1 {
            let idx = tab_order.iter().position(|p| p == &current[0]).unwrap_or(0);
            if forward {
                tab_order[(idx + 1) % tab_order.len()].clone()
            } else {
                tab_order[(idx + tab_order.len() - 1) % tab_order.len()].clone()
            }
        } else {
            tab_order[0].clone()
        };

        {
            let mut f = filter.borrow_mut();
            f.set_single_mode(next.clone());
        }

        for (ptype, button) in buttons.borrow().iter() {
            button.set_active(ptype == &next);
        }

        mode_label.set_label(filter.borrow().mode_display_name());
        entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow())));
        entry.emit_by_name::<()>("changed", &[]);
    }

    fn toggle_provider_button(
        provider: ProviderType,
        filter: &Rc<RefCell<ProviderFilter>>,
        buttons: &Rc<RefCell<HashMap<ProviderType, ToggleButton>>>,
        entry: &Entry,
        mode_label: &Label,
    ) {
        {
            let mut f = filter.borrow_mut();
            f.toggle(provider.clone());
        }

        if let Some(button) = buttons.borrow().get(&provider) {
            button.set_active(filter.borrow().is_enabled(provider));
        }

        mode_label.set_label(filter.borrow().mode_display_name());
        entry.set_placeholder_text(Some(&Self::build_placeholder(&filter.borrow())));
        entry.emit_by_name::<()>("changed", &[]);
    }

    fn update_results(&self, query: &str) {
        let cfg = self.config.borrow();
        let max_results = cfg.general.max_results;
        let frecency_weight = cfg.providers.frecency_weight;
        let use_frecency = cfg.providers.frecency;
        drop(cfg);

        let results: Vec<LaunchItem> = if use_frecency {
            self.providers
                .borrow_mut()
                .search_with_frecency(query, max_results, &self.filter.borrow(), &self.frecency.borrow(), frecency_weight, None)
                .into_iter()
                .map(|(item, _)| item)
                .collect()
        } else {
            self.providers
                .borrow()
                .search_filtered(query, max_results, &self.filter.borrow())
                .into_iter()
                .map(|(item, _)| item)
                .collect()
        };

        while let Some(child) = self.results_list.first_child() {
            self.results_list.remove(&child);
        }

        for item in &results {
            let row = ResultRow::new(item);
            self.results_list.append(&row);
        }

        if let Some(first_row) = self.results_list.row_at_index(0) {
            self.results_list.select_row(Some(&first_row));
        }

        *self.current_results.borrow_mut() = results;
    }

    /// Handle item activation - returns true if window should close
    fn handle_item_action(
        item: &LaunchItem,
        config: &Config,
        frecency: &Rc<RefCell<FrecencyStore>>,
        providers: &Rc<RefCell<ProviderManager>>,
    ) -> bool {
        // Check for plugin internal commands (format: PLUGIN_ID:action)
        // These are handled by the plugin itself, not launched as shell commands
        if providers.borrow().execute_plugin_action(&item.command) {
            // Plugin handled the action - don't close window
            // User might want to see updated state (e.g., pomodoro timer)
            return false;
        }

        // Regular item launch
        Self::launch_item(item, config, frecency);
        true
    }

    fn launch_item(item: &LaunchItem, config: &Config, frecency: &Rc<RefCell<FrecencyStore>>) {
        // Record this launch for frecency tracking
        if config.providers.frecency {
            frecency.borrow_mut().record_launch(&item.id);
            #[cfg(feature = "dev-logging")]
            debug!("[UI] Recorded frecency launch for: {}", item.id);
        }

        info!("Launching: {} ({})", item.name, item.command);

        #[cfg(feature = "dev-logging")]
        debug!("[UI] Launch details: terminal={}, provider={:?}", item.terminal, item.provider);

        let cmd = if item.terminal {
            format!("{} -e {}", config.general.terminal_command, item.command)
        } else {
            item.command.clone()
        };

        // Detect if this is a shell command vs an application launch
        // Shell commands: playerctl, dbus-send, systemctl, journalctl, or anything with shell operators
        let is_shell_command = cmd.starts_with("playerctl ")
            || cmd.starts_with("dbus-send ")
            || cmd.starts_with("systemctl ")
            || cmd.starts_with("journalctl ")
            || cmd.contains(" | ")
            || cmd.contains(" && ")
            || cmd.contains(" || ")
            || cmd.contains(" > ")
            || cmd.contains(" < ");

        // Use launch wrapper if configured (uwsm, hyprctl, etc.)
        // But skip wrapper for shell commands - they need sh -c
        let result = match &config.general.launch_wrapper {
            Some(wrapper) if !wrapper.is_empty() && !is_shell_command => {
                info!("Using launch wrapper: {}", wrapper);
                // Split wrapper into command and args (e.g., "uwsm app --" -> ["uwsm", "app", "--"])
                let mut wrapper_parts: Vec<&str> = wrapper.split_whitespace().collect();
                if wrapper_parts.is_empty() {
                    Command::new("sh").arg("-c").arg(&cmd).spawn()
                } else {
                    let wrapper_cmd = wrapper_parts.remove(0);
                    Command::new(wrapper_cmd)
                        .args(&wrapper_parts)
                        .arg(&cmd)
                        .spawn()
                }
            }
            _ => Command::new("sh").arg("-c").arg(&cmd).spawn(),
        };

        if let Err(e) = result {
            log::error!("Failed to launch '{}': {}", item.name, e);
        }
    }
}

impl std::ops::Deref for MainWindow {
    type Target = ApplicationWindow;

    fn deref(&self) -> &Self::Target {
        &self.window
    }
}
