#[derive(Debug, Clone)]
pub struct MidiConfig {
    pub client_name: String,
}

impl Default for MidiConfig {
    fn default() -> Self {
        Self {
            client_name: "forge".to_owned(),
        }
    }
}
