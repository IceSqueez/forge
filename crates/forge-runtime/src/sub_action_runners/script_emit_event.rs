use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct ScriptEmitEventRunner {
    publisher: Arc<dyn EventPublisher>,
}

impl ScriptEmitEventRunner {
    pub fn new(publisher: Arc<dyn EventPublisher>) -> Self {
        Self { publisher }
    }
}

#[async_trait]
impl SubActionRunner for ScriptEmitEventRunner {
    fn id(&self) -> &str {
        "script.emit_event"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Scripts
    }

    fn label(&self) -> &str {
        "Emit Custom Event"
    }

    fn summary(&self) -> &str {
        "Publish a named custom event to the bus"
    }

    fn search_text(&self) -> &str {
        "emit publish custom event script rhai bus trigger"
    }

    fn icon_name(&self) -> &str {
        "bolt"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("event_name".to_owned(), Variant::String(String::new()));
        cfg.insert("payload".to_owned(), Variant::Object(BTreeMap::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "event_name",
                label: "Event Name",
                placeholder: "my_event",
            },
            FormField::TextArea {
                key: "payload",
                label: "Payload (JSON object)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("event_name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "script.emit_event: event_name is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let name_template = config
            .get("event_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let event_name = ctx.arg_stack.interpolate(name_template);

        let outcome = if event_name.is_empty() {
            SubActionOutcome::Failed("event_name is required".to_owned())
        } else {
            let payload = config_payload(config);
            // Source must match the event_filter in the script.event.custom trigger descriptor.
            self.publisher.publish(Event::caused_by(
                EventSource::Server,
                format!("custom.{event_name}"),
                payload,
                ctx.parent_event_id,
            ));
            SubActionOutcome::Success
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "script.emit_event".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

fn config_payload(config: &SubActionConfig) -> serde_json::Value {
    match config.get("payload") {
        Some(Variant::Object(map)) => {
            let json_map = map.iter().map(|(k, v)| (k.clone(), v.to_json())).collect();
            serde_json::Value::Object(json_map)
        }
        Some(Variant::String(s)) if !s.is_empty() => serde_json::from_str(s)
            .unwrap_or_else(|_| serde_json::Value::Object(Default::default())),
        _ => serde_json::Value::Object(Default::default()),
    }
}
