use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::send_chat::YoutubeSendChat;

const KIND_ID: &str = "youtube.chat.create_poll";
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 4;

pub struct CreatePollRunner {
    sender: Arc<YoutubeSendChat>,
}

impl CreatePollRunner {
    pub fn new(sender: Arc<YoutubeSendChat>) -> Self {
        Self { sender }
    }
}

/// Returns Err if the option count falls outside `[MIN_OPTIONS, MAX_OPTIONS]`.
fn parse_options(raw: &str) -> Result<Vec<String>, String> {
    let options: Vec<String> = raw
        .split(['\n', ','])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if options.len() < MIN_OPTIONS {
        return Err(format!("at least {MIN_OPTIONS} options required"));
    }
    if options.len() > MAX_OPTIONS {
        return Err(format!(
            "too many options: max {MAX_OPTIONS}, got {}",
            options.len()
        ));
    }
    Ok(options)
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("question".to_owned(), Variant::String(String::new())),
            ("options".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "question",
                label: "Question",
                placeholder: "What next?",
            },
            FormField::TextArea {
                key: "options",
                label: "Options (one per line, 2-4)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("question") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'question' is required"
                )));
            }
        }

        let raw_options = match config.get("options") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        parse_options(raw_options)
            .map(|_| ())
            .map_err(|msg| RegistryError::InvalidConfig(format!("{KIND_ID}: {msg}")))
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let question_template = config.str("question").unwrap_or_default();
        let question = ctx.arg_stack.interpolate(question_template);

        if question.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "question is empty after interpolation".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        let options_template = config.str("options").unwrap_or_default();
        let raw_options = ctx.arg_stack.interpolate(options_template);

        let options = match parse_options(&raw_options) {
            Ok(v) => v,
            Err(msg) => {
                return (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Failed(msg),
                        index: ctx.index,
                    },
                    None,
                );
            }
        };

        let outcome =
            SubActionOutcome::from_result(&self.sender.create_poll(&question, &options).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use tokio::sync::Mutex;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::quota_state::QuotaState;

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
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let sender =
            YoutubeSendChat::new(token_source(), handle, quota).with_api_base(server.uri());
        CreatePollRunner::new(Arc::new(sender))
    }

    fn config(question: &str, options: &str) -> SubActionConfig {
        BTreeMap::from([
            ("question".to_owned(), Variant::String(question.to_owned())),
            ("options".to_owned(), Variant::String(options.to_owned())),
        ])
    }

    #[test]
    fn parse_options_splits_trims_filters_and_enforces_count() {
        assert_eq!(parse_options("Red,Blue").unwrap(), vec!["Red", "Blue"]);
        assert_eq!(
            parse_options("A\nB\nC\nD").unwrap(),
            vec!["A", "B", "C", "D"]
        );
        assert_eq!(parse_options("  a , b ").unwrap(), vec!["a", "b"]);
        assert_eq!(parse_options("a\n\n,b").unwrap(), vec!["a", "b"]);
        assert_eq!(parse_options("a,b\nc").unwrap(), vec!["a", "b", "c"]);

        assert!(parse_options("only-one").is_err(), "1 option below min");
        assert!(parse_options("").is_err(), "empty is below min");
        assert!(parse_options("a,b,c,d,e").is_err(), "5 options above max");
    }

    #[test]
    fn validate_config_requires_question_and_valid_option_count() {
        let runner = {
            let handle = LiveChatIdHandle::new();
            let quota = Arc::new(Mutex::new(QuotaState::default()));
            let sender = YoutubeSendChat::new(token_source(), handle, quota)
                .with_api_base("http://127.0.0.1:0".to_owned());
            CreatePollRunner::new(Arc::new(sender))
        };

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid", config("Q?", "A,B"), true),
            ("empty question", config("", "A,B"), false),
            ("one option", config("Q?", "A"), false),
            ("five options", config("Q?", "A,B,C,D,E"), false),
            ("missing question", BTreeMap::new(), false),
        ];
        for (label, cfg, ok) in cases {
            assert_eq!(runner.validate_config(&cfg).is_ok(), ok, "case: {label}");
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
