use crate::backend::SearchBackend;
use crate::cli::CliArgs;
use crate::client::CoreClient;
use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::paths;
use crate::providers::DmenuProvider;
use crate::providers::{Provider, ProviderManager, ProviderType};
use crate::theme;
use crate::ui::MainWindow;
use gtk4::prelude::*;
use gtk4::{Application, CssProvider, gio};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use log::{debug, info, warn};
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "org.owlry.launcher";

/// Process-start timestamp, set once at the top of `main()`. Lets `on_activate`
/// measure the pre-activate gap (GApplication/GTK/Wayland/D-Bus init) that the
/// `t0`-relative deltas below cannot see. Enable with RUST_LOG=debug.
pub static BOOT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub struct OwlryApp {
    app: Application,
}

impl OwlryApp {
    pub fn new(args: CliArgs) -> Self {
        let app = Application::builder()
            .application_id(APP_ID)
            .flags(gio::ApplicationFlags::FLAGS_NONE)
            .build();

        app.connect_activate(move |app| Self::on_activate(app, &args));

        Self { app }
    }

    pub fn run(&self) -> i32 {
        // Use empty args since clap already parsed our CLI arguments.
        // This prevents GTK from trying to parse --mode, --profile, etc.
        self.app.run_with_args(&[] as &[&str]).into()
    }

    fn on_activate(app: &Application, args: &CliArgs) {
        debug!("Activating Owlry");

        // Startup phase timing. Enable with RUST_LOG=debug to confirm where
        // per-open latency goes (GTK build vs. layer-shell vs. present vs. the
        // initial daemon query). Deltas are cumulative from activate().
        let t0 = std::time::Instant::now();
        if let Some(boot) = BOOT.get() {
            debug!(
                "startup: activate reached +{:.1?} after process boot (GTK/display/D-Bus init)",
                boot.elapsed()
            );
        }

        // Register bundled icon resources
        gio::resources_register_include!("icons.gresource")
            .expect("Failed to register icon resources");
        debug!("startup: gresource registered +{:.1?}", t0.elapsed());

        let config = Rc::new(RefCell::new(Config::load_or_default()));
        debug!("startup: config loaded +{:.1?}", t0.elapsed());

        // Build backend based on mode
        let dmenu_mode = DmenuProvider::has_stdin_data();

        let backend = if dmenu_mode {
            // dmenu mode: local ProviderManager, no daemon
            let mut dmenu = DmenuProvider::new();
            dmenu.enable();
            let core_providers: Vec<Box<dyn Provider>> = vec![Box::new(dmenu)];
            let provider_manager = ProviderManager::new(core_providers, Vec::new());
            let frecency =
                FrecencyStore::with_config(config.borrow().providers.frecency_prune_config());

            SearchBackend::Local {
                providers: Box::new(provider_manager),
                frecency,
            }
        } else {
            // Normal mode: connect to daemon via IPC
            match CoreClient::connect_or_start() {
                Ok(client) => {
                    info!("Connected to owlry-core daemon");
                    SearchBackend::Daemon(crate::backend::DaemonHandle::new(client))
                }
                Err(e) => {
                    warn!(
                        "Failed to connect to daemon ({}), falling back to local providers",
                        e
                    );
                    Self::create_local_backend(&config.borrow())
                }
            }
        };

        debug!("startup: backend ready +{:.1?}", t0.elapsed());

        let backend = Rc::new(RefCell::new(backend));

        // Create filter from CLI args, profile, and config
        let resolved_modes = resolve_modes(args, &config.borrow());
        let show_all_tab = resolved_modes.as_ref().map(|(_, s)| *s).unwrap_or(true);
        let filter = if let Some((modes, _)) = resolved_modes {
            // CLI --mode or --profile specified explicit modes
            let provider_types: Vec<ProviderType> = modes
                .iter()
                .map(|s| ProviderFilter::mode_string_to_provider_type(s))
                .collect();
            let tabs = &config.borrow().general.tabs.clone();
            if provider_types.len() == 1 {
                ProviderFilter::new(
                    Some(provider_types[0].clone()),
                    None,
                    &config.borrow().providers,
                    tabs,
                )
            } else {
                ProviderFilter::new(None, Some(provider_types), &config.borrow().providers, tabs)
            }
        } else {
            let tabs = config.borrow().general.tabs.clone();
            ProviderFilter::new(None, None, &config.borrow().providers, &tabs)
        };

        // A picker profile opens on its first tab rather than the mixed view
        // it was configured to drop.
        let mut filter = filter;
        if !show_all_tab {
            filter.hide_all_tab();
            if let Some(first) = filter.enabled_providers().first().cloned() {
                filter.set_single_mode(first);
            }
        }
        let filter = Rc::new(RefCell::new(filter));
        debug!("startup: filter built +{:.1?}", t0.elapsed());

        let window = MainWindow::new(
            app,
            config.clone(),
            backend.clone(),
            filter.clone(),
            args.prompt.clone(),
        );
        debug!("startup: MainWindow built +{:.1?}", t0.elapsed());

        // Set up layer shell for Wayland overlay behavior
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);

