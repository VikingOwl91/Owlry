//! Experimental Slint frontend (feature: slint-ui, runtime-selected with
//! OWLRY_UI=slint).
//!
//! Reuses the real architecture boundary: same config path, same backend
//! construction as `app.rs`, same `ProviderFilter` semantics as the GTK
//! frontend, activation through `SearchBackend`. Only the widget layer is
//! replaced. See SLINT_EXPERIMENT.md for the experiment report.

mod launch;
mod theme;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::app::BOOT;
use crate::backend::{DaemonHandle, SearchBackend};
use crate::cli::CliArgs;
use crate::client::CoreClient;
use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::{ParsedQuery, ProviderFilter};
use crate::ipc::ProviderDesc;
use crate::providers::{DmenuProvider, ItemActivation, LaunchItem, ProviderManager, ProviderType};
use crate::ui::provider_meta;
use log::{debug, info, warn};
use slint::{ComponentHandle, ModelRc, VecModel};

slint::include_modules!();

const SEARCH_DEBOUNCE_MS: u64 = 50;

// All shared state is Arc/Mutex (not Rc/RefCell) because query results
// cross back from a worker thread through slint::invoke_from_event_loop,
// which requires Send closures.
#[derive(Clone)]
struct Ctx {
    config: Arc<Mutex<Config>>,
    backend: Arc<Mutex<SearchBackend>>,
    filter: Arc<Mutex<ProviderFilter>>,
    descs: Arc<HashMap<String, ProviderDesc>>,
    /// Items currently displayed (results or submenu contents).
    items: Arc<Mutex<Vec<LaunchItem>>>,
    /// Snapshot of the submenu item list while a submenu is open (typing
    /// filters within it, mirroring the GTK SubmenuState).
    submenu_items: Arc<Mutex<Option<Vec<LaunchItem>>>>,
    submenu_title: Arc<Mutex<String>>,
}

