use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::window::ResonateWindow;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ResonateApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateApplication {
        const NAME: &'static str = "ResonateApplication";
        type Type = super::ResonateApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for ResonateApplication {}

    impl ApplicationImpl for ResonateApplication {
        fn activate(&self) {
            self.parent_activate();
            let app = self.obj();

            // Single-instance: re-activation (or launching again) raises the
            // existing window rather than creating a second one — this is also
            // how the user reopens the app after it has hidden to background.
            if let Some(win) = app.windows().into_iter().next() {
                win.set_visible(true);
                win.present();
                return;
            }

            let window = ResonateWindow::new(&*app);
            // When autostarted with --hidden, create the window (so the effects
            // chain runs) but leave it hidden until the user opens it.
            if !crate::START_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
                window.present();
            }
        }
    }

    impl GtkApplicationImpl for ResonateApplication {}
    impl AdwApplicationImpl for ResonateApplication {}
}

glib::wrapper! {
    pub struct ResonateApplication(ObjectSubclass<imp::ResonateApplication>)
        @extends adw::Application, gtk::Application, gtk::gio::Application,
        @implements gtk::gio::ActionGroup, gtk::gio::ActionMap;
}

impl ResonateApplication {
    pub fn new() -> Self {
        let app: Self = glib::Object::builder()
            .property("application-id", crate::APP_ID)
            .property("flags", gtk::gio::ApplicationFlags::empty())
            .build();
        // GApplication rejects unrecognised options before activate() runs, so
        // --hidden must be registered even though main() reads it from args.
        app.add_main_option(
            "hidden",
            glib::Char::from(0u8),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Start hidden in the background (used by the autostart entry)",
            None,
        );
        app
    }
}
