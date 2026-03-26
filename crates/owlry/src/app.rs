use crate::cli::CliArgs;
use crate::providers::DmenuProvider;
use crate::theme;
use crate::ui::MainWindow;
use owlry_core::config::Config;
use owlry_core::data::FrecencyStore;
use owlry_core::filter::ProviderFilter;
use owlry_core::paths;
use owlry_core::plugins::native_loader::NativePluginLoader;
#[cfg(feature = "lua")]
use owlry_core::plugins::PluginManager;
use owlry_core::providers::native_provider::NativeProvider;
use owlry_core::providers::Provider;  // For name() method
use owlry_core::providers::{ApplicationProvider, CommandProvider, ProviderManager};
use gtk4::prelude::*;
use gtk4::{gio, Application, CssProvider};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use log::{debug, info, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

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
        // This prevents GTK from trying to parse --mode, --providers, etc.
        self.app.run_with_args(&[] as &[&str]).into()
    }

    fn on_activate(app: &Application, args: &CliArgs) {
        debug!("Activating Owlry");

        // Register bundled icon resources
        gio::resources_register_include!("icons.gresource")
            .expect("Failed to register icon resources");

        let config = Rc::new(RefCell::new(Config::load_or_default()));

        // Load native plugins from /usr/lib/owlry/plugins/
        let native_providers = Self::load_native_plugins(&config.borrow());

        // Build core providers based on mode
        let dmenu_mode = DmenuProvider::has_stdin_data();
        let core_providers: Vec<Box<dyn Provider>> = if dmenu_mode {
            let mut dmenu = DmenuProvider::new();
            dmenu.enable();
            vec![Box::new(dmenu)]
        } else {
            vec![
                Box::new(ApplicationProvider::new()),
                Box::new(CommandProvider::new()),
            ]
        };

        // Create provider manager with core providers and native plugins
        let native_for_manager = if dmenu_mode { Vec::new() } else { native_providers };
        #[cfg(feature = "lua")]
        let mut provider_manager = ProviderManager::new(core_providers, native_for_manager);
        #[cfg(not(feature = "lua"))]
        let provider_manager = ProviderManager::new(core_providers, native_for_manager);

        // Load Lua plugins if enabled (requires lua feature)
        #[cfg(feature = "lua")]
        if config.borrow().plugins.enabled {
            Self::load_lua_plugins(&mut provider_manager, &config.borrow());
        }

        let providers = Rc::new(RefCell::new(provider_manager));
        let frecency = Rc::new(RefCell::new(FrecencyStore::load_or_default()));

        // Create filter from CLI args and config
        let filter = ProviderFilter::new(
            args.mode.clone(),
            args.providers.clone(),
            &config.borrow().providers,
        );
        let filter = Rc::new(RefCell::new(filter));

        let window = MainWindow::new(app, config.clone(), providers.clone(), frecency.clone(), filter.clone(), args.prompt.clone());

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

    /// Load native (.so) plugins from the system plugins directory
    /// Returns NativeProvider instances that can be passed to ProviderManager
    fn load_native_plugins(config: &Config) -> Vec<NativeProvider> {
        let mut loader = NativePluginLoader::new();

        // Set disabled plugins from config
        loader.set_disabled(config.plugins.disabled_plugins.clone());

        // Discover and load plugins
        match loader.discover() {
            Ok(count) => {
                if count == 0 {
                    debug!("No native plugins found in {}",
                           owlry_core::plugins::native_loader::SYSTEM_PLUGINS_DIR);
                    return Vec::new();
                }
                info!("Discovered {} native plugin(s)", count);
            }
            Err(e) => {
                warn!("Failed to discover native plugins: {}", e);
                return Vec::new();
            }
        }

        // Get all plugins and create providers
        let plugins: Vec<Arc<owlry_core::plugins::native_loader::NativePlugin>> =
            loader.into_plugins();

        // Create NativeProvider instances from loaded plugins
        let mut providers = Vec::new();
        for plugin in plugins {
            for provider_info in &plugin.providers {
                let provider = NativeProvider::new(Arc::clone(&plugin), provider_info.clone());
                info!("Created native provider: {} ({})", provider.name(), provider.type_id());
                providers.push(provider);
            }
        }

        info!("Loaded {} provider(s) from native plugins", providers.len());
        providers
    }

    /// Load Lua plugins from the user plugins directory (requires lua feature)
    #[cfg(feature = "lua")]
    fn load_lua_plugins(provider_manager: &mut ProviderManager, config: &Config) {
        let plugins_dir = match paths::plugins_dir() {
            Some(dir) => dir,
            None => {
                warn!("Could not determine plugins directory");
                return;
            }
        };

        // Get owlry version from Cargo.toml at compile time
        let owlry_version = env!("CARGO_PKG_VERSION");

        let mut plugin_manager = PluginManager::new(plugins_dir, owlry_version);

        // Set disabled plugins from config
        plugin_manager.set_disabled(config.plugins.disabled_plugins.clone());

        // Discover plugins
        match plugin_manager.discover() {
            Ok(count) => {
                if count == 0 {
                    debug!("No Lua plugins found");
                    return;
                }
                info!("Discovered {} Lua plugin(s)", count);
            }
            Err(e) => {
                warn!("Failed to discover Lua plugins: {}", e);
                return;
            }
        }

        // Initialize all plugins (load Lua code)
        let init_errors = plugin_manager.initialize_all();
        for error in &init_errors {
            warn!("Plugin initialization error: {}", error);
        }

        // Create providers from initialized plugins
        let plugin_providers = plugin_manager.create_providers();
        let provider_count = plugin_providers.len();

        // Add plugin providers to the main provider manager
        provider_manager.add_providers(plugin_providers);

        if provider_count > 0 {
            info!("Loaded {} provider(s) from Lua plugins", provider_count);
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