pub fn run(args: CliArgs) -> ! {
    let t0 = std::time::Instant::now();
    if let Some(boot) = BOOT.get() {
        debug!(
            "startup: slint activate reached +{:.1?} after process boot",
            boot.elapsed()
        );
    }

    let config = Arc::new(Mutex::new(Config::load_or_default()));

    // Backend construction: same shape as app.rs on_activate (dmenu stdin
    // detection -> local backend; else daemon IPC with local fallback).
    let backend = if DmenuProvider::has_stdin_data() {
        let mut dmenu = DmenuProvider::new();
        dmenu.enable();
        let manager = ProviderManager::new(vec![Box::new(dmenu)], Vec::new());
        SearchBackend::Local {
            providers: Box::new(manager),
            frecency: FrecencyStore::with_config(
                config.lock().unwrap().providers.frecency_prune_config(),
            ),
        }
    } else {
        match CoreClient::connect_or_start() {
            Ok(client) => {
                info!("Connected to owlry-core daemon");
                SearchBackend::Daemon(DaemonHandle::new(client))
            }
            Err(e) => {
                warn!("Failed to connect to daemon ({}), using local providers", e);
                crate::app::OwlryApp::create_local_backend(&config.lock().unwrap())
            }
        }
    };
    debug!("startup: backend ready +{:.1?}", t0.elapsed());
    let backend = Arc::new(Mutex::new(backend));

    // Provider descriptors for labels/placeholders (same IPC call the GTK
    // frontend makes).
    let descs: Arc<HashMap<String, ProviderDesc>> = Arc::new(
        backend
            .lock()
            .unwrap()
            .available_providers()
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect(),
    );

    // Filter resolution: mirrors app.rs (CLI --mode / --profile / default).
    let resolved_modes = crate::app::resolve_modes(&args, &config.lock().unwrap());
    let show_all_tab = resolved_modes.as_ref().map(|(_, s)| *s).unwrap_or(true);
    let filter = match &resolved_modes {
        Some((modes, _)) => {
            let types: Vec<ProviderType> = modes
                .iter()
                .map(|s| ProviderFilter::mode_string_to_provider_type(s))
                .collect();
            let tabs = config.lock().unwrap().general.tabs.clone();
            if types.len() == 1 {
                ProviderFilter::new(
                    Some(types[0].clone()),
                    None,
                    &config.lock().unwrap().providers,
                    &tabs,
                )
            } else {
                ProviderFilter::new(None, Some(types), &config.lock().unwrap().providers, &tabs)
            }
        }
        None => {
            let tabs = config.lock().unwrap().general.tabs.clone();
            ProviderFilter::new(None, None, &config.lock().unwrap().providers, &tabs)
        }
    };
    let mut filter = filter;
    if !show_all_tab {
        filter.hide_all_tab();
        if let Some(first) = filter.enabled_providers().first().cloned() {
            filter.set_single_mode(first);
        }
    }
    let filter = Arc::new(Mutex::new(filter));

    let ctx = Ctx {
        config: config.clone(),
        backend,
        filter: filter.clone(),
        descs,
        items: Arc::new(Mutex::new(Vec::new())),
        submenu_items: Arc::new(Mutex::new(None)),
        submenu_title: Arc::new(Mutex::new(String::new())),
    };
    let debounce = Rc::new(slint::Timer::default());

    let win = LauncherWindow::new().expect("failed to create Slint window");
    theme::apply(&win, &config.lock().unwrap().appearance);
    update_mode_label(&ctx, &win);
    update_placeholder(&ctx, &win);
    update_tabs(&ctx, &win);
    win.set_hint_text(
        "Enter: launch   Right: actions   Tab: cycle tabs   Ctrl+1..9: tab   Esc: close".into(),
    );

    // ---- callbacks -------------------------------------------------------

    win.on_query_changed({
        let ctx = ctx.clone();
        let win = win.as_weak();
        let debounce = debounce.clone();
        move |_| {
            let ctx = ctx.clone();
            let win = win.clone();
            let debounce = debounce.clone();
            // Submenu mode filters locally and immediately, like the GTK
            // SubmenuState path; normal queries go through the debounce.
            if ctx.submenu_items.lock().unwrap().is_some() {
                if let Some(w) = win.upgrade() {
                    filter_submenu_locally(&ctx, &w);
                }
                return;
            }
            debounce.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS),
                move || {
                    if let Some(w) = win.upgrade() {
                        run_query(&ctx, &w);
                    }
                },
            );
        }
    });

    win.on_move_selection({
        let ctx = ctx.clone();
        let win = win.as_weak();
        move |delta| {
            let Some(w) = win.upgrade() else {
                return;
            };
            debug!("slint: move-selection delta={}", delta);
            let count = ctx.items.lock().unwrap().len();
            if count == 0 {
                return;
            }
            let current = w.get_selected_index();
            let next = (current + delta).clamp(0, count as i32 - 1);
            w.set_selected_index(next);
        }
    });

    win.on_select_row({
        let win = win.as_weak();
        move |index| {
            if let Some(w) = win.upgrade() {
                w.set_selected_index(index);
            }
        }
    });

    win.on_activate_at({
        let ctx = ctx.clone();
        let win = win.as_weak();
        move |index| {
            let Some(w) = win.upgrade() else {
                return;
            };
            activate(&ctx, &w, index as usize);
        }
    });

    win.on_open_actions({
        let ctx = ctx.clone();
        let win = win.as_weak();
        move |index| {
            let Some(w) = win.upgrade() else {
                return;
            };
            open_actions(&ctx, &w, index as usize);
        }
    });

    win.on_cycle_tab({
        let ctx = ctx.clone();
        let win = win.as_weak();
        move |direction| {
            let Some(w) = win.upgrade() else {
                return;
            };
            exit_submenu_for_query(&ctx, &w);
            cycle_filter_mode(&ctx, &w, direction >= 0);
            update_tabs(&ctx, &w);
            update_mode_label(&ctx, &w);
            update_placeholder(&ctx, &w);
            run_query(&ctx, &w);
        }
    });

    win.on_tab_clicked({
        let ctx = ctx.clone();
        let win = win.as_weak();
        move |index| {
            let Some(w) = win.upgrade() else {
                return;
            };
            exit_submenu_for_query(&ctx, &w);
            tab_clicked(&ctx, &w, index as usize);
            update_tabs(&ctx, &w);
            update_mode_label(&ctx, &w);
            update_placeholder(&ctx, &w);
            run_query(&ctx, &w);
        }
    });

    win.on_dismiss({
        let ctx = ctx.clone();
        let win = win.as_weak();
        move || {
            debug!("slint: dismiss fired");
            let Some(w) = win.upgrade() else {
                slint::quit_event_loop().ok();
                return;
            };
            if ctx.submenu_items.lock().unwrap().is_some() {
                // Esc exits the submenu first, mirroring the GTK frontend.
                exit_submenu_for_query(&ctx, &w);
                update_mode_label(&ctx, &w);
                update_tabs(&ctx, &w);
                update_placeholder(&ctx, &w);
                run_query(&ctx, &w);
            } else {
                slint::quit_event_loop().ok();
            }
        }
    });

    // Initial results, then hand over to the event loop.
    run_query(&ctx, &win);
    debug!("startup: slint window created +{:.1?}", t0.elapsed());
    let _ = win.show();
    if let Some(boot) = BOOT.get() {
        debug!("startup: slint window shown +{:.1?}", boot.elapsed());
    }
    let code = win.run().map(|_| 0).unwrap_or(1);
    std::process::exit(code);
}

