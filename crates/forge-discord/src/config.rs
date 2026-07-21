use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub request_timeout: Duration,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(10),
        }
    }
}
