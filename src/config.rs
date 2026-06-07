use gtk::glib;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EffectEntry {
    pub id: String,
    pub enabled: bool,
    pub params: HashMap<String, f32>,
}

impl EffectEntry {
    pub fn gain(gain: f32, enabled: bool) -> Self {
        Self { id: "gain".into(), enabled, params: [("gain".into(), gain)].into() }
    }

    pub fn gate(threshold: f32, attack_ms: f32, release_ms: f32, enabled: bool) -> Self {
        Self {
            id: "gate".into(),
            enabled,
            params: [
                ("threshold".into(), threshold),
                ("attack_ms".into(), attack_ms),
                ("release_ms".into(), release_ms),
            ].into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub sounds_folder: PathBuf,
    pub move_files_to_folder: bool,
    pub polyphonic: bool,
    pub stop_on_play: bool,
    pub default_volume: u32,

    // Virtual device
    pub virtual_device_name: String,
    pub virtual_device_enabled: bool,

    // Monitor output (plays on the system default output device)
    pub monitor_enabled: bool,
    pub monitor_volume: f32,

    // Microphone input
    pub input_device_name: String,
    pub mic_volume: f32,

    // Effect chain applied to mic input (gate → gain order)
    #[serde(default = "default_effects_chain")]
    pub effects_chain: Vec<EffectEntry>,
}

fn default_effects_chain() -> Vec<EffectEntry> {
    vec![
        EffectEntry::gate(0.02, 10.0, 100.0, false),
        EffectEntry::gain(1.0, true),
    ]
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
            virtual_device_name: "Resonate Microphone".to_string(),
            virtual_device_enabled: true,
            monitor_enabled: true,
            monitor_volume: 1.0,
            input_device_name: String::new(),
            mic_volume: 1.0,
            effects_chain: default_effects_chain(),
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
            .join("io.github.kilo2071.Resonate")
            .join("config.json")
    }
}
