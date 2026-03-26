use crate::backend::SearchBackend;
use crate::cli::CliArgs;
use crate::client::CoreClient;
use crate::providers::DmenuProvider;
use crate::theme;
use crate::ui::MainWindow;
use owlry_core::config::Config;
use owlry_core::data::FrecencyStore;
use owlry_core::filter::ProviderFilter;
use owlry_core::paths;
use owlry_core::providers::{Provider, ProviderManager, ProviderType};
use gtk4::prelude::*;
use gtk4::{gio, Application, CssProvider};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use log::{debug, info, warn};
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "org.owlry.launcher";

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

        // Register bundled icon resources
        gio::resources_register_include!("icons.gresource")
            .expect("Failed to register icon resources");

        let config = Rc::new(RefCell::new(Config::load_or_default()));

        // Build backend based on mode
        let dmenu_mode = DmenuProvider::has_stdin_data();

        let backend = if dmenu_mode {
            // dmenu mode: local ProviderManager, no daemon
            let mut dmenu = DmenuProvider::new();
            dmenu.enable();
            let core_providers: Vec<Box<dyn Provider>> = vec![Box::new(dmenu)];
            let provider_manager = ProviderManager::new(core_providers, Vec::new());
            let frecency = FrecencyStore::load_or_default();

            SearchBackend::Local {
                providers: provider_manager,
                frecency,
            }
        } else {
            // Normal mode: connect to daemon via IPC
            match CoreClient::connect_or_start() {
                Ok(client) => {
                    info!("Connected to owlry-core daemon");
                    SearchBackend::Daemon(client)
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

        let backend = Rc::new(RefCell::new(backend));

        // Create filter from CLI args, profile, and config
        let resolved_modes = resolve_modes(args, &config.borrow());
        let filter = if let Some(modes) = resolved_modes {
            // CLI --mode or --profile specified explicit modes
            let provider_types: Vec<ProviderType> = modes
                .iter()
                .map(|s| ProviderFilter::mode_string_to_provider_type(s))
                .collect();
            if provider_types.len() == 1 {
                ProviderFilter::new(
                    Some(provider_types[0].clone()),
                    None,
                    &config.borrow().providers,
                )
            } else {
                ProviderFilter::new(
                    None,
                    Some(provider_types),
                    &config.borrow().providers,
                )
            }
        } else {
            ProviderFilter::new(None, None, &config.borrow().providers)
        };
        let filter = Rc::new(RefCell::new(filter));

        let window = MainWindow::new(
            app,
            config.clone(),
            backend.clone(),
            filter.clone(),
            args.prompt.clone(),
        );

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

        // Set up icon theme fallbacks
        Self::setup_icon_theme();

        // Load CSS styling with config for theming
        Self::load_css(&config.borrow());

        window.present();
    }

    /// Create a local backend as fallback when daemon is unavailable.
    /// Loads native plugins and creates providers locally.
    fn create_local_backend(config: &Config) -> SearchBackend {
        use owlry_core::plugins::native_loader::NativePluginLoader;
        use owlry_core::providers::native_provider::NativeProvider;
        use owlry_core::providers::{ApplicationProvider, CommandProvider};
        use std::sync::Arc;

        // Load native plugins
        let mut loader = NativePluginLoader::new();
        loader.set_disabled(config.plugins.disabled_plugins.clone());

        let native_providers: Vec<NativeProvider> = match loader.discover() {
            Ok(count) if count > 0 => {
                info!("Discovered {} native plugin(s) for local fallback", count);
                let plugins: Vec<Arc<owlry_core::plugins::native_loader::NativePlugin>> =
                    loader.into_plugins();
                let mut providers = Vec::new();
                for plugin in plugins {
                    for provider_info in &plugin.providers {
                        let provider =
                            NativeProvider::new(Arc::clone(&plugin), provider_info.clone());
                        providers.push(provider);
                    }
                }
                providers
            }
            _ => Vec::new(),
        };

        let core_providers: Vec<Box<dyn Provider>> = vec![
            Box::new(ApplicationProvider::new()),
            Box::new(CommandProvider::new()),
        ];

        let provider_manager = ProviderManager::new(core_providers, native_providers);
        let frecency = FrecencyStore::load_or_default();

        SearchBackend::Local {
            providers: provider_manager,
            frecency,
        }
    }

    fn setup_icon_theme() {
        // Ensure we have icon fallbacks for weather/media icons
        // These may not exist in all icon themes
        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);

            // Add Adwaita as fallback search path (has weather and media icons)
            icon_theme.add_search_path("/usr/share/icons/Adwaita");
            icon_theme.add_search_path("/usr/share/icons/breeze");

            debug!("Icon theme search paths configured with Adwaita/breeze fallbacks");
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
            && custom_path.exists() {
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

/// Resolve which modes to enable based on CLI args and config profiles.
///
/// Priority: `--mode` > `--profile` > default (all providers from config).
/// Returns `None` when no explicit mode selection was made.
fn resolve_modes(args: &CliArgs, config: &Config) -> Option<Vec<String>> {
    if let Some(ref mode) = args.mode {
        return Some(vec![mode.to_string()]);
    }
    if let Some(ref profile_name) = args.profile {
        if let Some(profile) = config.profiles.get(profile_name) {
            return Some(profile.modes.clone());
        }
        eprintln!("Unknown profile: {}", profile_name);
        std::process::exit(1);
    }
    None
}
