pub mod builtin;
pub mod host;
pub mod lv2;

use crate::config::EffectEntry;

/// Build a boxed plugin from a saved chain entry. Built-ins by id; anything with
/// the `lv2:` prefix is hosted via [`lv2::Lv2Plugin`]. Returns `None` if the
/// effect id is unknown or an LV2 plugin failed to instantiate.
pub fn plugin_from_entry(entry: &EffectEntry) -> Option<Box<dyn ResonatePlugin>> {
    let p: Box<dyn ResonatePlugin> = match entry.id.as_str() {
        "gain" => Box::new(builtin::GainPlugin::new(
            entry.params.get("gain").copied().unwrap_or(1.0),
            entry.enabled,
        )),
        "gate" => Box::new(builtin::NoiseGatePlugin::new(
            entry.params.get("threshold").copied().unwrap_or(0.02),
            entry.params.get("attack_ms").copied().unwrap_or(10.0),
            entry.params.get("release_ms").copied().unwrap_or(100.0),
            entry.enabled,
        )),
        other => {
            let uri = lv2::uri_from_id(other)?;
            let mut p = lv2::Lv2Plugin::instantiate(uri, &entry.params)?;
            p.set_enabled(entry.enabled);
            return Some(Box::new(p));
        }
    };
    Some(p)
}

/// Core trait every Resonate effect plugin must implement.
pub trait ResonatePlugin: Send {
    fn name(&self) -> &str;
    fn id(&self) -> &str;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    fn process(&mut self, samples: &mut [f32], sample_rate: u32);
    fn params(&self) -> Vec<PluginParam>;
    fn set_param(&mut self, id: &str, value: f32);
}

#[derive(Debug, Clone)]
pub struct PluginParam {
    pub id: String,
    pub label: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub value: f32,
}
