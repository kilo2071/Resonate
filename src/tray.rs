//! StatusNotifierItem (SNI) tray indicator via `ksni`.
//!
//! ksni runs its own thread and D-Bus connection, so tray clicks arrive off the
//! GTK main thread. We forward them as [`TrayCmd`]s through a plain `mpsc`
//! channel; the window polls the receiver on a glib timeout and acts on the GTK
//! thread (see `window.rs`). On GNOME a tray host requires the "AppIndicator and
//! KStatusNotifierItem Support" shell extension; without it the item is simply
//! not shown (registration is harmless).

use std::sync::mpsc::{Receiver, Sender};

use crate::APP_ID;

/// Commands raised by the tray, consumed on the GTK main thread.
#[derive(Clone, Copy, Debug)]
pub enum TrayCmd {
    /// Show / raise the main window.
    Show,
    /// Quit the application for real (not just hide to background).
    Quit,
}

struct ResonateTray {
    tx: Sender<TrayCmd>,
}

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
        use ksni::menu::{MenuItem, StandardItem};
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

/// Spawn the tray on its own thread and return the receiver for tray commands.
pub fn spawn() -> Receiver<TrayCmd> {
    let (tx, rx) = std::sync::mpsc::channel();
    ksni::TrayService::new(ResonateTray { tx }).spawn();
    rx
}
