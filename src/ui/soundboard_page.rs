use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;

use crate::audio::engine::SoundInfo;
use crate::ui::ResonateSoundTile;

#[derive(Clone, Debug)]
pub struct SoundEntry {
    pub name: String,
    pub path: PathBuf,
    pub tile: ResonateSoundTile,
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
        pub playback_progress: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub master_volume_scale: TemplateChild<gtk::Scale>,

        pub sounds: RefCell<Vec<SoundEntry>>,
        pub play_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
        pub cue_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
        pub rename_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
        pub remove_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
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
                playback_progress: Default::default(),
                master_volume_scale: Default::default(),
                sounds: RefCell::new(Vec::new()),
                play_fn: RefCell::new(None),
                cue_fn: RefCell::new(None),
                rename_fn: RefCell::new(None),
                remove_fn: RefCell::new(None),
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

    pub fn set_play_callback<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.imp().play_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_cue_callback<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.imp().cue_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_rename_callback<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.imp().rename_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_remove_callback<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.imp().remove_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_stop_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().stop_all_button.connect_clicked(move |_| f());
    }

    pub fn connect_play_pause<F: Fn() + 'static>(&self, f: F) {
        self.imp().play_pause_button.connect_clicked(move |_| f());
    }

    pub fn connect_skip<F: Fn() + 'static>(&self, f: F) {
        self.imp().skip_button.connect_clicked(move |_| f());
    }

    pub fn set_play_pause_icon(&self, playing: bool) {
        self.imp().play_pause_button.set_icon_name(if playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
    }

    /// Add a sound and wire its tile buttons.
    pub fn add_sound_from_path(&self, path: PathBuf) -> bool {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sound")
            .to_string();

        let n = self.imp().sound_grid.observe_children().n_items() + 1;
        let tile = ResonateSoundTile::new(n, &name);

        // Play button
        let path_p = path.clone();
        tile.connect_play(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().play_fn.borrow().as_ref() {
                    f(path_p.clone());
                }
            }
        ));

        // Cue button
        let path_c = path.clone();
        tile.connect_cue(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().cue_fn.borrow().as_ref() {
                    f(path_c.clone());
                }
            }
        ));

        // Rename menu item
        let path_r = path.clone();
        tile.connect_rename(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().rename_fn.borrow().as_ref() {
                    f(path_r.clone());
                }
            }
        ));

        // Remove menu item
        let path_x = path.clone();
        tile.connect_remove(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().remove_fn.borrow().as_ref() {
                    f(path_x.clone());
                }
            }
        ));

        self.imp().sounds.borrow_mut().push(SoundEntry {
            name,
            path,
            tile: tile.clone(),
        });
        self.imp().sound_grid.insert(&tile, -1);
        self.update_empty_state();
        true
    }

    /// Update the tile label and path after a rename.
    pub fn rename_sound_by_path(&self, old_path: &PathBuf, new_name: String, new_path: PathBuf) {
        let mut sounds = self.imp().sounds.borrow_mut();
        if let Some(entry) = sounds.iter_mut().find(|s| &s.path == old_path) {
            entry.tile.set_name(&new_name);
            entry.name = new_name;
            entry.path = new_path;
        }
    }

    /// Remove a sound tile and return the entry (for undo).
    pub fn remove_sound_by_path(&self, path: &PathBuf) -> Option<SoundEntry> {
        let entry = {
            let mut sounds = self.imp().sounds.borrow_mut();
            let idx = sounds.iter().position(|s| &s.path == path)?;
            sounds.remove(idx)
        };
        // Find and remove the GtkFlowBoxChild containing this tile
        let mut i = 0;
        loop {
            let Some(fbc) = self.imp().sound_grid.child_at_index(i) else {
                break;
            };
            if let Some(child_widget) = fbc.child() {
                if let Ok(t) = child_widget.downcast::<ResonateSoundTile>() {
                    if t == entry.tile {
                        self.imp().sound_grid.remove(&fbc);
                        break;
                    }
                }
            }
            i += 1;
        }
        self.update_empty_state();
        Some(entry)
    }

    /// Update the full-width LCD panel with current play state.
    pub fn update_playback_display(&self, playing: &[SoundInfo], queue: &[String]) {
        let markup = format_playback_markup(playing, queue);
        self.imp().now_playing_label.set_markup(&markup);

        let fraction = playing
            .first()
            .map(|s| s.fraction)
            .unwrap_or(0.0);
        self.imp().playback_progress.set_fraction(fraction);
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

fn escape_markup(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        let t: String = chars[..max.saturating_sub(1)].iter().collect();
        format!("{}…", t)
    } else {
        s.to_string()
    }
}

fn format_playback_markup(playing: &[SoundInfo], queue: &[String]) -> String {
    if playing.is_empty() && queue.is_empty() {
        return "<span foreground='#555555'>Nothing playing</span>".to_string();
    }

    const NAME_MAX: usize = 30;
    const TIME_W: usize = 6; // "-59:59"
    const PAD: usize = 2;

    let mut lines: Vec<String> = Vec::new();

    for info in playing {
        let name = escape_markup(&truncate(&info.name, NAME_MAX));
        let time = match info.remaining_secs {
            Some(0) | None => " —:——".to_string(),
            Some(s) => format!("-{}:{:02}", s / 60, s % 60),
        };
        let name_chars = info.name.chars().count().min(NAME_MAX);
        let spaces = " ".repeat((NAME_MAX + PAD).saturating_sub(name_chars));
        lines.push(format!(
            "<span foreground='#57e389'>♪ {}{}<span font_family='monospace'>{}</span></span>",
            name, spaces, time
        ));
    }

    if !queue.is_empty() {
        let sep = "─".repeat(NAME_MAX + TIME_W + PAD + 2);
        lines.push(format!("<span foreground='#2e2e2e'>{}</span>", sep));
        for (i, name) in queue.iter().enumerate().take(5) {
            let n = escape_markup(&truncate(name, NAME_MAX + TIME_W + PAD));
            lines.push(format!("<span foreground='#666666'>⏭ {}</span>", n));
            if i == 4 && queue.len() > 5 {
                lines.push(format!(
                    "<span foreground='#555555'>  +{} more…</span>",
                    queue.len() - 5
                ));
            }
        }
    }

    lines.join("\n")
}
