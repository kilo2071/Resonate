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
            let window = ResonateWindow::new(&*app);
            window.present();
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
        glib::Object::builder()
            .property("application-id", "io.github.kilo2071.Resonate")
            .property("flags", gtk::gio::ApplicationFlags::empty())
            .build()
    }
}
