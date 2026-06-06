use gtk::glib;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub sounds_folder: PathBuf,
    pub move_files_to_folder: bool,
    pub polyphonic: bool,
    pub stop_on_play: bool,
    pub default_volume: u32,
}

impl Default for Config {
    fn default() -> Self {
        let docs = glib::user_special_dir(glib::UserDirectory::Documents)
            .unwrap_or_else(|| glib::home_dir().join("Documents"));
        Self {
            sounds_folder: docs.join("Sounds"),
            move_files_to_folder: true,
            polyphonic: true,
            stop_on_play: true,
            default_volume: 100,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str(&text) {
                    return cfg;
                }
            }
        }
        let cfg = Self::default();
        cfg.save();
        cfg
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, text);
        }
    }

    fn config_path() -> PathBuf {
        glib::user_config_dir()
            .join("io.github.resonate")
            .join("config.json")
    }
}
