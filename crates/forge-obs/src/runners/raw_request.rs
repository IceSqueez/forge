use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
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
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
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

        let raw_type = config
            .get("request_type")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let request_type = ctx.arg_stack.interpolate(raw_type);

        let raw_data = config
            .get("request_data")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("{}");
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

        let outcome = match self.sink.raw_request(&request_type, &payload).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::ObsError;

    struct MockSink;

    #[async_trait]
    impl ObsSink for MockSink {
        async fn set_scene(&self, _: &str) -> Result<(), ObsError> {
            Ok(())
        }
        async fn set_source_visible(&self, _: &str, _: &str, _: bool) -> Result<(), ObsError> {
            Ok(())
        }
        async fn set_input_mute(&self, _: &str, _: bool) -> Result<(), ObsError> {
            Ok(())
        }
        async fn start_record(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn stop_record(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn start_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn stop_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn raw_request(&self, _: &str, _: &Variant) -> Result<Variant, ObsError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
    }

    #[test]
    fn validate_config_accepts_request_type_string() {
        let runner = RawRequestRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([
            (
                "request_type".to_owned(),
                Variant::String("GetVersion".to_owned()),
            ),
            ("request_data".to_owned(), Variant::String("{}".to_owned())),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_request_type() {
        let runner = RawRequestRunner::new(Arc::new(MockSink));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }
}
