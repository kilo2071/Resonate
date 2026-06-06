use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/resonate/ui/effects_page.ui")]
    pub struct ResonateEffectsPage {
        #[template_child]
        pub add_effect_sheet: TemplateChild<adw::BottomSheet>,
        #[template_child]
        pub add_effect_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub effects_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub settings_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub effect_params_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub available_effects_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub effect_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub input_source_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub chain_empty_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateEffectsPage {
        const NAME: &'static str = "ResonateEffectsPage";
        type Type = super::ResonateEffectsPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResonateEffectsPage {
        fn constructed(&self) {
            self.parent_constructed();

            let sheet = self.add_effect_sheet.clone();
            self.add_effect_button.connect_clicked(move |_| {
                sheet.set_open(true);
            });

            self.obj().populate_available_effects();
        }
    }

    impl WidgetImpl for ResonateEffectsPage {}
    impl BoxImpl for ResonateEffectsPage {}
}

glib::wrapper! {
    pub struct ResonateEffectsPage(ObjectSubclass<imp::ResonateEffectsPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ResonateEffectsPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn populate_available_effects(&self) {
        let effects = [
            ("Noise Gate", "Cuts background noise below a threshold"),
            ("Equalizer", "Adjust frequency bands"),
            ("Compressor", "Even out volume dynamics"),
            ("Reverb", "Add room ambience"),
            ("Noise Suppression", "AI-based background noise removal"),
            ("Pitch Shift", "Shift pitch up or down"),
            ("Gain", "Simple volume boost or cut"),
            ("De-Esser", "Reduce harsh sibilance"),
        ];

        let list = &self.imp().available_effects_list;
        for (name, desc) in &effects {
            let row = adw::ActionRow::builder()
                .title(*name)
                .subtitle(*desc)
                .activatable(true)
                .build();
            let add_btn = gtk::Button::builder()
                .icon_name("list-add-symbolic")
                .tooltip_text("Add to chain")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build();
            row.add_suffix(&add_btn);
            list.append(&row);
        }
    }
}

impl Default for ResonateEffectsPage {
    fn default() -> Self {
        Self::new()
    }
}
