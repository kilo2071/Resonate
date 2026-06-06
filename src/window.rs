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
    #[template(resource = "/io/github/kilo2071/Resonate/ui/window.ui")]
    pub struct ResonateWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
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
                toast_overlay: Default::default(),
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
                Ok(mut engine) => {
                    let cfg = self.config.borrow().clone();
                    engine.set_polyphonic(cfg.polyphonic);
                    engine.set_monitor_volume(cfg.monitor_volume);
                    engine.set_monitor_enabled(cfg.monitor_enabled);
                    engine.set_mic_volume(cfg.mic_volume);
                    *self.audio_engine.borrow_mut() = Some(engine);
                }
                Err(e) => log::error!("Audio engine init failed: {}", e),
            }

            // Start virtual mic device
            {
                let cfg = self.config.borrow().clone();
                if cfg.virtual_device_enabled {
                    // Build the mic-effects chain and share it with the PW thread.
                    let effects = match self.audio_engine.borrow().as_ref() {
                        Some(e) => {
                            e.rebuild_effects(&cfg.effects_chain);
                            e.effects_handle()
                        }
                        None => std::sync::Arc::new(std::sync::Mutex::new(
                            crate::plugins::host::PluginChain::new(),
                        )),
                    };
                    match crate::audio::virtual_device::start(
                        &cfg.virtual_device_name,
                        &cfg.input_device_name,
                        effects,
                        cfg.mic_volume,
                    ) {
                        Ok(dev) => {
                            if let Some(engine) = self.audio_engine.borrow_mut().as_mut() {
                                engine.set_virtual_device(dev);
                            }
                            // After PipeWire registers the node (~1 s), set it as default input.
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(1500),
                                glib::clone!(
                                    #[weak]
                                    win,
                                    move || {
                                        win.set_virtual_mic_as_default_input();
                                    }
                                ),
                            );
                        }
                        Err(e) => log::error!("Virtual device start failed: {}", e),
                    }
                }
            }

            win.register_actions();
            win.setup_audio();
            win.setup_effects();
            win.sync_settings_ui();
            win.scan_sounds_folder();

            // On close, stop our PipeWire thread (drops the bridge + mic streams).
            // The offline mic pass-through is provided by the drop-in at next login.
            win.connect_close_request(glib::clone!(
                #[weak(rename_to = w)]
                win,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_| {
                    if let Some(e) = w.imp().audio_engine.borrow_mut().as_mut() {
                        e.virtual_device = None;
                    }
                    glib::Propagation::Proceed
                }
            ));
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
        let page = self.imp().soundboard_page.get();

        // Set slider positions from config
        {
            let cfg = self.imp().config.borrow().clone();
            page.set_initial_volumes(cfg.mic_volume, cfg.monitor_volume);
        }

        // Mic / virtual device volume slider
        page.connect_mic_volume_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |v| {
                win.imp().config.borrow_mut().mic_volume = v;
                win.imp().config.borrow().save();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.set_mic_volume(v);
                }
            }
        ));

        // Monitor (headphone) volume slider
        page.connect_monitor_volume_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |v| {
                win.imp().config.borrow_mut().monitor_volume = v;
                win.imp().config.borrow().save();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.set_monitor_volume(v);
                }
            }
        ));

        // Play button on tile → polyphonic/sequential start
        page.set_play_callback(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |path| {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sound").to_string();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    if let Err(e) = engine.play(&path, &name) {
                        log::error!("Playback failed: {}", e);
                    }
                }
            }
        ));

        // Cue button → always queue regardless of polyphonic mode
        page.set_cue_callback(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |path| {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sound").to_string();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.cue(&path, &name);
                }
            }
        ));

        // Rename / Remove callbacks
        page.set_rename_callback(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |path| win.show_rename_dialog(path)
        ));

        page.set_remove_callback(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |path| win.confirm_remove_sound(path)
        ));

        // Stop all
        page.connect_stop_all(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move || {
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.stop_all();
                }
                let sb = win.imp().soundboard_page.get();
                sb.update_playback_display(&[], &[]);
                sb.set_play_pause_sensitive(false);
                sb.set_play_pause_icon(false);
            }
        ));

        // Play/pause toggle in bar
        page.connect_play_pause(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move || {
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.toggle_pause();
                }
            }
        ));

        // Skip: remove first queued item and try to start it immediately
        page.connect_skip(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move || {
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.skip_queue();
                }
            }
        ));

        // Progress timer — 100 ms
        glib::timeout_add_local(
            std::time::Duration::from_millis(100),
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    let (playing, queue) = {
                        if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                            engine.tick()
                        } else {
                            (vec![], vec![])
                        }
                    };

                    let sb = win.imp().soundboard_page.get();
                    sb.update_playback_display(&playing, &queue);
                    sb.set_play_pause_sensitive(!playing.is_empty());

                    let active = win
                        .imp()
                        .audio_engine
                        .borrow()
                        .as_ref()
                        .map(|e| e.is_anything_playing() && !e.is_all_paused())
                        .unwrap_or(false);
                    sb.set_play_pause_icon(active);

                    glib::ControlFlow::Continue
                }
            ),
        );
    }

    // ── File picker ──────────────────────────────────────────────────────────

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

    // ── Rename ───────────────────────────────────────────────────────────────

    fn show_rename_dialog(&self, path: PathBuf) {
        let current_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let entry = gtk::Entry::builder()
            .text(&current_name)
            .placeholder_text("New name")
            .activates_default(true)
            .margin_top(8)
            .margin_bottom(4)
            .margin_start(4)
            .margin_end(4)
            .build();

        let dialog = adw::AlertDialog::new(
            Some("Rename Sound"),
            Some("Enter a new name for this sound."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        dialog.set_extra_child(Some(&entry));

        dialog.connect_response(
            None,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[weak]
                entry,
                move |_, response| {
                    if response == "rename" {
                        let new_name = entry.text().trim().to_string();
                        if !new_name.is_empty() && new_name != current_name {
                            win.do_rename_sound(path.clone(), new_name);
                        }
                    }
                }
            ),
        );

        dialog.present(Some(self));
    }

    fn do_rename_sound(&self, old_path: PathBuf, new_name: String) {
        let ext = old_path.extension().map(|e| e.to_os_string());
        let parent = old_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();

        let mut new_path = parent.join(&new_name);
        if let Some(e) = ext {
            new_path.set_extension(e);
        }

        if let Err(e) = std::fs::rename(&old_path, &new_path) {
            log::error!("Rename failed: {}", e);
            let toast = adw::Toast::new(&format!("Rename failed: {e}"));
            self.imp().toast_overlay.add_toast(toast);
            return;
        }

        self.imp()
            .soundboard_page
            .rename_sound_by_path(&old_path, new_name.clone(), new_path);

        let toast = adw::Toast::new(&format!("Renamed to \"{new_name}\""));
        self.imp().toast_overlay.add_toast(toast);
    }

    // ── Remove + undo ────────────────────────────────────────────────────────

    fn confirm_remove_sound(&self, path: PathBuf) {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sound")
            .to_string();

        let dialog = adw::AlertDialog::new(
            Some("Remove Sound"),
            Some(&format!(
                "\"{name}\" will be permanently deleted from your Sounds Folder."
            )),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        dialog.connect_response(
            None,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_, response| {
                    if response == "delete" {
                        win.do_remove_sound(path.clone(), name.clone());
                    }
                }
            ),
        );

        dialog.present(Some(self));
    }

    fn do_remove_sound(&self, path: PathBuf, name: String) {
        if let Some(engine) = self.imp().audio_engine.borrow_mut().as_mut() {
            engine.stop_sound_by_path(&path);
        }

        // Use as_millis() for a unique timestamp (subsec_millis repeats every second)
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let temp_path = std::env::temp_dir().join(format!("resonate_undo_{ts}"));

        // rename(2) fails with EXDEV across filesystems (/tmp is tmpfs, sounds folder is ext4).
        // Fall back to copy + delete so the file always leaves the Sounds Folder.
        let moved = std::fs::rename(&path, &temp_path).is_ok()
            || (std::fs::copy(&path, &temp_path).is_ok()
                && std::fs::remove_file(&path).is_ok());

        if !moved {
            log::error!("Could not move '{}' to temp; deleting directly", path.display());
            let _ = std::fs::remove_file(&path);
        }

        self.imp().soundboard_page.remove_sound_by_path(&path);

        let toast = adw::Toast::builder()
            .title(format!("Removed \"{name}\""))
            .button_label("Undo")
            .timeout(6)
            .build();

        let path_restore = path.clone();
        let temp_restore = temp_path.clone();
        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                let restored = std::fs::rename(&temp_restore, &path_restore).is_ok()
                    || (std::fs::copy(&temp_restore, &path_restore).is_ok()
                        && { let _ = std::fs::remove_file(&temp_restore); true });
                if restored {
                    win.imp().soundboard_page.add_sound_from_path(path_restore.clone());
                }
            }
        ));

        let temp_for_dismiss = temp_path.clone();
        toast.connect_dismissed(move |_| {
            if temp_for_dismiss.exists() {
                let _ = std::fs::remove_file(&temp_for_dismiss);
            }
        });

        self.imp().toast_overlay.add_toast(toast);
    }

    // ── Effects ───────────────────────────────────────────────────────────────

    fn setup_effects(&self) {
        let chain = self.imp().config.borrow().effects_chain.clone();
        let page = self.imp().effects_page.get();
        page.init_chain(&chain);

        // Relay parameter changes to the engine (and save to config).
        page.connect_effect_enabled(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |idx, enabled| {
                {
                    let mut cfg = win.imp().config.borrow_mut();
                    if let Some(e) = cfg.effects_chain.get_mut(idx) {
                        e.enabled = enabled;
                    }
                }
                win.imp().config.borrow().save();
                if let Some(engine) = win.imp().audio_engine.borrow().as_ref() {
                    engine.set_effect_enabled(idx, enabled);
                }
            }
        ));

        page.connect_effect_param(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |idx, param, value| {
                {
                    let mut cfg = win.imp().config.borrow_mut();
                    if let Some(e) = cfg.effects_chain.get_mut(idx) {
                        e.params.insert(param.clone(), value);
                    }
                }
                win.imp().config.borrow().save();
                if let Some(engine) = win.imp().audio_engine.borrow().as_ref() {
                    engine.set_effect_param(idx, &param, value);
                }
            }
        ));

        // Supplies live parameter metadata so the panel works for LV2 too.
        page.connect_param_provider(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or_default]
            move |idx| {
                win.imp()
                    .audio_engine
                    .borrow()
                    .as_ref()
                    .map(|e| e.effect_params(idx))
                    .unwrap_or_default()
            }
        ));

        // Add a new effect: append to config, rebuild the live chain, refresh rows.
        page.connect_effect_add(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |id| {
                let entry = default_entry_for(&id);
                {
                    let mut cfg = win.imp().config.borrow_mut();
                    cfg.effects_chain.push(entry);
                }
                win.rebuild_and_refresh_effects();
            }
        ));

        // Remove an effect by index.
        page.connect_effect_remove(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |idx| {
                {
                    let mut cfg = win.imp().config.borrow_mut();
                    if idx < cfg.effects_chain.len() {
                        cfg.effects_chain.remove(idx);
                    }
                }
                win.rebuild_and_refresh_effects();
            }
        ));
    }

    /// Persist the chain, rebuild the live engine chain, and refresh the rows.
    fn rebuild_and_refresh_effects(&self) {
        let chain = self.imp().config.borrow().effects_chain.clone();
        self.imp().config.borrow().save();
        if let Some(engine) = self.imp().audio_engine.borrow().as_ref() {
            engine.rebuild_effects(&chain);
        }
        self.imp().effects_page.get().init_chain(&chain);
    }

    // ── Virtual mic default ───────────────────────────────────────────────────

    fn set_virtual_mic_as_default_input(&self) {
        use crate::audio::virtual_device::SOURCE_NAME;
        let nodes = crate::audio::virtual_device::enumerate_nodes();
        let Some(node) = nodes
            .iter()
            .find(|n| n.name == SOURCE_NAME)
            .or_else(|| {
                nodes
                    .iter()
                    .find(|n| n.media_class.contains("Source") && n.description.contains("Resonate"))
            })
        else {
            log::warn!("set_virtual_mic_as_default: Resonate source node not found");
            return;
        };
        let id_str = node.id.to_string();
        match std::process::Command::new("wpctl")
            .args(["set-default", &id_str])
            .status()
        {
            Ok(s) if s.success() => {
                log::info!("Set '{}' (id={}) as default audio input", node.description, id_str);
            }
            Ok(s) => log::warn!("wpctl set-default {} exited with {}", id_str, s),
            Err(e) => log::warn!("wpctl not found or failed: {}", e),
        }
    }

    // ── Settings sync ────────────────────────────────────────────────────────

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

        // Polyphonic toggle → update engine
        settings.imp().polyphonic_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                let v = row.is_active();
                win.imp().config.borrow_mut().polyphonic = v;
                win.imp().config.borrow().save();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.set_polyphonic(v);
                }
            }
        ));

        // Monitor enabled toggle
        settings.set_monitor_enabled(config.monitor_enabled);
        settings.imp().monitor_enabled_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                let v = row.is_active();
                win.imp().config.borrow_mut().monitor_enabled = v;
                win.imp().config.borrow().save();
                if let Some(engine) = win.imp().audio_engine.borrow_mut().as_mut() {
                    engine.set_monitor_enabled(v);
                }
            }
        ));

        // Virtual device autostart
        settings.set_autostart_virtual_device(config.virtual_device_enabled);
        settings.imp().autostart_virtual_device_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                win.imp().config.borrow_mut().virtual_device_enabled = row.is_active();
                win.imp().config.borrow().save();
            }
        ));

        // Virtual device name — save on change
        settings.set_virtual_device_name(&config.virtual_device_name);
        settings.imp().virtual_device_name_row.connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                use gtk::prelude::EditableExt;
                win.imp().config.borrow_mut().virtual_device_name = row.text().to_string();
                win.imp().config.borrow().save();
            }
        ));

        // Enumerate audio devices and populate combo rows
        let nodes = crate::audio::virtual_device::enumerate_nodes();
        let sources: Vec<String> = nodes
            .iter()
            .filter(|n| n.media_class.contains("Source") && !n.name.starts_with("resonate"))
            .map(|n| n.description.clone())
            .collect();
        let sinks: Vec<String> = nodes
            .iter()
            .filter(|n| n.media_class.contains("Sink") && !n.name.starts_with("resonate"))
            .map(|n| n.description.clone())
            .collect();
        settings.set_input_device_list(&sources, &config.input_device_name);
        settings.set_monitor_device_list(&sinks, &config.monitor_device_name);

        // Save device selection when changed
        let sources_for_cb = sources.clone();
        settings.imp().input_device_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                let idx = row.selected() as usize;
                let name = if idx == 0 {
                    String::new()
                } else {
                    sources_for_cb.get(idx - 1).cloned().unwrap_or_default()
                };
                win.imp().config.borrow_mut().input_device_name = name;
                win.imp().config.borrow().save();
            }
        ));

        let sinks_for_cb = sinks.clone();
        settings.imp().monitor_device_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |row| {
                let idx = row.selected() as usize;
                let name = if idx == 0 {
                    String::new()
                } else {
                    sinks_for_cb.get(idx - 1).cloned().unwrap_or_default()
                };
                win.imp().config.borrow_mut().monitor_device_name = name;
                win.imp().config.borrow().save();
            }
        ));
    }
}

/// Default config entry for a newly-added effect. Built-ins get sensible defaults;
/// LV2 entries start with no param overrides (the plugin's own defaults apply).
fn default_entry_for(id: &str) -> crate::config::EffectEntry {
    use crate::config::EffectEntry;
    match id {
        "gain" => EffectEntry::gain(1.0, true),
        "gate" => EffectEntry::gate(0.02, 10.0, 100.0, true),
        other => EffectEntry {
            id: other.to_string(),
            enabled: true,
            params: std::collections::HashMap::new(),
        },
    }
}
