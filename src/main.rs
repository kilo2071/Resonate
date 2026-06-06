mod application;
mod audio;
mod plugins;
mod ui;
mod window;

use application::ResonateApplication;
use gtk::glib;
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    env_logger::init();

    gtk::init().expect("Failed to initialize GTK");
    adw::init().expect("Failed to initialize libadwaita");

    let resources = gtk::gio::Resource::load(
        std::path::Path::new(concat!(env!("OUT_DIR"), "/resonate.gresource")),
    )
    .expect("Failed to load gresource bundle");
    gtk::gio::resources_register(&resources);

    // Register app icon from resources
    if let Some(display) = gtk::gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        icon_theme.add_resource_path("/io/github/resonate/icons");

        // Load app stylesheet
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/io/github/resonate/style.css");
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let app = ResonateApplication::new();
    app.run()
}
