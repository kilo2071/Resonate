//! StatusNotifierItem (SNI) tray indicator via `ksni`.
//!
//! ksni runs its own thread and D-Bus connection, so tray clicks arrive off the
//! GTK main thread. We forward them as [`TrayCmd`]s through a plain `mpsc`
//! channel; the window polls the receiver on a glib timeout and acts on the GTK
//! thread (see `window.rs`). On GNOME a tray host requires the "AppIndicator and
//! KStatusNotifierItem Support" shell extension; without it the item is simply
//! not shown (registration is harmless).
//!
//! The menu also carries the effect presets, so a chain can be switched without
//! opening the window. The list is pushed in from the GTK thread with
//! [`set_presets`]; `ksni::Handle::update` marks the menu dirty and the tray
//! thread re-emits it.

use std::sync::mpsc::{Receiver, Sender};

use crate::APP_ID;

/// Commands raised by the tray, consumed on the GTK main thread.
#[derive(Clone, Debug)]
pub enum TrayCmd {
    /// Show / raise the main window.
    Show,
    /// Quit the application for real (not just hide to background).
    Quit,
    /// Apply the named effects-chain preset.
    LoadPreset(String),
}

pub struct ResonateTray {
    tx: Sender<TrayCmd>,
    presets: Vec<String>,
    /// Preset the live chain currently matches, if any (for the checkmark).
    active: Option<String>,
}

/// Handle for updating the tray from the GTK thread.
pub type TrayHandle = ksni::Handle<ResonateTray>;

impl ksni::Tray for ResonateTray {
    fn id(&self) -> String {
        APP_ID.to_string()
    }

    fn title(&self) -> String {
        "Resonate".to_string()
    }

    fn icon_name(&self) -> String {
        APP_ID.to_string()
    }

    // Left-click on the tray icon opens the window.
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCmd::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, MenuItem, StandardItem, SubMenu};

        let presets: Vec<MenuItem<Self>> = if self.presets.is_empty() {
            vec![StandardItem {
                label: "No presets saved".into(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.presets
                .iter()
                .map(|name| {
                    let target = name.clone();
                    CheckmarkItem {
                        label: name.clone(),
                        checked: self.active.as_deref() == Some(name.as_str()),
                        activate: Box::new(move |t: &mut Self| {
                            let _ = t.tx.send(TrayCmd::LoadPreset(target.clone()));
                        }),
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        vec![
            StandardItem {
                label: "Show Resonate".into(),
                icon_name: APP_ID.to_string(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(TrayCmd::Show);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Effect Preset".into(),
                icon_name: "media-eq-symbolic".into(),
                submenu: presets,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(TrayCmd::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the tray on its own thread; returns the command receiver and a handle
/// for pushing menu updates.
pub fn spawn() -> (Receiver<TrayCmd>, TrayHandle) {
    let (tx, rx) = std::sync::mpsc::channel();
    let service = ksni::TrayService::new(ResonateTray {
        tx,
        presets: Vec::new(),
        active: None,
    });
    let handle = service.handle();
    service.spawn();
    (rx, handle)
}

/// Replace the preset list shown in the tray menu (call from the GTK thread).
pub fn set_presets(handle: &TrayHandle, presets: Vec<String>, active: Option<String>) {
    handle.update(move |tray| {
        tray.presets = presets;
        tray.active = active;
    });
}
