use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use crate::ui::ResonateSoundTile;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/resonate/ui/soundboard_page.ui")]
    pub struct ResonateSoundboardPage {
        #[template_child]
        pub sound_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub play_pause_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub skip_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub stop_all_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub now_playing_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub time_remaining_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub playback_progress: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub master_volume_scale: TemplateChild<gtk::Scale>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateSoundboardPage {
        const NAME: &'static str = "ResonateSoundboardPage";
        type Type = super::ResonateSoundboardPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            ResonateSoundTile::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResonateSoundboardPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_placeholder_tiles();
        }
    }

    impl WidgetImpl for ResonateSoundboardPage {}
    impl BoxImpl for ResonateSoundboardPage {}
}

glib::wrapper! {
    pub struct ResonateSoundboardPage(ObjectSubclass<imp::ResonateSoundboardPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ResonateSoundboardPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn add_tile(&self, name: &str) {
        let grid = &self.imp().sound_grid;
        let n = grid.observe_children().n_items() + 1;
        let tile = ResonateSoundTile::new(n, name);
        grid.insert(&tile, -1);
    }

    pub fn set_now_playing(&self, name: Option<&str>, progress: f64, secs_remaining: Option<u32>) {
        let imp = self.imp();
        match name {
            Some(n) => {
                imp.now_playing_label.set_label(n);
                let time_str = secs_remaining
                    .map(|s| format!("-{}:{:02}", s / 60, s % 60))
                    .unwrap_or_else(|| "—".to_string());
                imp.time_remaining_label.set_label(&time_str);
            }
            None => {
                imp.now_playing_label.set_label("Nothing playing");
                imp.time_remaining_label.set_label("—");
            }
        }
        imp.playback_progress.set_fraction(progress);
    }

    fn setup_placeholder_tiles(&self) {
        let samples = [
            "Intro Jingle",
            "Applause",
            "Drum Roll",
            "Sad Trombone",
            "Air Horn",
            "Rimshot",
        ];
        for name in &samples {
            self.add_tile(name);
        }
    }
}

impl Default for ResonateSoundboardPage {
    fn default() -> Self {
        Self::new()
    }
}
