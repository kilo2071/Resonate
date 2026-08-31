//! Sound editor dialog: waveform view with draggable start/end markers, preview
//! playback (monitor-only), a persistent start-marker toggle, fades and the
//! per-sound volume. All changes persist immediately through the `save` hook.
//!
//! The *one-shot* start point lives on the soundboard page's LCD waveform, not
//! here — this dialog edits only the persistent settings.

use adw::prelude::*;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::audio::engine::PlayParams;
use crate::audio::wave::{self, fmt_secs, Wave};
use crate::config::{SoundSettings, StartMode};

/// Callbacks into the window/engine. Wrapped in `Rc` internally, so plain boxes.
pub struct EditorHooks {
    /// Start a monitor-only preview with the given playback shape.
    pub preview: Box<dyn Fn(&PathBuf, PlayParams)>,
    pub stop_preview: Box<dyn Fn()>,
    /// Current preview position (secs from file start), None when not previewing.
    pub preview_pos: Box<dyn Fn() -> Option<f32>>,
    /// Persist + live-apply the settings for this sound.
    pub save: Box<dyn Fn(&PathBuf, SoundSettings)>,
}

fn preview_params(s: &SoundSettings) -> PlayParams {
    PlayParams {
        volume: s.volume,
        // The editor always previews from the marker, even when the persistent
        // start toggle is off — that is what's being auditioned.
        start_secs: s.start_secs,
        end_secs: s.end_secs,
        fade_in_ms: s.fade_in_ms,
        fade_out_ms: s.fade_out_ms,
    }
}

