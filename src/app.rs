use crate::cli::CliArgs;
use crate::config::Config;
use crate::filter::ProviderFilter;
use crate::providers::ProviderManager;
use crate::theme;
use crate::ui::MainWindow;
use gtk4::prelude::*;
use gtk4::{gio, Application, CssProvider};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use log::debug;
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
        self.app.run().into()
    }

    fn on_activate(app: &Application, args: &CliArgs) {
        debug!("Activating Owlry");

        let config = Rc::new(RefCell::new(Config::load_or_default()));
        let providers = Rc::new(RefCell::new(ProviderManager::new()));

        // Create filter from CLI args and config
        let filter = ProviderFilter::new(
            args.mode,
            args.providers.clone(),
            &config.borrow().providers,
        );
        let filter = Rc::new(RefCell::new(filter));

        let window = MainWindow::new(app, config.clone(), providers.clone(), filter.clone());

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

        // Load CSS styling with config for theming
        Self::load_css(&config.borrow());

        window.present();
    }

    fn load_css(config: &Config) {
        let display = gtk4::gdk::Display::default().expect("Could not get default display");

        // 1. Load base structural CSS (always applied)
        let base_provider = CssProvider::new();
        base_provider.load_from_string(include_str!("../resources/base.css"));
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
                    theme_provider.load_from_string(include_str!("../resources/owl-theme.css"));
                    debug!("Loaded built-in owl theme");
                }
                _ => {
                    // Check for custom theme in ~/.config/owlry/themes/{name}.css
                    if let Some(theme_path) = dirs::config_dir()
                        .map(|p| p.join("owlry").join("themes").join(format!("{}.css", theme_name)))
                    {
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
        if let Some(custom_path) = dirs::config_dir().map(|p| p.join("owlry").join("style.css")) {
            if custom_path.exists() {
                let custom_provider = CssProvider::new();
                custom_provider.load_from_path(&custom_path);
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &custom_provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_USER,
                );
                debug!("Loaded custom CSS from {:?}", custom_path);
            }
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