        // Anchor to all edges for centered overlay effect
        // We'll use margins to control the actual size
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, false);

        // Position from top
        window.set_margin(Edge::Top, 200);
        debug!("startup: layer-shell configured +{:.1?}", t0.elapsed());

        // Set up icon theme fallbacks
        Self::setup_icon_theme();
        debug!("startup: icon theme set +{:.1?}", t0.elapsed());

        // Load CSS styling with config for theming
        Self::load_css(&config.borrow());
        debug!("startup: CSS loaded +{:.1?}", t0.elapsed());

        // Fires when the surface is actually mapped by the compositor — the
        // closest signal to "the user sees the window". Captures first-frame /
        // GPU cost that happens after present() returns.
        {
            let t0_map = t0;
            window.connect_map(move |_| {
                debug!(
                    "startup: window mapped (first paint) +{:.1?}",
                    t0_map.elapsed()
                );
            });
        }

        window.present();
        debug!("startup: present() returned +{:.1?}", t0.elapsed());

        // Populate results AFTER present() so the window appears immediately
        window.schedule_initial_results();
        debug!("startup: scheduled initial results +{:.1?}", t0.elapsed());
    }

    /// Create a local backend as fallback when daemon is unavailable.
    ///
    /// Only built-in providers run locally — dmenu mode is the typical use case.
    /// All other providers belong to the daemon (Phase 1 keeps the daemon as the
    /// primary path).
    fn create_local_backend(config: &Config) -> SearchBackend {
        use crate::providers::{ApplicationProvider, CommandProvider};

        let core_providers: Vec<Box<dyn Provider>> = vec![
            Box::new(ApplicationProvider::new()),
            Box::new(CommandProvider::new()),
        ];

        let provider_manager = ProviderManager::new(core_providers, Vec::new());
        let frecency = FrecencyStore::with_config(config.providers.frecency_prune_config());

        SearchBackend::Local {
            providers: Box::new(provider_manager),
            frecency,
        }
    }

    fn setup_icon_theme() {
        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);

            // If the system icon theme doesn't exist on disk (e.g., set in
            // gsettings but not installed), GTK falls back to hicolor which
            // has almost no icons. Detect this and use Adwaita instead.
            let theme_name = icon_theme.theme_name();
            let theme_exists = icon_theme
                .search_path()
                .iter()
                .any(|p| p.join(theme_name.as_str()).is_dir());

            if !theme_exists && theme_name != "hicolor" && theme_name != "Adwaita" {
                info!(
                    "Icon theme '{}' not found on disk, falling back to Adwaita",
                    theme_name
                );
                icon_theme.set_theme_name(Some("Adwaita"));
            }
        }
    }

    fn load_css(config: &Config) {
        let display = gtk4::gdk::Display::default().expect("Could not get default display");

        // 1. Load base structural CSS (always applied)
        let base_provider = CssProvider::new();
        base_provider.load_from_string(include_str!("resources/base.css"));
        gtk4::style_context_add_provider_for_display(
            &display,
            &base_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        debug!("Loaded base structural CSS");

        // 2. Load theme if specified
        if let Some(ref theme_name) = config.appearance.theme {
            let theme_provider = CssProvider::new();
            match theme_name.as_str() {
                "owl" => {
                    theme_provider.load_from_string(include_str!("resources/owl-theme.css"));
                    debug!("Loaded built-in owl theme");
                }
                _ => {
                    // Check for custom theme in $XDG_CONFIG_HOME/owlry/themes/{name}.css
                    if let Some(theme_path) = paths::theme_file(theme_name) {
                        if theme_path.exists() {
                            theme_provider.load_from_path(&theme_path);
                            debug!("Loaded custom theme from {:?}", theme_path);
                        } else {
                            debug!("Theme '{}' not found at {:?}", theme_name, theme_path);
                        }
                    }
                }
            }
            gtk4::style_context_add_provider_for_display(
                &display,
                &theme_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }

        // 3. Load user's custom stylesheet if exists
        if let Some(custom_path) = paths::custom_style_file()
            && custom_path.exists()
        {
            let custom_provider = CssProvider::new();
            custom_provider.load_from_path(&custom_path);
            gtk4::style_context_add_provider_for_display(
                &display,
                &custom_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
            debug!("Loaded custom CSS from {:?}", custom_path);
        }

        // 4. Inject config variables (highest priority for overrides)
        let vars_css = theme::generate_variables_css(&config.appearance);
        let vars_provider = CssProvider::new();
        vars_provider.load_from_string(&vars_css);
        gtk4::style_context_add_provider_for_display(
            &display,
            &vars_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
        debug!("Injected config CSS variables");
    }
}

/// Resolve which modes to enable based on CLI args and config profiles,
/// along with whether the launch keeps the mixed "All" view in its tab cycle.
///
/// Priority: `--mode` > `--profile` > default (all providers from config).
/// Returns `None` when no explicit mode selection was made. Only a profile can
/// drop the All view; `--mode` is single-provider anyway and a bare
/// `--providers` keeps the default behaviour.
fn resolve_modes(args: &CliArgs, config: &Config) -> Option<(Vec<String>, bool)> {
    if let Some(ref mode) = args.mode {
        return Some((vec![mode.to_string()], true));
    }
    if let Some(ref profile_name) = args.profile {
        if let Some(profile) = config.profiles.get(profile_name) {
            return Some((profile.modes.clone(), profile.show_all_tab));
        }
        eprintln!("Unknown profile: {}", profile_name);
        std::process::exit(1);
    }
    None
}
