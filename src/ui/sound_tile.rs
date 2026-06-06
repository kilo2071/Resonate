use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/resonate/ui/sound_tile.ui")]
    pub struct ResonateSoundTile {
        #[template_child]
        pub number_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub play_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub cue_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub volume_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub menu_button: TemplateChild<gtk::MenuButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateSoundTile {
        const NAME: &'static str = "ResonateSoundTile";
        type Type = super::ResonateSoundTile;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResonateSoundTile {}
    impl WidgetImpl for ResonateSoundTile {}
    impl BoxImpl for ResonateSoundTile {}
}

glib::wrapper! {
    pub struct ResonateSoundTile(ObjectSubclass<imp::ResonateSoundTile>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ResonateSoundTile {
    pub fn new(number: u32, name: &str) -> Self {
        let tile: Self = glib::Object::builder().build();
        tile.imp().number_label.set_label(&number.to_string());
        tile.imp().name_label.set_label(name);
        tile
    }

    pub fn set_name(&self, name: &str) {
        self.imp().name_label.set_label(name);
    }

    pub fn set_number(&self, n: u32) {
        self.imp().number_label.set_label(&n.to_string());
    }

    pub fn connect_play<F: Fn() + 'static>(&self, f: F) {
        self.imp().play_button.connect_clicked(move |_| f());
    }

    pub fn connect_cue<F: Fn() + 'static>(&self, f: F) {
        self.imp().cue_button.connect_clicked(move |_| f());
    }

    pub fn volume(&self) -> f64 {
        self.imp().volume_scale.value()
    }
}
