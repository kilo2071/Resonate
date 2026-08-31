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

/// When a sound's saved start point applies.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartMode {
    /// Play from the beginning; the marker is kept but inactive.
    #[default]
    Off,
    /// Every play starts at the marker.
    Every,
    /// Only the next play starts at the marker, then the mode reverts to Off.
    Once,
}

/// Persisted per-sound settings, keyed by the sound's file name in `Config::sounds`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SoundSettings {
    /// Linear gain 0.0–1.0 (the tile slider, applied to monitor and virtual mic).
    pub volume: f32,
    /// Start marker in seconds.
    #[serde(default)]
    pub start_secs: f32,
    #[serde(default)]
    pub start_mode: StartMode,
    /// End trim in seconds; 0.0 = play to the end.
    #[serde(default)]
    pub end_secs: f32,
    #[serde(default)]
    pub fade_in_ms: f32,
    #[serde(default)]
    pub fade_out_ms: f32,
}

impl SoundSettings {
    pub fn with_volume(volume: f32) -> Self {
        Self {
            volume,
            start_secs: 0.0,
            start_mode: StartMode::Off,
            end_secs: 0.0,
            fade_in_ms: 0.0,
            fade_out_ms: 0.0,
        }
    }

    /// Effective start offset for the next play, in seconds.
    pub fn effective_start(&self) -> f32 {
        match self.start_mode {
            StartMode::Off => 0.0,
            StartMode::Every | StartMode::Once => self.start_secs.max(0.0),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub sounds_folder: PathBuf,
    pub move_files_to_folder: bool,
    pub polyphonic: bool,
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

    // Soundboard level into the virtual mic (balances sounds against the mic;
    // the monitor path uses monitor_volume instead)
    #[serde(default = "default_unity")]
    pub soundboard_mic_volume: f32,

    // Per-sound settings keyed by file name (volume, start marker, start mode)
    #[serde(default)]
    pub sounds: HashMap<String, SoundSettings>,

    // Tile order on the soundboard, as sound file names; unknown files sort after
    #[serde(default)]
    pub sound_order: Vec<String>,

    // Named effect-chain presets
    #[serde(default)]
    pub effect_presets: HashMap<String, Vec<EffectEntry>>,

    // Effect chain applied to mic input (gate → gain order)
    #[serde(default = "default_effects_chain")]
    pub effects_chain: Vec<EffectEntry>,
}

fn default_unity() -> f32 {
    1.0
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
            default_volume: 100,
            virtual_device_name: "Resonate Microphone".to_string(),
            virtual_device_enabled: true,
            monitor_enabled: true,
            monitor_volume: 1.0,
            input_device_name: String::new(),
            mic_volume: 1.0,
            soundboard_mic_volume: 1.0,
            sounds: HashMap::new(),
            sound_order: Vec::new(),
            effect_presets: HashMap::new(),
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

    // ── Per-sound settings ───────────────────────────────────────────────────

    /// Key into `sounds` for a sound file: its file name (folder-independent).
    pub fn sound_key(path: &std::path::Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Settings for a sound, falling back to a default built from `default_volume`.
    pub fn sound_settings(&self, path: &std::path::Path) -> SoundSettings {
        self.sounds
            .get(&Self::sound_key(path))
            .cloned()
            .unwrap_or_else(|| SoundSettings::with_volume(self.default_volume as f32 / 100.0))
    }

    /// Mutable settings entry for a sound, created from defaults on first access.
    pub fn sound_settings_mut(&mut self, path: &std::path::Path) -> &mut SoundSettings {
        let default = SoundSettings::with_volume(self.default_volume as f32 / 100.0);
        self.sounds.entry(Self::sound_key(path)).or_insert(default)
    }

    /// Carry settings over when a sound file is renamed.
    pub fn rename_sound_key(&mut self, old_path: &std::path::Path, new_path: &std::path::Path) {
        if let Some(s) = self.sounds.remove(&Self::sound_key(old_path)) {
            self.sounds.insert(Self::sound_key(new_path), s);
        }
    }

    pub fn remove_sound_settings(&mut self, path: &std::path::Path) {
        self.sounds.remove(&Self::sound_key(path));
    }
}
