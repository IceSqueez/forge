use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, FieldLiteral, FormField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText, Variant};

use crate::send_chat::YoutubeSendChat;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_CREATE_POLL;
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 4;
const OPTION_SEPARATORS: [char; 2] = ['\n', ','];

const QUESTION: TextField = TextField {
    key: config_keys::QUESTION,
    label: "Question",
    editor: TextEditor::Line {
        placeholder: "Poll question",
    },
    default: "",
    required: true,
};

struct PollOptionsField;

const OPTIONS: PollOptionsField = PollOptionsField;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&QUESTION, &OPTIONS]);

fn parse_options(raw: &str) -> Result<Vec<String>, ConfigReason> {
    let options: Vec<String> = raw
        .split(OPTION_SEPARATORS)
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect();
    if options.is_empty() {
        Err(ConfigReason::Required)
    } else if (MIN_OPTIONS..=MAX_OPTIONS).contains(&options.len()) {
        Ok(options)
    } else {
        Err(ConfigReason::CountOutOfRange(ValueBounds::between(
            MIN_OPTIONS,
            MAX_OPTIONS,
        )))
    }
}

impl TypedField for PollOptionsField {
    type Value = Vec<String>;

    fn key(&self) -> &'static str {
        config_keys::OPTIONS
    }

    fn form_field(&self) -> FormField {
        FormField::TextArea {
            key: config_keys::OPTIONS,
            label: "Options (one per line, 2-4)",
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Vec<String>>, ConfigReason> {
        match value {
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => parse_options(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Vec<String>, ConfigReason> {
        parse_options(text)
    }
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, Vec<String>), ConfigProblem> {
    Ok((QUESTION.read(config, args)?, OPTIONS.read(config, args)?))
}

pub struct CreatePollRunner {
    sender: Arc<YoutubeSendChat>,
}

impl CreatePollRunner {
    pub fn new(sender: Arc<YoutubeSendChat>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl SubActionRunner for CreatePollRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::PollsPredictions
    }

    fn label(&self) -> &str {
        "Create Live Poll"
    }

    fn summary(&self) -> &str {
        "Posts a poll to the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube poll vote create question options live chat"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (question, options) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if question.is_empty() {
            return (
                timer.failed(UserText::unlocalized(
                    "question is empty after interpolation",
                )),
                None,
            );
        }

        let outcome =
            SubActionOutcome::from_result(&self.sender.create_poll(&question, &options).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use forge_registry::RegistryError;
    use std::collections::BTreeMap;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source() -> Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok("poll-token".to_owned()) }))
    }

    fn runner_on(server: &MockServer, live: bool) -> CreatePollRunner {
        let handle = LiveChatIdHandle::new();
        if live {
            handle.set(Some("lc".to_owned()));
        }
        let sender = YoutubeSendChat::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            handle,
        );
        CreatePollRunner::new(Arc::new(sender))
    }

    fn config(question: &str, options: &str) -> SubActionConfig {
        BTreeMap::from([
            ("question".to_owned(), Variant::String(question.to_owned())),
            ("options".to_owned(), Variant::String(options.to_owned())),
        ])
    }

    fn offline_runner() -> CreatePollRunner {
        let sender = YoutubeSendChat::new(
            crate::endpoints_test_support::data_api_at("http://127.0.0.1:0"),
            token_source(),
            LiveChatIdHandle::new(),
        );
        CreatePollRunner::new(Arc::new(sender))
    }

    fn option_count_out_of_range() -> ConfigReason {
        ConfigReason::CountOutOfRange(ValueBounds::between(2, 4))
    }

    #[test]
    fn parse_options_splits_on_commas_and_newlines_trimming_and_dropping_blanks() {
        for (raw, expected) in [
            ("Red,Blue", vec!["Red", "Blue"]),
            ("A\nB\nC\nD", vec!["A", "B", "C", "D"]),
            ("  a , b ", vec!["a", "b"]),
            ("a\n\n,b", vec!["a", "b"]),
            ("a,b\nc", vec!["a", "b", "c"]),
            ("Так,Ні", vec!["Так", "Ні"]),
        ] {
            assert_eq!(
                parse_options(raw),
                Ok(expected.iter().map(|o| (*o).to_owned()).collect()),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn parse_options_rejects_no_options_as_required_and_counts_outside_two_to_four() {
        for (raw, reason) in [
            ("", ConfigReason::Required),
            (" , \n ", ConfigReason::Required),
            ("only-one", option_count_out_of_range()),
            ("a,b,c,d,e", option_count_out_of_range()),
        ] {
            assert_eq!(parse_options(raw), Err(reason), "{raw:?}");
        }
    }

    #[test]
    fn validate_config_requires_question_and_valid_option_count() {
        let runner = offline_runner();
        let question_required = Some(ConfigProblem::field("question", ConfigReason::Required));
        let options_problem = |reason| Some(ConfigProblem::field("options", reason));

        for (label, cfg, expected) in [
            ("valid", config("Q?", "A,B"), None),
            ("templated options", config("Q?", "%choices%"), None),
            (
                "empty question",
                config("", "A,B"),
                question_required.clone(),
            ),
            (
                "whitespace-only question",
                config(" \t ", "A,B"),
                question_required.clone(),
            ),
            ("missing question", BTreeMap::new(), question_required),
            (
                "one option",
                config("Q?", "A"),
                options_problem(option_count_out_of_range()),
            ),
            (
                "five options",
                config("Q?", "A,B,C,D,E"),
                options_problem(option_count_out_of_range()),
            ),
            (
                "blank options",
                config("Q?", " "),
                options_problem(ConfigReason::Required),
            ),
        ] {
            assert_eq!(
                runner
                    .validate_config(&cfg)
                    .err()
                    .map(RegistryError::into_config_problem),
                expected,
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn options_template_resolving_to_a_bad_count_fails_without_send() {
        let stack = ArgStack::new()
            .set("none".to_owned(), Variant::String(" , ".to_owned()))
            .set("one".to_owned(), Variant::String("Pizza".to_owned()))
            .set("five".to_owned(), Variant::String("a,b,c,d,e".to_owned()));
        for (template, reason) in [
            ("%none%", ConfigReason::Required),
            ("%one%", option_count_out_of_range()),
            ("%five%", option_count_out_of_range()),
        ] {
            let server = MockServer::start().await;
            let runner = runner_on(&server, true);

            let (telemetry, _) = runner
                .execute(&config("Q?", template), &make_ctx(&stack))
                .await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Failed(format!(
                    "invalid step config: {}",
                    ConfigProblem::field("options", reason)
                )),
                "{template}"
            );
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "{template}"
            );
        }
    }

    #[tokio::test]
    async fn empty_question_after_interpolation_fails_without_send() {
        let server = MockServer::start().await;
        let runner = runner_on(&server, true);
        let stack = ArgStack::new().set("q".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%q%", "A,B"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_interpolates_question_and_posts_poll() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server, true);
        let stack = ArgStack::new().set("topic".to_owned(), Variant::String("dinner".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Pick %topic%?", "Pizza,Sushi"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(
            body["snippet"]["pollDetails"]["metadata"]["questionText"],
            "Pick dinner?"
        );
    }
}
