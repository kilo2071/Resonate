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
//! thread re-emits it. The presets are a radio group (plus a "Custom" slot for a
//! chain matching no preset) and the active name is repeated in the submenu
//! label and the tooltip, so the current selection is readable even on hosts
//! that draw radio marks poorly.

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
    /// Preset the live chain currently matches, if any (the radio selection).
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

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: APP_ID.to_string(),
            title: "Resonate".into(),
            description: format!("Effect preset: {}", self.active_label()),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};

        // The radio group needs a selected index, and the chain often matches no
        // preset at all (freshly edited). A leading, inert "Custom" option gives
        // that state something to point at, so exactly one item is always marked.
        let active = self.active.as_deref().filter(|a| self.presets.iter().any(|p| p == a));
        let custom_slot = active.is_none();
        let mut options: Vec<RadioItem> = Vec::new();
        if custom_slot {
            options.push(RadioItem {
                label: "Custom (unsaved)".into(),
                ..Default::default()
            });
        }
        options.extend(self.presets.iter().map(|name| RadioItem {
            label: escape_label(name),
            ..Default::default()
        }));
        let selected = active
            .and_then(|a| self.presets.iter().position(|p| p == a))
            .map(|i| i + usize::from(custom_slot))
            .unwrap_or(0);

        let presets: Vec<MenuItem<Self>> = if self.presets.is_empty() {
            vec![StandardItem {
                label: "No presets saved".into(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            let names = self.presets.clone();
            vec![RadioGroup {
                selected,
                options,
                select: Box::new(move |t: &mut Self, idx: usize| {
                    // Index 0 is the "Custom" slot when it is shown; picking it
                    // means "keep what I have", so there is nothing to load.
                    let idx = match idx.checked_sub(usize::from(custom_slot)) {
                        Some(i) => i,
                        None => return,
                    };
                    if let Some(name) = names.get(idx) {
                        let _ = t.tx.send(TrayCmd::LoadPreset(name.clone()));
                    }
                }),
            }
            .into()]
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
                // The active preset is spelled out in the label too: some SNI
                // hosts (the GNOME AppIndicator extension included) draw the
                // radio mark faintly or not at all in submenus.
                label: format!("Effect Preset: {}", escape_label(&self.active_label())),
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

impl ResonateTray {
    /// What to show as the current preset: its name, or "Custom" when the live
    /// chain matches none of the saved ones.
    fn active_label(&self) -> String {
        self.active
            .clone()
            .filter(|a| self.presets.contains(a))
            .unwrap_or_else(|| "Custom".to_string())
    }
}

/// Escape a user-supplied string for a dbusmenu label: a single underscore is
/// eaten as a mnemonic marker, a doubled one renders as itself.
fn escape_label(label: &str) -> String {
    label.replace('_', "__")
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
