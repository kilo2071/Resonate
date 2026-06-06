use super::{plugin_from_entry, PluginParam, ResonatePlugin};
use crate::config::EffectEntry;

/// Ordered chain of mic effects. Applied sequentially in-place.
pub struct PluginChain {
    pub plugins: Vec<Box<dyn ResonatePlugin>>,
}

impl PluginChain {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    /// Build a chain from saved config entries, skipping any that fail to load.
    pub fn from_entries(entries: &[EffectEntry]) -> Self {
        let mut plugins = Vec::new();
        for entry in entries {
            match plugin_from_entry(entry) {
                Some(p) => plugins.push(p),
                None => log::warn!("Skipping effect '{}' (failed to load)", entry.id),
            }
        }
        Self { plugins }
    }

    pub fn process(&mut self, samples: &mut [f32], sample_rate: u32) {
        for plugin in &mut self.plugins {
            plugin.process(samples, sample_rate);
        }
    }

    pub fn set_enabled(&mut self, idx: usize, enabled: bool) {
        if let Some(p) = self.plugins.get_mut(idx) {
            p.set_enabled(enabled);
        }
    }

    pub fn set_param(&mut self, idx: usize, id: &str, value: f32) {
        if let Some(p) = self.plugins.get_mut(idx) {
            p.set_param(id, value);
        }
    }

    /// Parameters of the plugin at `idx` (for the effects UI).
    pub fn params_for(&self, idx: usize) -> Vec<PluginParam> {
        self.plugins.get(idx).map(|p| p.params()).unwrap_or_default()
    }
}

impl Default for PluginChain {
    fn default() -> Self {
        Self::new()
    }
}