// ---- search --------------------------------------------------------------

fn run_query(ctx: &Ctx, win: &LauncherWindow) {
    let raw = win.get_query_text();
    let parsed = ProviderFilter::parse_query(&raw);
    apply_parsed_query(ctx, win, &parsed);

    let max_results = ctx.config.lock().unwrap().general.max_results;
    let is_daemon = matches!(&*ctx.backend.lock().unwrap(), SearchBackend::Daemon(_));

    if is_daemon {
        let rx = ctx.backend.lock().unwrap().query_async(
            &parsed.query,
            max_results,
            &ctx.filter.lock().unwrap(),
            &ctx.config.lock().unwrap(),
            parsed.tag_filter.as_deref(),
        );
        let Some(mut rx) = rx else {
            return;
        };
        let ctx = ctx.clone();
        let win = win.as_weak();
        let raw_at_dispatch = raw.clone();
        std::thread::spawn(move || {
            // futures_channel oneshot is await-only, and this thread has no
            // executor; poll instead. Queries resolve in ~4ms, so a 1ms
            // poll interval is well below perceptual latency.
            let result = loop {
                match rx.try_recv() {
                    Ok(Some(result)) => break result,
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(1)),
                    Err(_) => return,
                }
            };
            let _ = slint::invoke_from_event_loop(move || {
                let Some(w) = win.upgrade() else {
                    return;
                };
                // Staleness guard, mirroring the GTK frontend: drop
                // results whose query the user has already changed.
                if w.get_query_text() != raw_at_dispatch {
                    return;
                }
                apply_results(&ctx, &w, result.items);
            });
        });
    } else {
        let items = ctx.backend.lock().unwrap().search_with_tag(
            &parsed.query,
            max_results,
            &ctx.filter.lock().unwrap(),
            &ctx.config.lock().unwrap(),
            parsed.tag_filter.as_deref(),
        );
        apply_results(ctx, win, items);
    }
}

fn apply_parsed_query(ctx: &Ctx, win: &LauncherWindow, parsed: &ParsedQuery) {
    ctx.filter.lock().unwrap().set_prefix(parsed.prefix.clone());
    update_mode_label(ctx, win);
    update_placeholder(ctx, win);
}

fn apply_results(ctx: &Ctx, win: &LauncherWindow, items: Vec<LaunchItem>) {
    *ctx.items.lock().unwrap() = items;
    let items = ctx.items.lock().unwrap();
    let colors = &ctx.config.lock().unwrap().appearance.colors;
    let accent = colors
        .accent
        .as_deref()
        .and_then(theme::parse_color)
        .unwrap_or(slint::Color::from_argb_encoded(0xffe0a458));

    let rows: Vec<RowData> = items
        .iter()
        .map(|item| {
            let meta = resolve_meta(ctx, &item.provider);
            let badge_color = theme::badge_color_for(&item.provider, colors)
                .and_then(theme::parse_color)
                .unwrap_or(accent);
            RowData {
                title: item.name.clone().into(),
                description: item.description.clone().unwrap_or_default().into(),
                badge: badge_letter(&meta.tab_label).into(),
                badge_color,
            }
        })
        .collect();

    win.set_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
    win.set_selected_index(0);
}

