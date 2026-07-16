use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.run_ad";

// Twitch accepts exactly these six durations (seconds); any other value returns 400.
// Stored as strings because FormField::Select round-trips its options as Variant::String.
const ALLOWED_DURATIONS: &[&str] = &["30", "60", "90", "120", "150", "180"];
const DEFAULT_DURATION: &str = "60";
const DEFAULT_DURATION_SECS: i64 = 60;

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

    async fn run(&self, duration: i64) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // broadcaster_id goes in the JSON body, not as a query param - this is
        // how the Twitch Helix /channels/commercial endpoint is specified.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channels/commercial")
            .body(serde_json::json!({ "broadcaster_id": user_id, "length": duration }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "duration_seconds".to_owned(),
            Variant::String(DEFAULT_DURATION.to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Select {
            key: "duration_seconds",
            label: "Duration",
            options: ALLOWED_DURATIONS,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("duration_seconds") {
            Some(Variant::String(s)) if ALLOWED_DURATIONS.contains(&s.as_str()) => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'duration_seconds' must be one of 30, 60, 90, 120, 150, 180"
                )));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let duration = config
            .get("duration_seconds")
            .and_then(|v| match v {
                Variant::String(s) if ALLOWED_DURATIONS.contains(&s.as_str()) => s.parse().ok(),
                _ => None,
            })
            .unwrap_or(DEFAULT_DURATION_SECS);

        let outcome = self.run(duration).await;

        (
            SubActionTelemetry {
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

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
        // Regression: the FormField::Select value arrives as Variant::String("90"),
        // but the Helix body's `length` must be the JSON number 90 (not "90"), and
        // broadcaster_id belongs in the BODY, not the query string.
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
    async fn execute_falls_back_to_length_60_when_config_is_invalid() {
        // Variant::Int violates the String convention; production must ignore it and
        // send the default length 60 rather than parsing or rejecting at execute time.
        for (label, bad) in [
            ("variant int", cfg(Variant::Int(90))),
            ("unlisted duration", cfg(Variant::String("45".to_owned()))),
            ("missing key", BTreeMap::new()),
        ] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&bad, &make_ctx(&stack)).await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Success,
                "case: {label}"
            );
            assert_eq!(
                transport.request(0).body.unwrap(),
                serde_json::json!({ "broadcaster_id": SELF_USER_ID, "length": 60 }),
                "case: {label}"
            );
        }
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
            // Proves the String convention is enforced: an integer 90 is NOT a valid
            // duration even though "90" is - the form stores Select values as strings.
            ("variant int", cfg(Variant::Int(90)), false),
            ("empty string", cfg(Variant::String(String::new())), false),
            ("missing key", BTreeMap::new(), false),
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

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("401") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
