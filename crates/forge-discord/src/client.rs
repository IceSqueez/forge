use std::sync::Arc;

use forge_events::EventPublisher;
use forge_platform_core::BuiltinId;
use forge_storage::CredentialsRepo;
use tokio::sync::broadcast;

use crate::config::DiscordConfig;
use crate::embed::DiscordEmbed;
use crate::error::DiscordError;
use crate::sink::DiscordSink;

pub(crate) type HealthTx = broadcast::Sender<forge_platform_core::HealthDelta>;

#[allow(dead_code)]
pub struct DiscordClient {
    pub(crate) id: BuiltinId,
    pub(crate) config: DiscordConfig,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) http: reqwest::Client,
    pub(crate) health_tx: HealthTx,
}

impl DiscordClient {
    pub fn new(
        config: DiscordConfig,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            id: BuiltinId::new("discord"),
            config,
            publisher,
            creds,
            http: reqwest::Client::new(),
            health_tx,
        })
    }
}

#[async_trait::async_trait]
impl DiscordSink for DiscordClient {
    async fn post_text(&self, _webhook_name: &str, _content: &str) -> Result<String, DiscordError> {
        Ok(String::new())
    }

    async fn post_embed(
        &self,
        _webhook_name: &str,
        _embed: DiscordEmbed,
    ) -> Result<String, DiscordError> {
        Ok(String::new())
    }

    async fn edit_message(
        &self,
        _webhook_name: &str,
        _message_id: &str,
        _content: Option<&str>,
        _embed: Option<DiscordEmbed>,
    ) -> Result<(), DiscordError> {
        Ok(())
    }
}