fn badge_letter(label: &str) -> String {
    label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "*".to_string())
}

fn resolve_meta(ctx: &Ctx, provider: &ProviderType) -> provider_meta::ProviderMeta {
    let desc = ctx.descs.get(&provider.to_string());
    provider_meta::resolve(provider, desc)
}

// ---- tabs / labels -------------------------------------------------------

fn update_tabs(ctx: &Ctx, win: &LauncherWindow) {
    let filter = ctx.filter.lock().unwrap();
    let mut tabs = Vec::new();
    if filter.show_all_tab() {
        tabs.push(TabData {
            label: "All".into(),
            active: filter.is_accept_all(),
        });
    }
    for provider in filter.tab_order() {
        tabs.push(TabData {
            label: resolve_meta(ctx, &provider).tab_label.into(),
            active: !filter.is_accept_all() && filter.is_active(provider.clone()),
        });
    }
    win.set_tabs(ModelRc::from(Rc::new(VecModel::from(tabs))));
}

fn update_mode_label(ctx: &Ctx, win: &LauncherWindow) {
    // Mirrors MainWindow::mode_display_name: generic "Plugin" fallback is
    // resolved to the actual provider's tab label.
    let filter = ctx.filter.lock().unwrap();
    let base = filter.mode_display_name().to_string();
    let label = if base == "Plugin" {
        let enabled = filter.enabled_providers();
        if enabled.len() == 1 {
            resolve_meta(ctx, &enabled[0]).tab_label
        } else {
            base
        }
    } else {
        base
    };
    win.set_mode_label(label.into());
}

fn update_placeholder(ctx: &Ctx, win: &LauncherWindow) {
    let nouns: Vec<String> = ctx
        .filter
        .lock()
        .unwrap()
        .enabled_providers()
        .iter()
        .map(|p| resolve_meta(ctx, p).search_noun)
        .collect();
    win.set_placeholder(format!("Search {}...", nouns.join(", ")).into());
}

// ---- filter mode changes (mirror navigation.rs cycle_filter_mode) --------

fn cycle_filter_mode(ctx: &Ctx, win: &LauncherWindow, forward: bool) {
    let tab_order = ctx.filter.lock().unwrap().tab_order();
    if tab_order.is_empty() {
        return;
    }
    let current = ctx.filter.lock().unwrap().enabled_providers();
    let show_all = ctx.filter.lock().unwrap().show_all_tab();
    let all_enabled = show_all && current.len() == tab_order.len();

    let pick = |idx: usize| tab_order[idx].clone();
    if all_enabled {
        let next = if forward { 0 } else { tab_order.len() - 1 };
        ctx.filter.lock().unwrap().set_single_mode(pick(next));
    } else if current.len() == 1 {
        let idx = tab_order.iter().position(|p| p == &current[0]).unwrap_or(0);
        let at_boundary = if forward {
            idx == tab_order.len() - 1
        } else {
            idx == 0
        };
        if at_boundary && !show_all {
            let next = if forward { 0 } else { tab_order.len() - 1 };
            ctx.filter.lock().unwrap().set_single_mode(pick(next));
        } else if at_boundary {
            let mut f = ctx.filter.lock().unwrap();
            for provider in &tab_order {
                f.enable(provider.clone());
            }
            f.restore_all_mode();
        } else {
            let next = if forward { idx + 1 } else { idx - 1 };
            ctx.filter.lock().unwrap().set_single_mode(pick(next));
        }
    } else {
        ctx.filter.lock().unwrap().set_single_mode(pick(0));
    }
    let _ = win;
}

fn tab_clicked(ctx: &Ctx, win: &LauncherWindow, index: usize) {
    let filter = ctx.filter.lock().unwrap();
    let all_index = usize::from(filter.show_all_tab());
    drop(filter);

    if index == 0 && all_index == 1 {
        // The All tab: enable every provider in the tab order and restore
        // the mixed view, mirroring the GTK toggle-all path.
        let tab_order = ctx.filter.lock().unwrap().tab_order();
        let mut f = ctx.filter.lock().unwrap();
        for provider in &tab_order {
            f.enable(provider.clone());
        }
        f.restore_all_mode();
    } else {
        let provider_index = index - all_index;
        let provider = ctx
            .filter
            .lock()
            .unwrap()
            .tab_order()
            .get(provider_index)
            .cloned();
        if let Some(provider) = provider {
            ctx.filter.lock().unwrap().toggle(provider);
        }
    }
    let _ = win;
}

