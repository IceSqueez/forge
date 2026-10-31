mod delete_message;
mod edit_message;
mod fields;
mod mention_fields;
mod post_embed;
mod post_text;
mod send_file;
mod step_failure;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionIo, SubActionRegistry};
use forge_types::{ArgStack, Variant};

use crate::client::DiscordClient;
use crate::error::DiscordError;
use crate::sink::DiscordSink;
use crate::vocabulary::variables;

pub(crate) use delete_message::DeleteMessageRunner;
pub(crate) use edit_message::EditMessageRunner;
pub(crate) use post_embed::PostEmbedRunner;
pub(crate) use post_text::PostTextRunner;
pub(crate) use send_file::SendFileRunner;

pub(crate) fn message_id_scope_io() -> SubActionIo {
    SubActionIo {
        fixed_outputs: vec![variables::MESSAGE_ID.to_owned()],
        ..SubActionIo::default()
    }
}

pub(crate) fn message_id_stack(
    stack: &ArgStack,
    posted: &Result<String, DiscordError>,
) -> Option<ArgStack> {
    posted.as_ref().ok().map(|id| {
        stack.clone().set(
            variables::MESSAGE_ID.to_owned(),
            Variant::String(id.clone()),
        )
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
    use forge_events::test_support::NullPublisher;

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
        let publisher = NullPublisher;
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
                stack.get(variables::MESSAGE_ID),
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
            assert_eq!(
                io.fixed_outputs,
                vec![variables::MESSAGE_ID.to_owned()],
                "{name}"
            );
        }
    }

    const STORED_DEFAULT_CONFIGS: [(&str, &str); 5] = [
        (
            "discord.webhook.send_message",
            r#"{"allow_everyone_pings":{"type":"bool","value":false},"allow_role_pings":{"type":"bool","value":true},"content":{"type":"string","value":""},"webhook_name":{"type":"string","value":""}}"#,
        ),
        (
            "discord.webhook.send_embed",
            r#"{"embed_description":{"type":"string","value":""},"embed_title":{"type":"string","value":""},"webhook_name":{"type":"string","value":""}}"#,
        ),
        (
            "discord.webhook.update_message",
            r#"{"allow_everyone_pings":{"type":"bool","value":false},"allow_role_pings":{"type":"bool","value":true},"content":{"type":"string","value":""},"message_id":{"type":"string","value":""},"webhook_name":{"type":"string","value":""}}"#,
        ),
        (
            "discord.webhook.send_file",
            r#"{"allow_everyone_pings":{"type":"bool","value":false},"allow_role_pings":{"type":"bool","value":true},"content":{"type":"string","value":""},"file_path":{"type":"string","value":""},"webhook_name":{"type":"string","value":""}}"#,
        ),
        (
            "discord.webhook.delete_message",
            r#"{"message_id":{"type":"string","value":""},"webhook_name":{"type":"string","value":""}}"#,
        ),
    ];

    fn all_runners(sink: &Arc<dyn DiscordSink>) -> Vec<Box<dyn forge_registry::SubActionRunner>> {
        vec![
            Box::new(PostTextRunner::new(Arc::clone(sink))),
            Box::new(PostEmbedRunner::new(Arc::clone(sink))),
            Box::new(EditMessageRunner::new(Arc::clone(sink))),
            Box::new(SendFileRunner::new(Arc::clone(sink))),
            Box::new(DeleteMessageRunner::new(Arc::clone(sink))),
        ]
    }

    fn config_of(entries: &[(&str, &str)]) -> forge_types::SubActionConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), text(value)))
            .collect()
    }

    #[test]
    fn every_runner_seeds_the_same_stored_default_config_as_before_declared_fields() {
        let sink: Arc<dyn DiscordSink> = Arc::new(MockSink);
        let runners = all_runners(&sink);
        assert_eq!(runners.len(), STORED_DEFAULT_CONFIGS.len());

        for (id, stored) in STORED_DEFAULT_CONFIGS {
            let runner = runners
                .iter()
                .find(|runner| runner.id() == id)
                .unwrap_or_else(|| panic!("missing runner: {id}"));
            let expected: serde_json::Value = serde_json::from_str(stored).unwrap();

            assert_eq!(
                serde_json::to_value(runner.default_config()).unwrap(),
                expected,
                "{id} seeds a different default config"
            );
        }
    }

    #[derive(Default)]
    struct CountingSink {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl CountingSink {
        fn called(&self) {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl DiscordSink for CountingSink {
        async fn post_text(
            &self,
            _: &str,
            _: &str,
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
            self.called();
            Ok(String::new())
        }
        async fn post_embed(&self, _: &str, _: DiscordEmbed) -> Result<String, DiscordError> {
            self.called();
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
            self.called();
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
            self.called();
            Ok(String::new())
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            self.called();
            Ok(())
        }
    }

    fn invalid_config(
        kind: &str,
        problem: forge_registry::ConfigProblem,
    ) -> forge_types::SubActionOutcome {
        let message = problem.message(|keys| forge_types::MessageArg::StepFields {
            kind: kind.to_owned(),
            keys: keys.to_vec(),
        });
        forge_types::SubActionOutcome::Failed(message.into())
    }

    #[tokio::test]
    async fn required_fields_that_resolve_to_blank_fail_without_calling_the_sink() {
        use forge_registry::{ConfigProblem, ConfigReason, RunContext};
        use forge_types::EventId;

        let base = [
            ("discord.webhook.send_message", vec![("content", "hi")]),
            ("discord.webhook.send_embed", vec![("embed_title", "Live")]),
            (
                "discord.webhook.update_message",
                vec![("message_id", "42"), ("content", "hi")],
            ),
            (
                "discord.webhook.send_file",
                vec![("file_path", "/nonexistent/clip.png")],
            ),
            ("discord.webhook.delete_message", vec![("message_id", "42")]),
        ];
        let cases = [
            ("discord.webhook.send_message", "webhook_name"),
            ("discord.webhook.send_message", "content"),
            ("discord.webhook.send_embed", "webhook_name"),
            ("discord.webhook.update_message", "webhook_name"),
            ("discord.webhook.update_message", "message_id"),
            ("discord.webhook.send_file", "webhook_name"),
            ("discord.webhook.send_file", "file_path"),
            ("discord.webhook.delete_message", "webhook_name"),
            ("discord.webhook.delete_message", "message_id"),
        ];
        let stack = ArgStack::new().set("blank".to_owned(), text(" \t "));
        let publisher = NullPublisher;

        for (id, key) in cases {
            let counting = Arc::new(CountingSink::default());
            let sink: Arc<dyn DiscordSink> = counting.clone();
            let runners = all_runners(&sink);
            let runner = runners.iter().find(|runner| runner.id() == id).unwrap();
            let entries = base.iter().find(|(base_id, _)| *base_id == id).unwrap();
            let mut config = config_of(&entries.1);
            config.insert("webhook_name".to_owned(), text("alerts"));
            config.insert(key.to_owned(), text("%blank%"));

            assert!(runner.validate_config(&config).is_ok(), "{id} {key}");
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
            let (telemetry, _) = runner.execute(&config, &ctx).await;

            assert_eq!(
                telemetry.outcome,
                invalid_config(id, ConfigProblem::field(key, ConfigReason::Required)),
                "{id} {key}"
            );
            assert_eq!(counting.calls(), 0, "{id} {key}");
        }
    }

    fn any_of(keys: [&str; 2]) -> forge_registry::ConfigProblem {
        forge_registry::ConfigProblem::whole(forge_registry::ConfigReason::any_of(keys))
    }

    #[test]
    fn content_steps_validate_only_with_at_least_one_content_field() {
        use forge_registry::SubActionRunner;

        let embed = PostEmbedRunner::new(Arc::new(MockSink));
        let edit = EditMessageRunner::new(Arc::new(MockSink));
        let embed_needs = Some(any_of(["embed_title", "embed_description"]));
        let edit_needs = Some(any_of(["content", "embed_title"]));
        type Case<'a> = (
            &'a dyn SubActionRunner,
            Vec<(&'a str, &'a str)>,
            Option<forge_registry::ConfigProblem>,
        );
        let cases: [Case<'_>; 10] = [
            (&embed, vec![], embed_needs.clone()),
            (
                &embed,
                vec![("embed_title", ""), ("embed_description", "")],
                embed_needs,
            ),
            (&embed, vec![("embed_title", "Live")], None),
            (&embed, vec![("embed_description", "Now playing")], None),
            (&embed, vec![("embed_title", "%title%")], None),
            (&edit, vec![], edit_needs.clone()),
            (&edit, vec![("content", "")], edit_needs),
            (&edit, vec![("embed_title", "Live")], None),
            (&edit, vec![("content", "Ended")], None),
            (&edit, vec![("content", "%summary%")], None),
        ];

        for (runner, entries, expected) in cases {
            let mut config = config_of(&entries);
            config.insert("webhook_name".to_owned(), text("alerts"));
            config.insert("message_id".to_owned(), text("42"));
            let problem = runner
                .validate_config(&config)
                .err()
                .map(|error| error.into_config_problem());

            assert_eq!(problem, expected, "{} {entries:?}", runner.id());
        }
    }

    #[test]
    fn content_steps_holding_only_whitespace_content_fail_validation() {
        use forge_registry::SubActionRunner;

        let embed = PostEmbedRunner::new(Arc::new(MockSink));
        let edit = EditMessageRunner::new(Arc::new(MockSink));
        type Case<'a> = (
            &'a dyn SubActionRunner,
            Vec<(&'a str, &'a str)>,
            [&'a str; 2],
        );
        let cases: [Case<'_>; 2] = [
            (
                &embed,
                vec![("embed_title", "  "), ("embed_description", "\t")],
                ["embed_title", "embed_description"],
            ),
            (&edit, vec![("content", "  ")], ["content", "embed_title"]),
        ];

        for (runner, entries, needs) in cases {
            let mut config = config_of(&entries);
            config.insert("webhook_name".to_owned(), text("alerts"));
            config.insert("message_id".to_owned(), text("42"));
            let problem = runner
                .validate_config(&config)
                .err()
                .map(|error| error.into_config_problem());

            assert_eq!(problem, Some(any_of(needs)), "{} {entries:?}", runner.id());
        }
    }

    #[tokio::test]
    async fn content_steps_whose_content_resolves_empty_fail_without_calling_the_sink() {
        use forge_registry::{RunContext, SubActionRunner};
        use forge_types::EventId;

        let counting = Arc::new(CountingSink::default());
        let sink: Arc<dyn DiscordSink> = counting.clone();
        let embed = PostEmbedRunner::new(Arc::clone(&sink));
        let edit = EditMessageRunner::new(Arc::clone(&sink));
        let cases: [(&dyn SubActionRunner, &str, _); 2] = [
            (
                &embed,
                "embed_title",
                any_of(["embed_title", "embed_description"]),
            ),
            (&edit, "content", any_of(["content", "embed_title"])),
        ];
        let stack = ArgStack::new().set("empty".to_owned(), text(""));
        let publisher = NullPublisher;

        for (runner, key, problem) in cases {
            let config = config_of(&[
                ("webhook_name", "alerts"),
                ("message_id", "42"),
                (key, "%empty%"),
            ]);
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);

            let (telemetry, _) = runner.execute(&config, &ctx).await;

            assert_eq!(
                telemetry.outcome,
                invalid_config(runner.id(), problem),
                "{}",
                runner.id()
            );
        }
        assert_eq!(counting.calls(), 0);
    }
}
