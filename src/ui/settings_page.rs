use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/resonate/ui/settings_page.ui")]
    pub struct ResonateSettingsPage {
        #[template_child]
        pub input_device_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub monitor_device_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub monitor_enabled_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub monitor_mic_effects_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub polyphonic_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub stop_on_play_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub default_volume_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub virtual_device_name_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub autostart_virtual_device_row: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateSettingsPage {
        const NAME: &'static str = "ResonateSettingsPage";
        type Type = super::ResonateSettingsPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResonateSettingsPage {}
    impl WidgetImpl for ResonateSettingsPage {}
    impl adw::subclass::prelude::PreferencesPageImpl for ResonateSettingsPage {}
}

glib::wrapper! {
    pub struct ResonateSettingsPage(ObjectSubclass<imp::ResonateSettingsPage>)
        @extends adw::PreferencesPage, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ResonateSettingsPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

impl Default for ResonateSettingsPage {
    fn default() -> Self {
        Self::new()
    }
}
