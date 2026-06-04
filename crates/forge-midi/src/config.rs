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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_client_name_is_forge() {
        assert_eq!(MidiConfig::default().client_name, "forge");
    }
}
