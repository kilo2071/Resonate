use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use crate::ui::{ResonateEffectsPage, ResonateSettingsPage, ResonateSoundTile, ResonateSoundboardPage};

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/resonate/ui/window.ui")]
    pub struct ResonateWindow {
        #[template_child]
        pub add_sound_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub soundboard_page: TemplateChild<ResonateSoundboardPage>,
        #[template_child]
        pub effects_page: TemplateChild<ResonateEffectsPage>,
        #[template_child]
        pub settings_page: TemplateChild<ResonateSettingsPage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateWindow {
        const NAME: &'static str = "ResonateWindow";
        type Type = super::ResonateWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            ResonateSoundTile::ensure_type();
            ResonateSoundboardPage::ensure_type();
            ResonateEffectsPage::ensure_type();
            ResonateSettingsPage::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResonateWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_actions();
        }
    }

    impl WidgetImpl for ResonateWindow {}
    impl WindowImpl for ResonateWindow {}
    impl ApplicationWindowImpl for ResonateWindow {}
    impl AdwApplicationWindowImpl for ResonateWindow {}
}

glib::wrapper! {
    pub struct ResonateWindow(ObjectSubclass<imp::ResonateWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::gio::ActionGroup, gtk::gio::ActionMap,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl ResonateWindow {
    pub fn new(app: &impl IsA<adw::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn setup_actions(&self) {
        let imp = self.imp();
        let soundboard = imp.soundboard_page.get();
        imp.add_sound_button.connect_clicked(move |_| {
            // TODO: open file chooser; for now add a placeholder
            let n = soundboard
                .imp()
                .sound_grid
                .observe_children()
                .n_items() + 1;
            soundboard.add_tile(&format!("Sound {}", n));
        });
    }
}