pub fn present(parent: &impl IsA<gtk::Widget>, path: PathBuf, initial: SoundSettings, hooks: EditorHooks) {
    let hooks = Rc::new(hooks);
    let settings = Rc::new(RefCell::new(initial));
    let wave: Rc<RefCell<Option<Wave>>> = Rc::new(RefCell::new(None));
    let playhead: Rc<Cell<Option<f32>>> = Rc::new(Cell::new(None));
    let previewing = Rc::new(Cell::new(false));

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Sound")
        .to_string();

    // ── Widgets ──────────────────────────────────────────────────────────────

    let area = gtk::DrawingArea::builder()
        .content_height(120)
        .hexpand(true)
        .css_classes(["card"])
        .tooltip_text("Drag to move the start marker; drag near the end marker to trim the end")
        .build();

    let time_label = gtk::Label::builder()
        .label("Decoding…")
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();

    let preview_btn = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Preview from the start marker (monitor only)")
        .css_classes(["circular", "suggested-action"])
        .build();

    let start_row = adw::SwitchRow::builder()
        .title("Start at the marker")
        .subtitle("Every play begins at the orange marker")
        .active(!matches!(settings.borrow().start_mode, StartMode::Off))
        .build();

    let fade_in_row = adw::SpinRow::with_range(0.0, 5000.0, 50.0);
    fade_in_row.set_title("Fade in (ms)");
    fade_in_row.set_value(settings.borrow().fade_in_ms as f64);

    let fade_out_row = adw::SpinRow::with_range(0.0, 5000.0, 50.0);
    fade_out_row.set_title("Fade out (ms)");
    fade_out_row.set_value(settings.borrow().fade_out_ms as f64);

    let volume_scale = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .draw_value(false)
        .width_request(200)
        .valign(gtk::Align::Center)
        .build();
    volume_scale.set_range(0.0, 100.0);
    volume_scale.set_value(settings.borrow().volume as f64 * 100.0);
    let volume_row = adw::ActionRow::builder().title("Volume").build();
    volume_row.add_suffix(&volume_scale);

    let group = adw::PreferencesGroup::new();
    group.add(&start_row);
    group.add(&fade_in_row);
    group.add(&fade_out_row);
    group.add(&volume_row);

    let transport = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .build();
    transport.append(&preview_btn);
    transport.append(&time_label);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&area);
    content.append(&transport);
    content.append(&group);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&content));

    let dialog = adw::Dialog::builder()
        .title(&name)
        .content_width(560)
        .build();
    dialog.set_child(Some(&toolbar));

    // ── Time label helper ────────────────────────────────────────────────────

    let refresh_label = {
        let settings = settings.clone();
        let wave = wave.clone();
        let label_weak = time_label.downgrade();
        Rc::new(move || {
            let Some(label) = label_weak.upgrade() else { return };
            let duration = wave.borrow().as_ref().map(|w| w.duration_secs).unwrap_or(0.0);
            if duration <= 0.0 {
                return;
            }
            let s = settings.borrow();
            let end = if s.end_secs > 0.0 { s.end_secs } else { duration };
            label.set_label(&format!(
                "Start {} · End {} / {}",
                fmt_secs(s.start_secs),
                fmt_secs(end),
                fmt_secs(duration)
            ));
        })
    };

    // ── Background decode → waveform peaks ───────────────────────────────────

    let slot: Arc<Mutex<Option<Wave>>> = wave::load_async(&path);
    {
        let wave = wave.clone();
        let refresh = refresh_label.clone();
        let area_weak = area.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let Some(area) = area_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let ready = slot.lock().ok().and_then(|mut g| g.take());
            if let Some(w) = ready {
                *wave.borrow_mut() = Some(w);
                refresh();
                area.queue_draw();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    // ── Drawing ──────────────────────────────────────────────────────────────

    {
        let wave = wave.clone();
        let settings = settings.clone();
        let playhead = playhead.clone();
        area.set_draw_func(move |da, cr, w, h| {
            let (wf, hf) = (w as f64, h as f64);
            let fg = da.color();

            if let Some(wave) = wave.borrow().as_ref() {
                let dur = wave.duration_secs.max(0.001) as f64;
                let s = settings.borrow();
                let start_x = (s.start_secs as f64 / dur).clamp(0.0, 1.0) * wf;
                let end_x = if s.end_secs > 0.0 {
                    (s.end_secs as f64 / dur).clamp(0.0, 1.0) * wf
                } else {
                    wf
                };
                drop(s);

                let mid = hf / 2.0;
                for px in 0..w {
                    let bucket = (px as usize * wave.peaks.len()) / (w.max(1) as usize);
                    let amp = wave.peaks[bucket.min(wave.peaks.len() - 1)] as f64;
                    let half = (amp * (hf / 2.0 - 4.0)).max(0.75);
                    // Dim the trimmed-away regions.
                    let alpha = if (px as f64) < start_x || (px as f64) > end_x { 0.25 } else { 0.75 };
                    cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, alpha);
                    cr.rectangle(px as f64, mid - half, 1.0, half * 2.0);
                    let _ = cr.fill();
                }

                // Start marker (orange)
                cr.set_source_rgba(1.0, 0.47, 0.0, 0.95);
                cr.rectangle(start_x - 1.0, 0.0, 2.0, hf);
                let _ = cr.fill();

                // End marker (red) — only when trimming
                if end_x < wf {
                    cr.set_source_rgba(0.88, 0.11, 0.14, 0.95);
                    cr.rectangle(end_x - 1.0, 0.0, 2.0, hf);
                    let _ = cr.fill();
                }

                // Preview playhead (green)
                if let Some(pos) = playhead.get() {
                    let x = (pos as f64 / dur).clamp(0.0, 1.0) * wf;
                    cr.set_source_rgba(0.34, 0.89, 0.54, 0.9);
                    cr.rectangle(x - 1.0, 0.0, 2.0, hf);
                    let _ = cr.fill();
                }
            } else {
                cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.4);
                cr.move_to(wf / 2.0 - 30.0, hf / 2.0);
                let _ = cr.show_text("Decoding…");
            }
        });
    }

    // ── Scrubbing: drag the nearest marker (start or end) ────────────────────

    // Which marker the current drag is moving (true = end marker).
    let dragging_end = Rc::new(Cell::new(false));

    let update_marker = {
        let wave = wave.clone();
        let settings = settings.clone();
        let refresh = refresh_label.clone();
        let dragging_end = dragging_end.clone();
        let area_weak = area.downgrade();
        Rc::new(move |x: f64, begin: bool| {
            let Some(area) = area_weak.upgrade() else { return };
            let duration = wave.borrow().as_ref().map(|w| w.duration_secs).unwrap_or(0.0);
            if duration <= 0.0 {
                return;
            }
            let w = area.width().max(1) as f64;
            let frac = (x / w).clamp(0.0, 1.0);
            let secs = frac as f32 * duration;

            if begin {
                // Pick the marker nearest to the press point.
                let s = settings.borrow();
                let end = if s.end_secs > 0.0 { s.end_secs } else { duration };
                dragging_end.set((secs - s.start_secs).abs() > (secs - end).abs());
            }

            let mut s = settings.borrow_mut();
            if dragging_end.get() {
                // Snap to "no trim" when released at the far right edge.
                s.end_secs = if frac > 0.995 { 0.0 } else { secs.max(s.start_secs + 0.05) };
            } else {
                s.start_secs = if s.end_secs > 0.0 { secs.min(s.end_secs - 0.05) } else { secs };
            }
            drop(s);
            refresh();
            area.queue_draw();
        })
    };

    let commit_marker = {
        let settings = settings.clone();
        let hooks = hooks.clone();
        let path = path.clone();
        let previewing = previewing.clone();
        let dragging_end = dragging_end.clone();
        let start_row_weak = start_row.downgrade();
        Rc::new(move || {
            // Moving the start marker while the toggle is off arms it — a marker
            // that does nothing would be confusing.
            if !dragging_end.get() && settings.borrow().start_mode == StartMode::Off {
                if let Some(row) = start_row_weak.upgrade() {
                    row.set_active(true); // triggers the switch handler → saves
                }
            }
            let s = settings.borrow().clone();
            (hooks.save)(&path, s.clone());
            // Scrub-preview: if a preview is running, restart it at the marker.
            if previewing.get() {
                (hooks.preview)(&path, preview_params(&s));
            }
        })
    };

    let drag = gtk::GestureDrag::new();
    {
        let update = update_marker.clone();
        drag.connect_drag_begin(move |_, x, _| update(x, true));
    }
    {
        let update = update_marker.clone();
        drag.connect_drag_update(move |g, dx, _| {
            if let Some((sx, _)) = g.start_point() {
                update(sx + dx, false);
            }
        });
    }
    {
        let commit = commit_marker.clone();
        drag.connect_drag_end(move |_, _, _| commit());
    }
    area.add_controller(drag);

    // ── Preview button + playhead poll ───────────────────────────────────────

    {
        let hooks = hooks.clone();
        let settings = settings.clone();
        let path = path.clone();
        let previewing = previewing.clone();
        preview_btn.connect_clicked(move |btn| {
            if previewing.get() {
                (hooks.stop_preview)();
                previewing.set(false);
                btn.set_icon_name("media-playback-start-symbolic");
            } else {
                (hooks.preview)(&path, preview_params(&settings.borrow()));
                previewing.set(true);
                btn.set_icon_name("media-playback-stop-symbolic");
            }
        });
    }
    {
        let hooks = hooks.clone();
        let playhead = playhead.clone();
        let previewing = previewing.clone();
        let area_weak = area.downgrade();
        let btn_weak = preview_btn.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(66), move || {
            let Some(area) = area_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let pos = (hooks.preview_pos)();
            if pos.is_none() && previewing.get() {
                previewing.set(false);
                if let Some(b) = btn_weak.upgrade() {
                    b.set_icon_name("media-playback-start-symbolic");
                }
            }
            if playhead.get() != pos {
                playhead.set(pos);
                area.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }

    // ── Rows → save ──────────────────────────────────────────────────────────

    {
        let settings = settings.clone();
        let hooks = hooks.clone();
        let path = path.clone();
        start_row.connect_active_notify(move |row| {
            settings.borrow_mut().start_mode = if row.is_active() {
                StartMode::Every
            } else {
                StartMode::Off
            };
            (hooks.save)(&path, settings.borrow().clone());
        });
    }
    {
        let settings = settings.clone();
        let hooks = hooks.clone();
        let path = path.clone();
        fade_in_row.connect_value_notify(move |row| {
            settings.borrow_mut().fade_in_ms = row.value() as f32;
            (hooks.save)(&path, settings.borrow().clone());
        });
    }
    {
        let settings = settings.clone();
        let hooks = hooks.clone();
        let path = path.clone();
        fade_out_row.connect_value_notify(move |row| {
            settings.borrow_mut().fade_out_ms = row.value() as f32;
            (hooks.save)(&path, settings.borrow().clone());
        });
    }
    {
        let settings = settings.clone();
        let hooks = hooks.clone();
        let path = path.clone();
        volume_scale.connect_value_changed(move |s| {
            settings.borrow_mut().volume = (s.value() / 100.0) as f32;
            (hooks.save)(&path, settings.borrow().clone());
        });
    }

    // Stop the preview when the dialog goes away.
    {
        let hooks = hooks.clone();
        dialog.connect_closed(move |_| (hooks.stop_preview)());
    }

    dialog.present(Some(parent));
}
