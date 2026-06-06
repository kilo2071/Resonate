use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;

use crate::ui::ResonateSoundTile;

#[derive(Clone, Debug)]
pub struct SoundEntry {
    pub name: String,
    pub path: PathBuf,
}

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/github/resonate/ui/soundboard_page.ui")]
    pub struct ResonateSoundboardPage {
        #[template_child]
        pub content_stack: TemplateChild<gtk::Stack>,
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

        pub sounds: RefCell<Vec<SoundEntry>>,
        pub play_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
    }

    impl Default for ResonateSoundboardPage {
        fn default() -> Self {
            Self {
                content_stack: Default::default(),
                sound_grid: Default::default(),
                play_pause_button: Default::default(),
                skip_button: Default::default(),
                stop_all_button: Default::default(),
                now_playing_label: Default::default(),
                time_remaining_label: Default::default(),
                playback_progress: Default::default(),
                master_volume_scale: Default::default(),
                sounds: RefCell::new(Vec::new()),
                play_fn: RefCell::new(None),
            }
        }
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

    impl ObjectImpl for ResonateSoundboardPage {}
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

    /// Register a closure to call when any tile's play button is pressed.
    pub fn set_play_callback<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.imp().play_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_stop_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().stop_all_button.connect_clicked(move |_| f());
    }

    pub fn connect_play_pause<F: Fn() + 'static>(&self, f: F) {
        self.imp().play_pause_button.connect_clicked(move |_| f());
    }

    /// Add a sound. Connects the tile's play button to the registered callback.
    pub fn add_sound_from_path(&self, path: PathBuf) -> bool {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sound")
            .to_string();

        self.imp().sounds.borrow_mut().push(SoundEntry {
            name: name.clone(),
            path: path.clone(),
        });

        let n = self.imp().sound_grid.observe_children().n_items() + 1;
        let tile = ResonateSoundTile::new(n, &name);

        let path_clone = path.clone();
        tile.connect_play(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().play_fn.borrow().as_ref() {
                    f(path_clone.clone());
                }
            }
        ));

        self.imp().sound_grid.insert(&tile, -1);
        self.update_empty_state();
        true
    }

    pub fn set_now_playing(&self, name: Option<&str>, progress: f64, secs_remaining: Option<u32>) {
        let imp = self.imp();
        match name {
            Some(n) => {
                imp.now_playing_label.set_label(n);
                let time_str = match secs_remaining {
                    Some(0) | None => "—:——".to_string(),
                    Some(s) => format!("-{}:{:02}", s / 60, s % 60),
                };
                imp.time_remaining_label.set_label(&time_str);
            }
            None => {
                imp.now_playing_label.set_label("Nothing playing");
                imp.time_remaining_label.set_label("—:——");
            }
        }
        imp.playback_progress.set_fraction(progress);
    }

    fn update_empty_state(&self) {
        let n = self.imp().sound_grid.observe_children().n_items();
        self.imp()
            .content_stack
            .set_visible_child_name(if n == 0 { "empty" } else { "grid" });
    }
}

impl Default for ResonateSoundboardPage {
    fn default() -> Self {
        Self::new()
    }
}
