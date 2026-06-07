mod application;
mod audio;
mod config;
mod plugins;
mod tray;
mod ui;
mod window;

use application::ResonateApplication;
use gtk::glib;
use gtk::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Application id; also the basename of the desktop entry and the icon name.
pub const APP_ID: &str = "io.github.kilo2071.Resonate";

/// Set when launched with `--hidden` (e.g. from the autostart entry): the window
/// is created so the effects run, but is not shown until the user opens it.
pub static START_HIDDEN: AtomicBool = AtomicBool::new(false);

fn main() -> glib::ExitCode {
    env_logger::init();

    if std::env::args().any(|a| a == "--hidden") {
        START_HIDDEN.store(true, Ordering::Relaxed);
    }

    gtk::init().expect("Failed to initialize GTK");
    adw::init().expect("Failed to initialize libadwaita");
    glib::set_application_name("Resonate");

    let resources = gtk::gio::Resource::load(
        std::path::Path::new(concat!(env!("OUT_DIR"), "/resonate.gresource")),
    )
    .expect("Failed to load gresource bundle");
    gtk::gio::resources_register(&resources);

    gtk::Window::set_default_icon_name(APP_ID);

    if let Some(display) = gtk::gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        icon_theme.add_resource_path("/io/github/kilo2071/Resonate/icons");

        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/io/github/kilo2071/Resonate/style.css");
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let app = ResonateApplication::new();
    app.run()
}
