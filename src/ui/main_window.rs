use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::providers::{LaunchItem, ProviderManager, ProviderType, UuctlProvider};
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

        // Create toggle buttons for each provider
        let filter_buttons = Self::create_filter_buttons(&filter_tabs, &filter);
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
            .label(&Self::build_hints(&cfg.providers))
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
        };

        main_window.setup_signals();
        main_window.update_results("");

        // Ensure search entry has focus when window is shown
        main_window.search_entry.grab_focus();

        main_window
    }

    fn create_filter_buttons(
        container: &GtkBox,
        filter: &Rc<RefCell<ProviderFilter>>,
    ) -> HashMap<ProviderType, ToggleButton> {
        let providers = [
            (ProviderType::Application, "Apps", "Ctrl+1"),
            (ProviderType::Command, "Cmds", "Ctrl+2"),
            (ProviderType::Uuctl, "uuctl", "Ctrl+3"),
        ];

        let mut buttons = HashMap::new();

        for (provider_type, label, shortcut) in providers {
            let button = ToggleButton::builder()
                .label(label)
                .tooltip_text(shortcut)
                .active(filter.borrow().is_enabled(provider_type))
                .build();

            button.add_css_class("owlry-filter-button");
            let css_class = match provider_type {
                ProviderType::Application => "owlry-filter-app",
                ProviderType::Bookmarks => "owlry-filter-bookmark",
                ProviderType::Calculator => "owlry-filter-calc",
                ProviderType::Clipboard => "owlry-filter-clip",
                ProviderType::Command => "owlry-filter-cmd",
                ProviderType::Dmenu => "owlry-filter-dmenu",
                ProviderType::Emoji => "owlry-filter-emoji",
                ProviderType::Files => "owlry-filter-file",
                ProviderType::Scripts => "owlry-filter-script",
                ProviderType::Ssh => "owlry-filter-ssh",
                ProviderType::System => "owlry-filter-sys",
                ProviderType::Uuctl => "owlry-filter-uuctl",
                ProviderType::WebSearch => "owlry-filter-web",
            };
            button.add_css_class(css_class);

            container.append(&button);
            buttons.insert(provider_type, button);
        }

        buttons
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
                ProviderType::Scripts => "scripts",
                ProviderType::Ssh => "SSH hosts",
                ProviderType::System => "system",
                ProviderType::Uuctl => "uuctl units",
                ProviderType::WebSearch => "web",
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

    /// Enter submenu mode for a service
    fn enter_submenu(
        submenu_state: &Rc<RefCell<SubmenuState>>,
        results_list: &ListBox,
        current_results: &Rc<RefCell<Vec<LaunchItem>>>,
        mode_label: &Label,
        hints_label: &Label,
        search_entry: &Entry,
        unit_name: &str,
        display_name: &str,
        is_active: bool,
    ) {
        #[cfg(feature = "dev-logging")]
        debug!("[UI] Entering submenu for service: {} (active={})", unit_name, is_active);

        let actions = UuctlProvider::actions_for_service(unit_name, display_name, is_active);

        // Save current state
        {
            let mut state = submenu_state.borrow_mut();
            state.active = true;
            state.service_name = unit_name.to_string();
            state.display_name = display_name.to_string();
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
                f.set_prefix(parsed.prefix);
            }

            mode_label.set_label(filter.borrow().mode_display_name());

            if parsed.prefix.is_some() {
                let prefix_name = match parsed.prefix.unwrap() {
                    ProviderType::Application => "applications",
                    ProviderType::Bookmarks => "bookmarks",
                    ProviderType::Calculator => "calculator",
                    ProviderType::Clipboard => "clipboard",
                    ProviderType::Command => "commands",
                    ProviderType::Dmenu => "options",
                    ProviderType::Emoji => "emoji",
                    ProviderType::Files => "files",
                    ProviderType::Scripts => "scripts",
                    ProviderType::Ssh => "SSH hosts",
                    ProviderType::System => "system",
                    ProviderType::Uuctl => "uuctl units",
                    ProviderType::WebSearch => "web",
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
                    .search_with_frecency(&parsed.query, max_results, &filter.borrow(), &frecency.borrow(), frecency_weight)
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
        let window_for_activate = self.window.clone();
        let submenu_state_for_activate = self.submenu_state.clone();
        let mode_label_for_activate = self.mode_label.clone();
        let hints_label_for_activate = self.hints_label.clone();
        let search_entry_for_activate = self.search_entry.clone();

        self.search_entry.connect_activate(move |_| {
            let selected = results_list_for_activate
                .selected_row()
                .or_else(|| results_list_for_activate.row_at_index(0));

            if let Some(row) = selected {
                let index = row.index() as usize;
                let results = current_results_for_activate.borrow();
                if let Some(item) = results.get(index) {
                    // Check if this is a submenu item
                    if let Some((unit_name, display_name, is_active)) =
                        UuctlProvider::parse_submenu_data(item)
                    {
                        drop(results); // Release borrow before calling enter_submenu
                        Self::enter_submenu(
                            &submenu_state_for_activate,
                            &results_list_for_activate,
                            &current_results_for_activate,
                            &mode_label_for_activate,
                            &hints_label_for_activate,
                            &search_entry_for_activate,
                            &unit_name,
                            &display_name,
                            is_active,
                        );
                    } else {
                        // Execute the command
                        Self::launch_item(item, &config_for_activate.borrow(), &frecency_for_activate);
                        window_for_activate.close();
                    }
                }
            }
        });

        // Filter button signals
        for (provider_type, button) in self.filter_buttons.borrow().iter() {
            let filter = self.filter.clone();
            let search_entry = self.search_entry.clone();
            let mode_label = self.mode_label.clone();
            let ptype = *provider_type;

            button.connect_toggled(move |btn| {
                {
                    let mut f = filter.borrow_mut();
                    if btn.is_active() {
                        f.enable(ptype);
                    } else {
                        f.disable(ptype);
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
                        if prev_index >= 0 {
                            if let Some(prev_row) = results_list.row_at_index(prev_index) {
                                results_list.select_row(Some(&prev_row));
                                Self::scroll_to_row(&scrolled, &results_list, &prev_row);
                            }
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
                            false,
                        );
                    }
                    gtk4::glib::Propagation::Stop
                }
                // Ctrl+1/2/3 toggle specific providers (only when not in submenu)
                Key::_1 if ctrl => {
                    if !submenu_state.borrow().active {
                        Self::toggle_provider_button(
                            ProviderType::Application,
                            &filter,
                            &filter_buttons,
                            &search_entry,
                            &mode_label,
                        );
                    }
                    gtk4::glib::Propagation::Stop
                }
                Key::_2 if ctrl => {
                    if !submenu_state.borrow().active {
                        Self::toggle_provider_button(
                            ProviderType::Command,
                            &filter,
                            &filter_buttons,
                            &search_entry,
                            &mode_label,
                        );
                    }
                    gtk4::glib::Propagation::Stop
                }
                Key::_3 if ctrl => {
                    if !submenu_state.borrow().active {
                        Self::toggle_provider_button(
                            ProviderType::Uuctl,
                            &filter,
                            &filter_buttons,
                            &search_entry,
                            &mode_label,
                        );
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
                // Check if this is a submenu item
                if let Some((unit_name, display_name, is_active)) =
                    UuctlProvider::parse_submenu_data(item)
                {
                    drop(results);
                    Self::enter_submenu(
                        &submenu_state,
                        &results_list_for_click,
                        &current_results,
                        &mode_label,
                        &hints_label,
                        &search_entry,
                        &unit_name,
                        &display_name,
                        is_active,
                    );
                } else {
                    Self::launch_item(item, &config.borrow(), &frecency);
                    window.close();
                }
            }
        });
    }

    fn cycle_filter_mode(
        filter: &Rc<RefCell<ProviderFilter>>,
        buttons: &Rc<RefCell<HashMap<ProviderType, ToggleButton>>>,
        entry: &Entry,
        mode_label: &Label,
        forward: bool,
    ) {
        let order = [
            ProviderType::Application,
            ProviderType::Command,
            ProviderType::Uuctl,
        ];
        let current = filter.borrow().enabled_providers();

        let next = if current.len() == 1 {
            let idx = order.iter().position(|p| p == &current[0]).unwrap_or(0);
            if forward {
                order[(idx + 1) % order.len()]
            } else {
                order[(idx + order.len() - 1) % order.len()]
            }
        } else {
            ProviderType::Application
        };

        {
            let mut f = filter.borrow_mut();
            f.set_single_mode(next);
        }

        for (ptype, button) in buttons.borrow().iter() {
            button.set_active(*ptype == next);
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
            f.toggle(provider);
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
                .search_with_frecency(query, max_results, &self.filter.borrow(), &self.frecency.borrow(), frecency_weight)
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

        // Use launch wrapper if configured (uwsm, hyprctl, etc.)
        let result = match &config.general.launch_wrapper {
            Some(wrapper) if !wrapper.is_empty() => {
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
