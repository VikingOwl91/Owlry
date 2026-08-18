use super::launch::HandlerResolution;
use super::results::INITIAL_RESULTS;
use super::*;
use crate::providers::CommandProvider;
use crate::ui::submenu;

/// Debounce delay for search input (milliseconds).
const SEARCH_DEBOUNCE_MS: u64 = 50;

impl MainWindow {
    pub(super) fn setup_signals(&self) {
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
                        "rofimoji" => "emoji and kaomoji",
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

        self.setup_keyboard_navigation();

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
