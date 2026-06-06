pub mod builtin;
pub mod host;

/// Core trait every Resonate effect plugin must implement.
pub trait ResonatePlugin: Send {
    fn name(&self) -> &str;
    fn process(&mut self, input: &[f32], output: &mut [f32], sample_rate: u32);
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
}
