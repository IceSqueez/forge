use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, Interpolation, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};
use futures::future::BoxFuture;

use super::field_kinds::{Blankable, CommaListField, WholeNumberField, non_empty, stores_blank};
use crate::api::channel::KickChannel;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_UPDATE_INFO;
const MAX_TAGS: usize = 10;
const KEEP_CURRENT_HINT: &str = "Leave empty to keep current";

const TITLE: TextField = TextField {
    key: config_keys::TITLE,
    label: "Stream Title",
    editor: TextEditor::Line {
        placeholder: KEEP_CURRENT_HINT,
    },
    default: "",
    required: false,
};

const CATEGORY_ID: Blankable<WholeNumberField> = Blankable(WholeNumberField {
    key: config_keys::CATEGORY_ID,
    label: "Category ID",
    placeholder: KEEP_CURRENT_HINT,
    bounds: ValueBounds::UNBOUNDED,
});

const TAGS: CommaListField = CommaListField {
    key: config_keys::TAGS,
    label: "Tags (comma-separated, max 10)",
    placeholder: KEEP_CURRENT_HINT,
    default: "",
    max_items: MAX_TAGS,
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TITLE, &CATEGORY_ID, &TAGS]);

struct ChannelUpdate {
    title: Option<String>,
    category_id: Option<u64>,
    tags: Option<Vec<String>>,
}

impl ChannelUpdate {
    fn is_empty(&self) -> bool {
        self.title.is_none() && self.category_id.is_none() && self.tags.is_none()
    }
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<ChannelUpdate, ConfigProblem> {
    let tags = TAGS.read(config, args)?;
    Ok(ChannelUpdate {
        title: non_empty(TITLE.read(config, args)?),
        category_id: CATEGORY_ID.read(config, args)?,
        tags: (!tags.is_empty()).then_some(tags),
    })
}

pub struct UpdateInfoRunner {
    client: Arc<KickChannel>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl UpdateInfoRunner {
    pub fn new(
        client: Arc<KickChannel>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for UpdateInfoRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Update Channel Info"
    }

    fn summary(&self) -> &str {
        "Updates stream title, category, or tags on the Kick channel. Requires channel:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel update title category tags stream info"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        _interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        let updatable = [
            config_keys::TITLE,
            config_keys::CATEGORY_ID,
            config_keys::TAGS,
        ];
        if updatable.iter().all(|key| stores_blank(config, key)) {
            Err(ConfigProblem::whole(ConfigReason::any_of(updatable)))
        } else {
            Ok(())
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let update = match read_config(config, ctx.arg_stack) {
            Ok(update) => update,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if update.is_empty() {
            return (
                timer.failed(UserText::unlocalized(
                    "all fields are empty after interpolation; nothing to update",
                )),
                None,
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(UserText::unlocalized(format!("token error: {e}"))),
            Ok(token) => SubActionOutcome::from_result(
                &self
                    .client
                    .update_info(&token, update.title, update.category_id, update.tags)
                    .await,
            ),
        };
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RegistryError;
    use forge_types::EventId;
    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> UpdateInfoRunner {
        let client = KickChannel::new(crate::endpoints_test_support::api_at(&server.uri()));
        UpdateInfoRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> UpdateInfoRunner {
        let client = KickChannel::new(crate::endpoints_test_support::unreachable_api());
        UpdateInfoRunner::new(Arc::new(client), token_source())
    }

    fn config(title: &str, category: &str, tags: &str) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(title.to_owned())),
            (
                "category_id".to_owned(),
                Variant::String(category.to_owned()),
            ),
            ("tags".to_owned(), Variant::String(tags.to_owned())),
        ])
    }

    #[tokio::test]
    async fn execute_with_only_interpolated_title_reaches_server_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("g".to_owned(), Variant::String("Rust".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Live %g%", "", ""), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn non_numeric_category_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("", "not-a-number", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric category id must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn eleven_tags_at_runtime_fail_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new();
        let eleven = tags(11);

        let (telemetry, _) = runner
            .execute(&config("", "", &eleven), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "the runtime tag-overflow guard must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn all_fields_empty_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("x".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%x%", "%x%", "%x%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an all-empty resolved config must not reach the transport"
        );
    }

    fn tags(count: usize) -> String {
        (1..=count)
            .map(|n| format!("t{n}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn validate_config_asks_for_any_field_as_a_whole_and_keys_the_tag_cap_to_tags() {
        let runner = runner_offline();
        let none_set = forge_registry::ConfigProblem::whole(ConfigReason::any_of([
            "title",
            "category_id",
            "tags",
        ]));
        let too_many_tags = forge_registry::ConfigProblem::field(
            "tags",
            ConfigReason::CountOutOfRange(ValueBounds::at_most(10)),
        );
        for (label, cfg, expected) in [
            ("all empty", config("", "", ""), Some(none_set)),
            ("only title", config("t", "", ""), None),
            ("only category", config("", "1", ""), None),
            ("only tags", config("", "", "a,b"), None),
            ("ten tags", config("", "", &tags(10)), None),
            (
                "eleven tags",
                config("", "", &tags(11)),
                Some(too_many_tags),
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
}
