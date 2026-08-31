use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::audio::engine::SoundInfo;
use crate::audio::wave::{self, Wave};
use crate::ui::ResonateSoundTile;

#[derive(Clone, Debug)]
pub struct SoundEntry {
    pub name: String,
    pub path: PathBuf,
    pub tile: ResonateSoundTile,
    /// Shared with all tile callbacks so rename keeps them in sync.
    pub shared_path: Rc<RefCell<PathBuf>>,
}

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/github/kilo2071/Resonate/ui/soundboard_page.ui")]
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
        pub progress_area: TemplateChild<gtk::DrawingArea>,
        #[template_child]
        pub total_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub until_next_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub master_volume_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub monitor_volume_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub soundboard_volume_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub sound_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub wave_area: TemplateChild<gtk::DrawingArea>,
        #[template_child]
        pub preview_button: TemplateChild<gtk::Button>,

        pub sounds: RefCell<Vec<SoundEntry>>,
        pub play_fn: RefCell<Option<Box<dyn Fn(PathBuf, f32) + 'static>>>,
        pub cue_fn: RefCell<Option<Box<dyn Fn(PathBuf, f32) + 'static>>>,
        pub rename_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
        pub remove_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
        pub edit_fn: RefCell<Option<Box<dyn Fn(PathBuf) + 'static>>>,
        pub volume_fn: RefCell<Option<Box<dyn Fn(PathBuf, f32) + 'static>>>,
        /// Fired when the user finishes a scrub on the LCD waveform: (path, secs).
        pub scrub_fn: RefCell<Option<Box<dyn Fn(PathBuf, f32) + 'static>>>,
        /// Fired after a tile drag-reorder with the new file-name order.
        pub reorder_fn: RefCell<Option<Box<dyn Fn(Vec<String>) + 'static>>>,

        /// Fired when the user finishes a scrub while playing: fraction 0–1.
        pub seek_fn: RefCell<Option<Box<dyn Fn(f64) + 'static>>>,

        // Selection + LCD state
        pub selected: RefCell<Option<PathBuf>>,
        /// Waveform of the selected sound — used for its duration (secs↔fraction).
        pub lcd_wave: RefCell<Option<Rc<Wave>>>,
        pub wave_cache: RefCell<HashMap<PathBuf, Rc<Wave>>>,
        /// One-shot start point (secs) for the selected sound; consumed on play.
        pub one_shot: Cell<Option<f32>>,
        pub preview_pos: Cell<Option<f32>>,
        /// Latest oscilloscope samples (mono mix tail).
        pub scope_samples: RefCell<Vec<f32>>,
        pub progress_fraction: Cell<f64>,
        pub is_playing: Cell<bool>,
        /// While dragging the progress bar during playback: the drag fraction.
        pub drag_frac: Cell<Option<f64>>,
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
                progress_area: Default::default(),
                total_time_label: Default::default(),
                until_next_label: Default::default(),
                master_volume_scale: Default::default(),
                monitor_volume_scale: Default::default(),
                soundboard_volume_scale: Default::default(),
                sound_search_entry: Default::default(),
                wave_area: Default::default(),
                preview_button: Default::default(),
                sounds: RefCell::new(Vec::new()),
                play_fn: RefCell::new(None),
                cue_fn: RefCell::new(None),
                rename_fn: RefCell::new(None),
                remove_fn: RefCell::new(None),
                edit_fn: RefCell::new(None),
                volume_fn: RefCell::new(None),
                scrub_fn: RefCell::new(None),
                reorder_fn: RefCell::new(None),
                seek_fn: RefCell::new(None),
                selected: RefCell::new(None),
                lcd_wave: RefCell::new(None),
                wave_cache: RefCell::new(HashMap::new()),
                one_shot: Cell::new(None),
                preview_pos: Cell::new(None),
                scope_samples: RefCell::new(Vec::new()),
                progress_fraction: Cell::new(0.0),
                is_playing: Cell::new(false),
                drag_frac: Cell::new(None),
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

    impl ObjectImpl for ResonateSoundboardPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_search();
            self.obj().setup_lcd_wave();
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

    pub fn set_play_callback<F: Fn(PathBuf, f32) + 'static>(&self, f: F) {
        *self.imp().play_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_cue_callback<F: Fn(PathBuf, f32) + 'static>(&self, f: F) {
        *self.imp().cue_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_edit_callback<F: Fn(PathBuf) + 'static>(&self, f: F) {
        *self.imp().edit_fn.borrow_mut() = Some(Box::new(f));
    }

    /// Fires as a tile's volume slider moves (path, 0.0–1.0 fraction).
    pub fn set_volume_callback<F: Fn(PathBuf, f32) + 'static>(&self, f: F) {
        *self.imp().volume_fn.borrow_mut() = Some(Box::new(f));
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

    pub fn set_play_pause_sensitive(&self, sensitive: bool) {
        self.imp().play_pause_button.set_sensitive(sensitive);
    }

    pub fn connect_mic_volume_changed<F: Fn(f32) + 'static>(&self, f: F) {
        self.imp()
            .master_volume_scale
            .connect_value_changed(move |s| f(s.value() as f32 / 100.0));
    }

    pub fn connect_monitor_volume_changed<F: Fn(f32) + 'static>(&self, f: F) {
        self.imp()
            .monitor_volume_scale
            .connect_value_changed(move |s| f(s.value() as f32 / 100.0));
    }

    pub fn connect_soundboard_volume_changed<F: Fn(f32) + 'static>(&self, f: F) {
        self.imp()
            .soundboard_volume_scale
            .connect_value_changed(move |s| f(s.value() as f32 / 100.0));
    }

    pub fn set_initial_volumes(&self, mic_vol: f32, monitor_vol: f32, soundboard_vol: f32) {
        self.imp()
            .master_volume_scale
            .set_value((mic_vol * 100.0) as f64);
        self.imp()
            .monitor_volume_scale
            .set_value((monitor_vol * 100.0) as f64);
        self.imp()
            .soundboard_volume_scale
            .set_value((soundboard_vol * 100.0) as f64);
    }

    // ── Search ───────────────────────────────────────────────────────────────

    fn setup_search(&self) {
        self.imp().sound_search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |entry| {
                let query = entry.text().to_lowercase();
                let sounds = page.imp().sounds.borrow();
                for s in sounds.iter() {
                    let visible = query.is_empty() || s.name.to_lowercase().contains(&query);
                    if let Some(fbc) = s.tile.parent() {
                        fbc.set_visible(visible);
                    }
                }
            }
        ));
    }

    // ── Selection + LCD waveform ─────────────────────────────────────────────

    fn setup_lcd_wave(&self) {
        // ── Oscilloscope (wave_area): recent mono samples of the playing mix ──
        let area = self.imp().wave_area.get();
        area.set_draw_func(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, cr, w, h| {
                let (wf, hf) = (w as f64, h as f64);
                let mid = hf / 2.0;
                let samples = page.imp().scope_samples.borrow();

                // Center line, always.
                cr.set_source_rgba(0.35, 0.55, 0.42, 0.45);
                cr.rectangle(0.0, mid - 0.5, wf, 1.0);
                let _ = cr.fill();

                if samples.len() < 64 {
                    return;
                }
                // Min/max envelope per pixel column — classic scope trace.
                cr.set_source_rgba(0.34, 0.89, 0.54, 0.9);
                let len = samples.len();
                for px in 0..w {
                    let i0 = px as usize * len / w.max(1) as usize;
                    let i1 = ((px as usize + 1) * len / w.max(1) as usize).max(i0 + 1);
                    let (mut lo, mut hi) = (0.0f32, 0.0f32);
                    for &s in &samples[i0..i1.min(len)] {
                        lo = lo.min(s);
                        hi = hi.max(s);
                    }
                    let half = hf / 2.0 - 1.0;
                    let y_top = mid - (hi as f64).clamp(-1.0, 1.0) * half;
                    let y_bot = mid - (lo as f64).clamp(-1.0, 1.0) * half;
                    cr.rectangle(px as f64, y_top, 1.0, (y_bot - y_top).max(1.0));
                    let _ = cr.fill();
                }
            }
        ));

        // ── Progress bar (progress_area): draw + scrub ───────────────────────
        let progress = self.imp().progress_area.get();
        progress.set_draw_func(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, cr, w, h| {
                let (wf, hf) = (w as f64, h as f64);
                let bar_h = 6.0f64.min(hf);
                let y = (hf - bar_h) / 2.0;

                // Track
                cr.set_source_rgba(0.5, 0.5, 0.5, 0.25);
                cr.rectangle(0.0, y, wf, bar_h);
                let _ = cr.fill();

                // Fill: playback progress (drag position wins while scrubbing)
                let frac = page
                    .imp()
                    .drag_frac
                    .get()
                    .unwrap_or_else(|| page.imp().progress_fraction.get());
                if frac > 0.0 {
                    cr.set_source_rgba(0.34, 0.89, 0.54, 0.9);
                    cr.rectangle(0.0, y, frac.clamp(0.0, 1.0) * wf, bar_h);
                    let _ = cr.fill();
                }

                let duration = page
                    .imp()
                    .lcd_wave
                    .borrow()
                    .as_ref()
                    .map(|wv| wv.duration_secs)
                    .unwrap_or(0.0);

                // One-shot start marker (orange) — idle only, needs a duration.
                if !page.imp().is_playing.get() && duration > 0.0 {
                    if let Some(secs) = page.imp().one_shot.get() {
                        let x = (secs / duration).clamp(0.0, 1.0) as f64 * wf;
                        cr.set_source_rgba(1.0, 0.47, 0.0, 0.95);
                        cr.rectangle(x - 1.5, 0.0, 3.0, hf);
                        let _ = cr.fill();
                    }
                }

                // Preview playhead (bright green line over the full height).
                if duration > 0.0 {
                    if let Some(pos) = page.imp().preview_pos.get() {
                        let x = (pos / duration).clamp(0.0, 1.0) as f64 * wf;
                        cr.set_source_rgba(0.9, 1.0, 0.9, 0.9);
                        cr.rectangle(x - 1.0, 0.0, 2.0, hf);
                        let _ = cr.fill();
                    }
                }
            }
        ));

        // Drag on the progress bar:
        //  - while playing → live seek preview, commit the seek on release
        //  - while idle    → set the one-shot start marker for the selected sound
        let drag = gtk::GestureDrag::new();
        let scrub = Rc::new(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |x: f64, done: bool| {
                let w = page.imp().progress_area.width().max(1) as f64;
                let frac = (x / w).clamp(0.0, 1.0);

                if page.imp().is_playing.get() {
                    if done {
                        page.imp().drag_frac.set(None);
                        if let Some(f) = page.imp().seek_fn.borrow().as_ref() {
                            f(frac);
                        }
                    } else {
                        page.imp().drag_frac.set(Some(frac));
                    }
                    page.imp().progress_area.queue_draw();
                    return;
                }

                let duration = page
                    .imp()
                    .lcd_wave
                    .borrow()
                    .as_ref()
                    .map(|wv| wv.duration_secs)
                    .unwrap_or(0.0);
                if duration <= 0.0 {
                    return;
                }
                let secs = frac as f32 * duration;
                page.imp().one_shot.set(Some(secs));
                page.imp().progress_area.queue_draw();
                if done {
                    let selected = page.imp().selected.borrow().clone();
                    if let Some(path) = selected {
                        if let Some(f) = page.imp().scrub_fn.borrow().as_ref() {
                            f(path, secs);
                        }
                    }
                }
            }
        ));
        {
            let scrub = scrub.clone();
            drag.connect_drag_begin(move |_, x, _| scrub(x, false));
        }
        {
            let scrub = scrub.clone();
            drag.connect_drag_update(move |g, dx, _| {
                if let Some((sx, _)) = g.start_point() {
                    scrub(sx + dx, false);
                }
            });
        }
        drag.connect_drag_end(move |g, dx, _| {
            if let Some((sx, _)) = g.start_point() {
                scrub(sx + dx, true);
            }
        });
        progress.add_controller(drag);
    }

    /// Feed the oscilloscope with the latest mono samples of the playing mix.
    pub fn set_scope(&self, samples: &[f32]) {
        {
            let mut display = self.imp().scope_samples.borrow_mut();
            display.clear();
            display.extend_from_slice(samples);
        }
        self.imp().wave_area.queue_draw();
    }

    pub fn set_seek_callback<F: Fn(f64) + 'static>(&self, f: F) {
        *self.imp().seek_fn.borrow_mut() = Some(Box::new(f));
    }

    /// Select a sound: highlight its tile and load its waveform into the LCD.
    pub fn select_sound(&self, path: &PathBuf) {
        if self.imp().selected.borrow().as_ref() == Some(path) {
            return;
        }
        *self.imp().selected.borrow_mut() = Some(path.clone());
        self.imp().one_shot.set(None);

        for s in self.imp().sounds.borrow().iter() {
            if &s.path == path {
                s.tile.add_css_class("sound-selected");
            } else {
                s.tile.remove_css_class("sound-selected");
            }
        }
        self.imp().preview_button.set_sensitive(true);

        // Waveform (for the secs↔fraction mapping): cache hit or async decode.
        *self.imp().lcd_wave.borrow_mut() = self.imp().wave_cache.borrow().get(path).cloned();
        self.imp().progress_area.queue_draw();
        if self.imp().lcd_wave.borrow().is_none() {
            let slot: Arc<Mutex<Option<Wave>>> = wave::load_async(path);
            let path = path.clone();
            let page_weak = self.downgrade();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                let Some(page) = page_weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let ready = slot.lock().ok().and_then(|mut g| g.take());
                if let Some(w) = ready {
                    let w = Rc::new(w);
                    page.imp().wave_cache.borrow_mut().insert(path.clone(), w.clone());
                    // Only apply it if this sound is still the selected one.
                    if page.imp().selected.borrow().as_ref() == Some(&path) {
                        *page.imp().lcd_wave.borrow_mut() = Some(w);
                        page.imp().progress_area.queue_draw();
                    }
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            });
        }
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.imp().selected.borrow().clone()
    }

    /// One-shot start for `path` without consuming it (used by preview).
    pub fn peek_one_shot(&self, path: &std::path::Path) -> Option<f32> {
        (self.imp().selected.borrow().as_deref() == Some(path))
            .then(|| self.imp().one_shot.get())
            .flatten()
    }

    /// Take the one-shot start for `path` — it applies to one play only.
    pub fn consume_one_shot(&self, path: &std::path::Path) -> Option<f32> {
        let hit = self.peek_one_shot(path);
        if hit.is_some() {
            self.imp().one_shot.set(None);
            self.imp().progress_area.queue_draw();
        }
        hit
    }

    /// Update the preview playhead on the progress bar (None = not previewing).
    pub fn set_preview_pos(&self, pos: Option<f32>) {
        if self.imp().preview_pos.get() != pos {
            self.imp().preview_pos.set(pos);
            self.imp().progress_area.queue_draw();
        }
    }

    pub fn connect_preview<F: Fn() + 'static>(&self, f: F) {
        self.imp().preview_button.connect_clicked(move |_| f());
    }

    pub fn set_preview_active(&self, active: bool) {
        let icon = if active {
            "media-playback-stop-symbolic"
        } else {
            "audio-headphones-symbolic"
        };
        // Called every UI tick — only touch the widget on a real change.
        if self.imp().preview_button.icon_name().as_deref() != Some(icon) {
            self.imp().preview_button.set_icon_name(icon);
        }
    }

    pub fn set_scrub_callback<F: Fn(PathBuf, f32) + 'static>(&self, f: F) {
        *self.imp().scrub_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_reorder_callback<F: Fn(Vec<String>) + 'static>(&self, f: F) {
        *self.imp().reorder_fn.borrow_mut() = Some(Box::new(f));
    }

    /// Sound path at display position `idx` (0-based) — used by global hotkeys.
    pub fn sound_at_index(&self, idx: usize) -> Option<(PathBuf, f32)> {
        self.imp()
            .sounds
            .borrow()
            .get(idx)
            .map(|s| (s.path.clone(), s.tile.volume_fraction()))
    }

    /// Renumber tiles to match their display order.
    fn renumber(&self) {
        for (i, s) in self.imp().sounds.borrow().iter().enumerate() {
            s.tile.set_number(i as u32 + 1);
        }
    }

    /// Current display order as sound file names (for persistence).
    fn current_order(&self) -> Vec<String> {
        self.imp()
            .sounds
            .borrow()
            .iter()
            .map(|s| crate::config::Config::sound_key(&s.path))
            .collect()
    }

    /// Move the tile for `src` in front of the tile for `dest` (drag-reorder).
    fn move_sound_before(&self, src: &PathBuf, dest: &PathBuf) {
        if src == dest {
            return;
        }
        {
            let mut sounds = self.imp().sounds.borrow_mut();
            let Some(from) = sounds.iter().position(|s| &s.path == src) else { return };
            let entry = sounds.remove(from);
            let Some(to) = sounds.iter().position(|s| &s.path == dest) else {
                sounds.insert(from, entry);
                return;
            };
            sounds.insert(to, entry);

            // Rebuild the FlowBox in the new order: detach every tile, re-add.
            let grid = &self.imp().sound_grid;
            while let Some(fbc) = grid.child_at_index(0) {
                if let Some(child) = fbc.child() {
                    fbc.set_child(None::<&gtk::Widget>);
                    let _ = child;
                }
                grid.remove(&fbc);
            }
            for s in sounds.iter() {
                grid.insert(&s.tile, -1);
            }
        }
        self.renumber();
        if let Some(f) = self.imp().reorder_fn.borrow().as_ref() {
            f(self.current_order());
        }
    }

    /// Move a tile's volume slider without going through the user (editor sync).
    /// The value-changed callback still fires, which is fine — it re-saves the
    /// same value.
    pub fn set_tile_volume(&self, path: &PathBuf, fraction: f32) {
        let sounds = self.imp().sounds.borrow();
        if let Some(entry) = sounds.iter().find(|s| &s.path == path) {
            entry.tile.set_volume((fraction * 100.0) as f64);
        }
    }

    /// Add a sound and wire its tile buttons. Paths are shared via Rc so rename stays in sync.
    /// `volume` is the persisted per-sound gain (0.0–1.0).
    pub fn add_sound_from_path(&self, path: PathBuf, volume: f32) -> bool {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sound")
            .to_string();

        let n = self.imp().sound_grid.observe_children().n_items() + 1;
        let tile = ResonateSoundTile::new(n, &name);
        tile.set_volume((volume * 100.0) as f64);

        let shared_path = Rc::new(RefCell::new(path.clone()));

        // Play button
        let sp_play = shared_path.clone();
        let tile_play = tile.clone();
        tile.connect_play(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().play_fn.borrow().as_ref() {
                    f(sp_play.borrow().clone(), tile_play.volume_fraction());
                }
            }
        ));

        // Cue button
        let sp_cue = shared_path.clone();
        let tile_cue = tile.clone();
        tile.connect_cue(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().cue_fn.borrow().as_ref() {
                    f(sp_cue.borrow().clone(), tile_cue.volume_fraction());
                }
            }
        ));

        // Rename menu item
        let sp_rename = shared_path.clone();
        tile.connect_rename(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().rename_fn.borrow().as_ref() {
                    f(sp_rename.borrow().clone());
                }
            }
        ));

        // Remove menu item
        let sp_remove = shared_path.clone();
        tile.connect_remove(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().remove_fn.borrow().as_ref() {
                    f(sp_remove.borrow().clone());
                }
            }
        ));

        // Edit menu item (waveform editor)
        let sp_edit = shared_path.clone();
        tile.connect_edit(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move || {
                if let Some(f) = page.imp().edit_fn.borrow().as_ref() {
                    f(sp_edit.borrow().clone());
                }
            }
        ));

        // Popover volume slider → persist + live-apply
        let sp_vol = shared_path.clone();
        tile.connect_volume_changed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |fraction| {
                if let Some(f) = page.imp().volume_fn.borrow().as_ref() {
                    f(sp_vol.borrow().clone(), fraction);
                }
            }
        ));

        // Click anywhere on the tile selects it (capture phase, no claim, so the
        // play/menu buttons keep working).
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let sp_select = shared_path.clone();
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_, _, _, _| {
                page.select_sound(&sp_select.borrow().clone());
            }
        ));
        tile.add_controller(click);

        // Drag-reorder: a tile can be dropped onto another tile to move before it.
        let drag_src = gtk::DragSource::new();
        drag_src.set_actions(gtk::gdk::DragAction::MOVE);
        let sp_drag = shared_path.clone();
        drag_src.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(
                &sp_drag.borrow().to_string_lossy().to_string().to_value(),
            ))
        });
        tile.add_controller(drag_src);

        let drop = gtk::DropTarget::new(glib::types::Type::STRING, gtk::gdk::DragAction::MOVE);
        let sp_drop = shared_path.clone();
        drop.connect_drop(glib::clone!(
            #[weak(rename_to = page)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| {
                let Ok(src) = value.get::<String>() else { return false };
                page.move_sound_before(&PathBuf::from(src), &sp_drop.borrow().clone());
                true
            }
        ));
        tile.add_controller(drop);

        self.imp().sounds.borrow_mut().push(SoundEntry {
            name,
            path,
            tile: tile.clone(),
            shared_path,
        });
        self.imp().sound_grid.insert(&tile, -1);
        self.update_empty_state();
        self.renumber();
        true
    }

    /// Update tile label and path (and shared path used by callbacks) after a rename.
    pub fn rename_sound_by_path(&self, old_path: &PathBuf, new_name: String, new_path: PathBuf) {
        let mut sounds = self.imp().sounds.borrow_mut();
        if let Some(entry) = sounds.iter_mut().find(|s| &s.path == old_path) {
            entry.tile.set_name(&new_name);
            entry.name = new_name;
            *entry.shared_path.borrow_mut() = new_path.clone();
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
        // Clear the selection if it pointed at the removed sound.
        if self.imp().selected.borrow().as_ref() == Some(path) {
            *self.imp().selected.borrow_mut() = None;
            *self.imp().lcd_wave.borrow_mut() = None;
            self.imp().one_shot.set(None);
            self.imp().preview_button.set_sensitive(false);
            self.imp().progress_area.queue_draw();
        }
        self.update_empty_state();
        self.renumber();
        Some(entry)
    }

    /// Update the LCD panel with current play state and time totals.
    pub fn update_playback_display(&self, playing: &[SoundInfo], queue: &[(String, Option<u32>)]) {
        let markup = format_playback_markup(playing, queue);
        self.imp().now_playing_label.set_markup(&markup);

        let fraction = playing.first().map(|s| s.fraction).unwrap_or(0.0);
        self.imp().progress_fraction.set(fraction);
        self.imp().is_playing.set(!playing.is_empty());
        self.imp().progress_area.queue_draw();

        // Time totals
        let until_next_secs: u32 = playing.iter().filter_map(|s| s.remaining_secs).sum();
        let queue_secs: u32 = queue.iter().filter_map(|(_, s)| *s).sum();
        let grand_total_secs = until_next_secs + queue_secs;

        if playing.is_empty() && queue.is_empty() {
            self.imp().total_time_label.set_label("—:——");
            self.imp().until_next_label.set_visible(false);
        } else {
            self.imp().total_time_label.set_label(&fmt_time(grand_total_secs));
            if !queue.is_empty() {
                self.imp().until_next_label.set_label(&fmt_time(until_next_secs));
                self.imp().until_next_label.set_visible(true);
            } else {
                self.imp().until_next_label.set_visible(false);
            }
        }
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

fn fmt_time(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
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

fn format_playback_markup(playing: &[SoundInfo], queue: &[(String, Option<u32>)]) -> String {
    if playing.is_empty() && queue.is_empty() {
        return "<span foreground='#555555'>Nothing playing</span>".to_string();
    }

    const NAME_MAX: usize = 30;
    const TIME_W: usize = 6;
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
        for (i, (name, _)) in queue.iter().enumerate().take(5) {
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
