use crate::cli::CliArgs;
use crate::config::Config;
use crate::filter::ProviderFilter;
use crate::providers::ProviderManager;
use crate::ui::MainWindow;
use gtk4::prelude::*;
use gtk4::{gio, Application};
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

        // Load CSS styling
        Self::load_css();

        window.present();
    }

    fn load_css() {
        let provider = gtk4::CssProvider::new();

        // Try to load from config directory first, then fall back to embedded
        let config_css = dirs::config_dir().map(|p| p.join("owlry").join("style.css"));

        if let Some(css_path) = config_css {
            if css_path.exists() {
                provider.load_from_path(&css_path);
                debug!("Loaded CSS from {:?}", css_path);
            } else {
                provider.load_from_string(include_str!("../resources/style.css"));
                debug!("Loaded embedded CSS");
            }
        } else {
            provider.load_from_string(include_str!("../resources/style.css"));
        }

        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("Could not get default display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