// ---- activation ----------------------------------------------------------

fn activate(ctx: &Ctx, win: &LauncherWindow, index: usize) {
    debug!("slint: activate index={}", index);
    let item = ctx.items.lock().unwrap().get(index).cloned();
    let Some(item) = item else {
        return;
    };

    if let ItemActivation::Submenu { provider_id, data } = &item.activation {
        enter_submenu(ctx, win, &item, provider_id, data);
        return;
    }

    let config = ctx.config.lock().unwrap();
    if launch::handle_activation(&item, &config, &mut ctx.backend.lock().unwrap()) {
        drop(config);
        let _ = win.hide();
        slint::quit_event_loop().ok();
    } else if ctx.submenu_items.lock().unwrap().is_none() {
        // Provider-internal action handled in place: refresh the result set.
        drop(config);
        run_query(ctx, win);
    }
}

fn enter_submenu(
    ctx: &Ctx,
    win: &LauncherWindow,
    item: &LaunchItem,
    provider_id: &str,
    data: &str,
) {
    let resolved = ctx
        .backend
        .lock()
        .unwrap()
        .query_submenu_actions(provider_id, data, &item.name);
    match resolved {
        Some((display_name, actions)) => {
            *ctx.submenu_items.lock().unwrap() = Some(actions.clone());
            *ctx.submenu_title.lock().unwrap() = display_name.clone();
            win.set_mode_label(display_name.into());
            apply_results(ctx, win, actions);
        }
        None => {
            // Nothing to show: fall through to primary activation semantics.
            let config = ctx.config.lock().unwrap();
            if launch::handle_activation(item, &config, &mut ctx.backend.lock().unwrap()) {
                drop(config);
                let _ = win.hide();
                slint::quit_event_loop().ok();
            }
        }
    }
}

fn open_actions(ctx: &Ctx, win: &LauncherWindow, index: usize) {
    debug!("slint: open-actions index={}", index);
    if ctx.submenu_items.lock().unwrap().is_some() {
        return;
    }
    let item = ctx.items.lock().unwrap().get(index).cloned();
    let Some(item) = item else {
        return;
    };
    // Framework-neutral action builder shared with the GTK frontend.
    let actions = crate::ui::actions::for_item(&item);
    if actions.is_empty() {
        return;
    }
    let title = format!("{} — Actions", item.name);
    *ctx.submenu_items.lock().unwrap() = Some(actions.clone());
    *ctx.submenu_title.lock().unwrap() = title.clone();
    win.set_mode_label(title.into());
    apply_results(ctx, win, actions);
}

fn exit_submenu_for_query(ctx: &Ctx, win: &LauncherWindow) {
    if ctx.submenu_items.lock().unwrap().is_some() {
        *ctx.submenu_items.lock().unwrap() = None;
        *ctx.submenu_title.lock().unwrap() = String::new();
        update_mode_label(ctx, win);
    }
}

/// In submenu mode the GTK frontend filters the already-fetched submenu items
/// locally instead of re-querying. The slice filters with a case-insensitive
/// substring match (the GTK path uses the fuzzy matcher); the fetch path is
/// identical.
fn filter_submenu_locally(ctx: &Ctx, win: &LauncherWindow) {
    let raw = win.get_query_text();
    let Some(submenu) = ctx.submenu_items.lock().unwrap().clone() else {
        return;
    };
    let needle = ProviderFilter::parse_query(&raw).query.to_lowercase();
    let filtered: Vec<LaunchItem> = submenu
        .iter()
        .filter(|item| {
            needle.is_empty()
                || item.name.to_lowercase().contains(&needle)
                || item
                    .description
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&needle))
                    .unwrap_or(false)
        })
        .cloned()
        .collect();
    apply_results(ctx, win, filtered);
}
