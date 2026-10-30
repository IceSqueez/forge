use crate::vocabulary::labels;
use forge_types::MessageCode;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_RUN_AD;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdLength {
    Seconds30,
    Seconds60,
    Seconds90,
    Seconds120,
    Seconds150,
    Seconds180,
}

impl AdLength {
    fn seconds(self) -> i64 {
        match self {
            Self::Seconds30 => 30,
            Self::Seconds60 => 60,
            Self::Seconds90 => 90,
            Self::Seconds120 => 120,
            Self::Seconds150 => 150,
            Self::Seconds180 => 180,
        }
    }

    pub(crate) fn label(self) -> MessageCode {
        match self {
            Self::Seconds30 => labels::AD_30_SECONDS,
            Self::Seconds60 => labels::AD_60_SECONDS,
            Self::Seconds90 => labels::AD_90_SECONDS,
            Self::Seconds120 => labels::AD_120_SECONDS,
            Self::Seconds150 => labels::AD_150_SECONDS,
            Self::Seconds180 => labels::AD_180_SECONDS,
        }
    }
}

impl ConfigChoice for AdLength {
    const ALL: &'static [Self] = &[
        Self::Seconds30,
        Self::Seconds60,
        Self::Seconds90,
        Self::Seconds120,
        Self::Seconds150,
        Self::Seconds180,
    ];
    const WIRE_NAMES: &'static [&'static str] = &["30", "60", "90", "120", "150", "180"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Seconds30 => "30",
            Self::Seconds60 => "60",
            Self::Seconds90 => "90",
            Self::Seconds120 => "120",
            Self::Seconds150 => "150",
            Self::Seconds180 => "180",
        }
    }
}

pub(crate) const AD_LENGTH: ChoiceField<AdLength> = ChoiceField {
    key: config_keys::DURATION_SECONDS,
    label: "Duration",
    default: AdLength::Seconds60,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&AD_LENGTH]);

pub struct RunAdRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl RunAdRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn run(&self, length: AdLength) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channels/commercial")
            .body(serde_json::json!({ "broadcaster_id": user_id, "length": length.seconds() }));
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for RunAdRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Run Ad"
    }

    fn summary(&self) -> &str {
        "Starts a commercial break in the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch ad commercial break run start channel"
    }

    fn icon_name(&self) -> &str {
        "player-play"
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
        let length = match AD_LENGTH.read(config, ctx.arg_stack) {
            Ok(length) => length,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.run(length).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, RunAdRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = RunAdRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(duration: Variant) -> SubActionConfig {
        BTreeMap::from([("duration_seconds".to_owned(), duration)])
    }

    #[tokio::test]
    async fn execute_posts_self_broadcaster_and_integer_length_in_body() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner
            .execute(&cfg(Variant::String("90".to_owned())), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(output.is_none());
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/channels/commercial");
        assert!(
            request.query.is_empty(),
            "broadcaster_id must not be a query param"
        );
        assert_eq!(
            request.body.unwrap(),
            serde_json::json!({ "broadcaster_id": SELF_USER_ID, "length": 90 })
        );
    }

    #[tokio::test]
    async fn execute_fails_an_unlisted_length_without_calling_helix() {
        for (label, bad) in [
            ("variant int", cfg(Variant::Int(90))),
            ("unlisted duration", cfg(Variant::String("45".to_owned()))),
        ] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&bad, &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "case: {label}"
            );
            assert_eq!(transport.call_count(), 0, "case: {label}");
        }
    }

    #[tokio::test]
    async fn execute_reads_a_missing_length_as_the_declared_default() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "broadcaster_id": SELF_USER_ID, "length": 60 })
        );
    }

    #[test]
    fn validate_config_accepts_allowed_strings_and_rejects_everything_else() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("30", cfg(Variant::String("30".to_owned())), true),
            ("60", cfg(Variant::String("60".to_owned())), true),
            ("90", cfg(Variant::String("90".to_owned())), true),
            ("120", cfg(Variant::String("120".to_owned())), true),
            ("150", cfg(Variant::String("150".to_owned())), true),
            ("180", cfg(Variant::String("180".to_owned())), true),
            (
                "unlisted string",
                cfg(Variant::String("45".to_owned())),
                false,
            ),
            ("variant int", cfg(Variant::Int(90)), false),
            ("empty string", cfg(Variant::String(String::new())), false),
            (
                "missing key reads the declared default",
                BTreeMap::new(),
                true,
            ),
        ];

        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 401,
            body: "token expired".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&cfg(Variant::String("60".to_owned())), &make_ctx(&stack))
            .await;

        assert_request_refused(&telemetry.outcome, 401);
    }
}
