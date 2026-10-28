mod delete_message;
mod edit_message;
mod mention_fields;
mod post_embed;
mod post_text;
mod send_file;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionIo, SubActionRegistry};
use forge_types::{ArgStack, Variant};

use crate::client::DiscordClient;
use crate::error::DiscordError;
use crate::sink::DiscordSink;

pub use delete_message::DeleteMessageRunner;
pub use edit_message::EditMessageRunner;
pub use post_embed::PostEmbedRunner;
pub use post_text::PostTextRunner;
pub use send_file::SendFileRunner;

pub(crate) const MESSAGE_ID_VAR: &str = "discord.message_id";

pub(crate) fn message_id_scope_io() -> SubActionIo {
    SubActionIo {
        fixed_outputs: vec![MESSAGE_ID_VAR.to_owned()],
        ..SubActionIo::default()
    }
}

pub(crate) fn message_id_stack(
    stack: &ArgStack,
    posted: &Result<String, DiscordError>,
) -> Option<ArgStack> {
    posted.as_ref().ok().map(|id| {
        stack
            .clone()
            .set(MESSAGE_ID_VAR.to_owned(), Variant::String(id.clone()))
    })
}

pub fn register_discord_sub_actions(
    reg: &mut SubActionRegistry,
    client: Arc<DiscordClient>,
) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::DISCORD_INTEGRATION.id);
    let sink: Arc<dyn DiscordSink> = client;
    reg.register(Box::new(PostTextRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(PostEmbedRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(EditMessageRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(SendFileRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(DeleteMessageRunner::new(Arc::clone(&sink))))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use crate::mention::MentionPolicy;
    use async_trait::async_trait;

    struct MockSink;

    #[async_trait]
    impl DiscordSink for MockSink {
        async fn post_text(
            &self,
            _: &str,
            _: &str,
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
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
            _: MentionPolicy,
        ) -> Result<(), DiscordError> {
            Ok(())
        }
        async fn send_file(
            &self,
            _: &str,
            _: Option<&str>,
            _: &str,
            _: &[u8],
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    #[test]
    fn registering_all_runners_exposes_every_expected_id() {
        let mut reg = SubActionRegistry::new();
        let sink: Arc<dyn DiscordSink> = Arc::new(MockSink);
        reg.register(Box::new(PostTextRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(PostEmbedRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(EditMessageRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(SendFileRunner::new(Arc::clone(&sink))))
            .unwrap();
        reg.register(Box::new(DeleteMessageRunner::new(Arc::clone(&sink))))
            .unwrap();
        for id in &[
            "discord.webhook.send_message",
            "discord.webhook.send_embed",
            "discord.webhook.update_message",
            "discord.webhook.send_file",
            "discord.webhook.delete_message",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
        }
    }

    #[test]
    fn post_text_keys_a_missing_webhook_before_a_missing_content() {
        use forge_registry::SubActionRunner;
        use forge_types::Variant;

        let runner = PostTextRunner::new(Arc::new(MockSink));
        let text = |value: &str| Variant::String(value.to_owned());
        for (entries, expected) in [
            (vec![], Some("webhook_name")),
            (vec![("content", text("hi"))], Some("webhook_name")),
            (vec![("webhook_name", text("alerts"))], Some("content")),
            (
                vec![("webhook_name", text("alerts")), ("content", text("hi"))],
                None,
            ),
        ] {
            let config = entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect();
            let field = runner
                .validate_config(&config)
                .err()
                .and_then(|error| error.into_config_problem().field);
            assert_eq!(field.as_deref(), expected);
        }
    }
}
