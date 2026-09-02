use super::*;
use crate::ui::actions;
use gtk4::gdk::Key;
use gtk4::{EventControllerKey, ListBoxRow};
use log::info;

#[cfg(feature = "dev-logging")]
use log::debug;

impl MainWindow {
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
    pub(super) fn enter_submenu(
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
    pub(super) fn exit_submenu(
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

    pub(super) fn setup_keyboard_navigation(&self) {
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
        let show_all = filter.borrow().show_all_tab();
        // With the All view hidden the cycle is the tabs alone, so a launch
        // that is somehow showing several at once still has to collapse to a
        // single tab rather than being read as "currently All".
        let all_enabled = show_all && current.len() == tab_order.len();

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

            if at_boundary && !show_all {
                // No All view to fall back to: wrap straight around.
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
            } else if at_boundary {
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
