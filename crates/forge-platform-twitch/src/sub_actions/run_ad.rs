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
        // broadcaster_id goes in the JSON body, not as a query param — this is
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
