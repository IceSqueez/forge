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

use crate::ObsSink;

pub struct RawRequestRunner {
    sink: Arc<dyn ObsSink>,
}

impl RawRequestRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for RawRequestRunner {
    fn id(&self) -> &str {
        "obs.misc.raw_request"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Raw OBS Request"
    }

    fn summary(&self) -> &str {
        "Sends an arbitrary obs-websocket request. For advanced users."
    }

    fn search_text(&self) -> &str {
        "obs raw request advanced websocket passthrough custom"
    }

    fn icon_name(&self) -> &str {
        "code"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("request_type".to_owned(), Variant::String(String::new())),
            ("request_data".to_owned(), Variant::String("{}".to_owned())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "request_type",
                label: "Request type",
                placeholder: "e.g. GetVersion",
            },
            FormField::TextArea {
                key: "request_data",
                label: "Request data (JSON)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("request_type") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            Some(Variant::String(_)) => Err(RegistryError::InvalidConfig(
                "obs.misc.raw_request: 'request_type' must not be empty".to_owned(),
            )),
            _ => Err(RegistryError::InvalidConfig(
                "obs.misc.raw_request: 'request_type' must be a string".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_type = config.str("request_type").unwrap_or_default();
        let request_type = ctx.arg_stack.interpolate(raw_type);

        let raw_data = config.str("request_data").unwrap_or("{}");
        let interpolated_data = ctx.arg_stack.interpolate(raw_data);

        let payload = if interpolated_data.is_empty() {
            Variant::Object(BTreeMap::new())
        } else {
            match serde_json::from_str::<serde_json::Value>(&interpolated_data) {
                Ok(json) => match serde_json::from_value::<Variant>(json) {
                    Ok(v) => v,
                    Err(e) => {
                        return (
                            SubActionTelemetry {
                                args_in: ::std::collections::BTreeMap::new(),
                                produced: ::std::collections::BTreeMap::new(),
                                kind: "obs.misc.raw_request".to_owned(),
                                started_at,
                                duration_ms: start.elapsed().as_millis() as u64,
                                outcome: SubActionOutcome::Failed(format!(
                                    "request_data is not valid Variant JSON: {e}"
                                )),
                                index: ctx.index,
                            },
                            None,
                        );
                    }
                },
                Err(e) => {
                    return (
                        SubActionTelemetry {
                            args_in: ::std::collections::BTreeMap::new(),
                            produced: ::std::collections::BTreeMap::new(),
                            kind: "obs.misc.raw_request".to_owned(),
                            started_at,
                            duration_ms: start.elapsed().as_millis() as u64,
                            outcome: SubActionOutcome::Failed(format!(
                                "request_data is not valid JSON: {e}"
                            )),
                            index: ctx.index,
                        },
                        None,
                    );
                }
            }
        };

        let outcome =
            SubActionOutcome::from_result(&self.sink.raw_request(&request_type, &payload).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.misc.raw_request".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
