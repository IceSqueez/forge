use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub base_url: String,
    pub request_timeout: Duration,
    pub max_retries: u32,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            base_url: "https://discord.com/api".to_owned(),
            request_timeout: Duration::from_secs(10),
            max_retries: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url() {
        assert_eq!(DiscordConfig::default().base_url, "https://discord.com/api");
    }

    #[test]
    fn default_max_retries() {
        assert_eq!(DiscordConfig::default().max_retries, 1);
    }
}
