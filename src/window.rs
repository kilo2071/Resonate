use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::RefCell;
use std::path::PathBuf;

use crate::audio::AudioEngine;
use crate::config::Config;
use crate::ui::{ResonateEffectsPage, ResonateSettingsPage, ResonateSoundTile, ResonateSoundboardPage};

const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "aac", "m4a", "opus", "wma"];

fn is_audio_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
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

        pub config: RefCell<Config>,
        pub audio_engine: RefCell<Option<AudioEngine>>,
    }

    impl Default for ResonateWindow {
        fn default() -> Self {
            Self {
                add_sound_button: Default::default(),
                soundboard_page: Default::default(),
                effects_page: Default::default(),
                settings_page: Default::default(),
                config: RefCell::new(Config::load()),
                audio_engine: RefCell::new(None),
            }
        }
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
            let win = self.obj();

            match AudioEngine::new() {
                Ok(engine) => *self.audio_engine.borrow_mut() = Some(engine),
                Err(e) => log::error!("Audio engine init failed: {}", e),
            }

            win.register_actions();
            win.setup_audio();
            win.sync_settings_ui();
            win.scan_sounds_folder();
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
        @implements gio::ActionGroup, gio::ActionMap,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl ResonateWindow {
    pub fn new(app: &impl IsA<adw::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn register_actions(&self) {
        let add_files = gio::SimpleAction::new("add-files", None);
        add_files.connect_activate(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _| win.open_file_dialog()
        ));
        self.add_action(&add_files);

        let choose_folder = gio::SimpleAction::new("choose-sounds-folder", None);
        choose_folder.connect_activate(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _| win.choose_sounds_folder()
        ));
        self.add_action(&choose_folder);
    }

    fn setup_audio(&self) {
        // Wire tile play buttons → engine
        self.imp().soundboard_page.set_play_callback(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |path| {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Sound")
                    .to_string();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    if let Err(e) = engine.play(&path, &name) {
                        log::error!("Playback failed: {}", e);
                    }
                }
            }
        ));

        // Stop all button
        self.imp().soundboard_page.connect_stop_all(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move || {
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.stop();
                }
                win.imp().soundboard_page.set_now_playing(None, 0.0, None);
            }
        ));

        // Play/pause button
        self.imp().soundboard_page.connect_play_pause(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move || {
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.pause_or_resume();
                }
            }
        ));

        // Progress update timer (every 100 ms)
        glib::timeout_add_local(
            std::time::Duration::from_millis(100),
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    let state = win
                        .imp()
                        .audio_engine
                        .borrow()
                        .as_ref()
                        .and_then(|e| e.playback_progress());

                    let finished = win
                        .imp()
                        .audio_engine
                        .borrow()
                        .as_ref()
                        .map(|e| e.is_finished())
                        .unwrap_or(true);

                    if finished && state.is_none() {
                        win.imp().soundboard_page.set_now_playing(None, 0.0, None);
                    } else if let Some((frac, remaining, name)) = state {
                        win.imp().soundboard_page.set_now_playing(
                            Some(&name),
                            frac,
                            remaining,
                        );
                    }
                    glib::ControlFlow::Continue
                }
            ),
        );
    }

    fn audio_filter() -> gtk::FileFilter {
        let f = gtk::FileFilter::new();
        f.set_name(Some("Audio files"));
        f.add_mime_type("audio/*");
        for ext in AUDIO_EXTENSIONS {
            f.add_suffix(ext);
        }
        f
    }

    fn open_file_dialog(&self) {
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&Self::audio_filter());

        let dialog = gtk::FileDialog::builder()
            .title("Add Sounds")
            .accept_label("Add")
            .filters(&filters)
            .build();

        dialog.open_multiple(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |result| {
                    if let Ok(files) = result {
                        for i in 0..files.n_items() {
                            if let Some(file) = files.item(i).and_downcast::<gio::File>() {
                                if let Some(path) = file.path() {
                                    win.add_sound_file(path);
                                }
                            }
                        }
                    }
                }
            ),
        );
    }

    fn choose_sounds_folder(&self) {
        let dialog = gtk::FileDialog::builder()
            .title("Choose Sounds Folder")
            .accept_label("Select")
            .build();

        dialog.select_folder(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |result| {
                    if let Ok(folder) = result {
                        if let Some(path) = folder.path() {
                            let display = path.to_string_lossy().to_string();
                            win.imp().config.borrow_mut().sounds_folder = path;
                            win.imp().config.borrow().save();
                            win.imp().settings_page.set_sounds_folder_label(&display);
                        }
                    }
                }
            ),
        );
    }

    fn add_sound_file(&self, path: PathBuf) {
        if !is_audio_file(&path) {
            return;
        }
        let config = self.imp().config.borrow().clone();
        let final_path = if config.move_files_to_folder {
            self.move_to_sounds_folder(path, &config.sounds_folder)
        } else {
            path
        };
        self.imp().soundboard_page.add_sound_from_path(final_path);
    }

    /// Scan the sounds folder on startup (files are already there — no moving).
    fn scan_sounds_folder(&self) {
        let folder = self.imp().config.borrow().sounds_folder.clone();
        let Ok(entries) = std::fs::read_dir(&folder) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_audio_file(p))
            .collect();
        paths.sort();
        for path in paths {
            self.imp().soundboard_page.add_sound_from_path(path);
        }
    }

    fn move_to_sounds_folder(&self, src: PathBuf, dest_dir: &PathBuf) -> PathBuf {
        if src.parent().map(|p| p == dest_dir).unwrap_or(false) {
            return src;
        }
        if std::fs::create_dir_all(dest_dir).is_err() {
            return src;
        }
        let Some(file_name) = src.file_name() else {
            return src;
        };
        let dest = dest_dir.join(file_name);
        if std::fs::rename(&src, &dest).is_ok() {
            return dest;
        }
        if std::fs::copy(&src, &dest).is_ok() {
            let _ = std::fs::remove_file(&src);
            return dest;
        }
        src
    }

    fn sync_settings_ui(&self) {
        let config = self.imp().config.borrow().clone();
        let display = config.sounds_folder.to_string_lossy().to_string();
        self.imp().settings_page.set_sounds_folder_label(&display);
        self.imp().settings_page.set_move_files_active(config.move_files_to_folder);

        let settings = self.imp().settings_page.get();
        settings.imp().sounds_folder_row.connect_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.choose_sounds_folder()
        ));

        settings.imp().move_files_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                win.imp().config.borrow_mut().move_files_to_folder = row.is_active();
                win.imp().config.borrow().save();
            }
        ));
    }
}
