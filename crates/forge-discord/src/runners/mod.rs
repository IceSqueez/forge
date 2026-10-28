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

    struct IdSink {
        fail: bool,
    }

    impl IdSink {
        fn outcome(&self) -> Result<String, DiscordError> {
            if self.fail {
                Err(DiscordError::Connect("down".to_owned()))
            } else {
                Ok("987654321".to_owned())
            }
        }
    }

    #[async_trait]
    impl DiscordSink for IdSink {
        async fn post_text(
            &self,
            _: &str,
            _: &str,
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
            self.outcome()
        }
        async fn post_embed(&self, _: &str, _: DiscordEmbed) -> Result<String, DiscordError> {
            self.outcome()
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
            self.outcome()
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    struct SilentPublisher;
    impl forge_events::EventPublisher for SilentPublisher {
        fn publish(&self, _: forge_events::Event) {}
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    async fn run_posting_runners(
        fail: bool,
        file_path: &str,
    ) -> Vec<(&'static str, Option<ArgStack>, SubActionIo)> {
        use forge_registry::{RunContext, SubActionRunner};
        use forge_types::EventId;

        let sink: Arc<dyn DiscordSink> = Arc::new(IdSink { fail });
        let stack = ArgStack::new().set("user".to_owned(), text("alice"));
        let publisher = SilentPublisher;
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);

        let post_text_runner = PostTextRunner::new(Arc::clone(&sink));
        let post_embed_runner = PostEmbedRunner::new(Arc::clone(&sink));
        let send_file_runner = SendFileRunner::new(Arc::clone(&sink));

        let mut text_config = post_text_runner.default_config();
        text_config.insert("webhook_name".to_owned(), text("alerts"));
        text_config.insert("content".to_owned(), text("hello"));
        let mut embed_config = post_embed_runner.default_config();
        embed_config.insert("webhook_name".to_owned(), text("alerts"));
        embed_config.insert("embed_title".to_owned(), text("title"));
        let mut file_config = send_file_runner.default_config();
        file_config.insert("webhook_name".to_owned(), text("alerts"));
        file_config.insert("file_path".to_owned(), text(file_path));

        vec![
            (
                "post_text",
                post_text_runner.execute(&text_config, &ctx).await.1,
                post_text_runner.scope_io(),
            ),
            (
                "post_embed",
                post_embed_runner.execute(&embed_config, &ctx).await.1,
                post_embed_runner.scope_io(),
            ),
            (
                "send_file",
                send_file_runner.execute(&file_config, &ctx).await.1,
                send_file_runner.scope_io(),
            ),
        ]
    }

    #[tokio::test]
    async fn posting_runners_keep_existing_variables_and_publish_the_message_id() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.txt");
        std::fs::write(&file, b"data").unwrap();

        for (name, produced, _) in run_posting_runners(false, file.to_str().unwrap()).await {
            let stack = produced.unwrap_or_else(|| panic!("{name} returned no stack"));
            assert_eq!(
                stack.get("user"),
                Some(&text("alice")),
                "{name} dropped a variable"
            );
            assert_eq!(
                stack.get(MESSAGE_ID_VAR),
                Some(&text("987654321")),
                "{name} did not publish the id"
            );
            assert_eq!(stack.interpolate("%discord.message_id%"), "987654321");
        }
    }

    #[tokio::test]
    async fn posting_runners_return_no_stack_when_the_sink_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.txt");
        std::fs::write(&file, b"data").unwrap();

        for (name, produced, _) in run_posting_runners(true, file.to_str().unwrap()).await {
            assert!(produced.is_none(), "{name} returned a stack on failure");
        }
    }

    #[tokio::test]
    async fn posting_runners_declare_the_message_id_as_a_fixed_output() {
        for (name, _, io) in run_posting_runners(true, "").await {
            assert_eq!(io.fixed_outputs, vec![MESSAGE_ID_VAR.to_owned()], "{name}");
        }
    }
}
