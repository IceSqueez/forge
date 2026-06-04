use async_trait::async_trait;

use crate::embed::DiscordEmbed;
use crate::error::DiscordError;

#[async_trait]
pub trait DiscordSink: Send + Sync {
    async fn post_text(&self, webhook_name: &str, content: &str) -> Result<String, DiscordError>;

    async fn post_embed(
        &self,
        webhook_name: &str,
        embed: DiscordEmbed,
    ) -> Result<String, DiscordError>;

    async fn edit_message(
        &self,
        webhook_name: &str,
        message_id: &str,
        content: Option<&str>,
        embed: Option<DiscordEmbed>,
    ) -> Result<(), DiscordError>;
}
