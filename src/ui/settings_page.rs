use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/kilo2071/Resonate/ui/settings_page.ui")]
    pub struct ResonateSettingsPage {
        #[template_child]
        pub sounds_folder_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub move_files_row: TemplateChild<adw::SwitchRow>,
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

    pub fn set_sounds_folder_label(&self, path: &str) {
        self.imp().sounds_folder_row.set_subtitle(path);
    }

    pub fn move_files_active(&self) -> bool {
        self.imp().move_files_row.is_active()
    }

    pub fn set_move_files_active(&self, active: bool) {
        self.imp().move_files_row.set_active(active);
    }

    /// Populate input device combo and select the previously saved device.
    /// Returns the device names list (offset 1 = index 0 is "System Default").
    pub fn set_input_device_list(&self, names: &[String], saved: &str) {
        let all: Vec<&str> = std::iter::once("System Default")
            .chain(names.iter().map(|s| s.as_str()))
            .collect();
        let list = gtk::StringList::new(&all);
        self.imp().input_device_row.set_model(Some(&list));
        // Restore previously selected device
        let idx = names.iter().position(|n| n == saved).map(|i| i + 1).unwrap_or(0);
        self.imp().input_device_row.set_selected(idx as u32);
    }

    /// Populate monitor device combo and select the previously saved device.
    pub fn set_monitor_device_list(&self, names: &[String], saved: &str) {
        let all: Vec<&str> = std::iter::once("System Default")
            .chain(names.iter().map(|s| s.as_str()))
            .collect();
        let list = gtk::StringList::new(&all);
        self.imp().monitor_device_row.set_model(Some(&list));
        let idx = names.iter().position(|n| n == saved).map(|i| i + 1).unwrap_or(0);
        self.imp().monitor_device_row.set_selected(idx as u32);
    }

    pub fn set_virtual_device_name(&self, name: &str) {
        self.imp().virtual_device_name_row.set_text(name);
    }

    pub fn set_autostart_virtual_device(&self, active: bool) {
        self.imp().autostart_virtual_device_row.set_active(active);
    }

    pub fn set_monitor_enabled(&self, active: bool) {
        self.imp().monitor_enabled_row.set_active(active);
    }
}

impl Default for ResonateSettingsPage {
    fn default() -> Self {
        Self::new()
    }
}
