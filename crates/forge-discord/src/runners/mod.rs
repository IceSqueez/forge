mod delete_message;
mod edit_message;
mod post_embed;
mod post_text;
mod send_file;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

use crate::client::DiscordClient;
use crate::sink::DiscordSink;

pub use delete_message::DeleteMessageRunner;
pub use edit_message::EditMessageRunner;
pub use post_embed::PostEmbedRunner;
pub use post_text::PostTextRunner;
pub use send_file::SendFileRunner;

pub fn register_discord_sub_actions(
    reg: &mut SubActionRegistry,
    client: Arc<DiscordClient>,
) -> Result<(), RegistryError> {
    let sink: Arc<dyn DiscordSink> = client;
    reg.register(Box::new(PostTextRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(PostEmbedRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(EditMessageRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SendFileRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(DeleteMessageRunner::new(Arc::clone(&sink))))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use async_trait::async_trait;

    struct MockSink;

    #[async_trait]
    impl DiscordSink for MockSink {
        async fn post_text(&self, _: &str, _: &str) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn post_embed(&self, _: &str, _: DiscordEmbed) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn edit_message(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<DiscordEmbed>,
        ) -> Result<(), DiscordError> {
            Ok(())
        }
        async fn send_file(
            &self,
            _: &str,
            _: Option<&str>,
            _: &str,
            _: &[u8],
        ) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    #[test]
    fn register_discord_sub_actions_registers_three_runners() {
        let mut reg = SubActionRegistry::new();
        let sink: Arc<dyn DiscordSink> = Arc::new(MockSink);
        reg.register(Box::new(PostTextRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(PostEmbedRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(EditMessageRunner::new(Arc::clone(&sink))))
            .unwrap();
        assert_eq!(reg.all().count(), 3);
    }

    #[test]
    fn all_expected_runner_ids_are_present() {
        let mut reg = SubActionRegistry::new();
        let sink: Arc<dyn DiscordSink> = Arc::new(MockSink);
        reg.register(Box::new(PostTextRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(PostEmbedRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(EditMessageRunner::new(Arc::clone(&sink))))
            .unwrap();
        for id in &[
            "discord.webhook.send_message",
            "discord.webhook.send_embed",
            "discord.webhook.update_message",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
        }
    }
}
