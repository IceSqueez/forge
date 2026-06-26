use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct ServerBroadcastRunner {
    publisher: Arc<dyn EventPublisher>,
}

impl ServerBroadcastRunner {
    pub fn new(publisher: Arc<dyn EventPublisher>) -> Self {
        Self { publisher }
    }
}

#[async_trait]
impl SubActionRunner for ServerBroadcastRunner {
    fn id(&self) -> &str {
        "server.broadcast"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Server
    }

    fn label(&self) -> &str {
        "Broadcast to Overlay Clients"
    }

    fn summary(&self) -> &str {
        "Push a JSON message to all connected WebSocket clients"
    }

    fn search_text(&self) -> &str {
        "broadcast overlay websocket push event server client message json"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("event_name".to_owned(), Variant::String(String::new()));
        cfg.insert("payload".to_owned(), Variant::Object(BTreeMap::new()));
        cfg.insert("client_filter".to_owned(), Variant::Array(vec![]));
        cfg.insert("client_type_filter".to_owned(), Variant::Array(vec![]));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "event_name",
                label: "Event Name",
                placeholder: "alert_donation",
            },
            FormField::TextArea {
                key: "payload",
                label: "Payload (JSON object)",
            },
            FormField::Text {
                key: "client_filter",
                label: "Client IDs (JSON array, empty = all)",
                placeholder: "[]",
            },
            FormField::Text {
                key: "client_type_filter",
                label: "Client Types (JSON array: overlay/dashboard/remote)",
                placeholder: "[]",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("event_name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "server.broadcast: event_name is required".to_owned(),
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

        let (outcome, updated_stack) = if event_name.is_empty() {
            (
                SubActionOutcome::Failed("event_name is required".to_owned()),
                None,
            )
        } else {
            let payload = config_payload(config);
            // BusAdapter (forge-server) delivers every bus event to WS clients whose subscription
            // filter matches. Overlays subscribe with { "source": "Server", "type": "broadcast.*" }
            // or a wildcard source filter — no forge-server change required.
            self.publisher.publish(Event::caused_by(
                EventSource::Server,
                format!("broadcast.{event_name}"),
                payload,
                ctx.parent_event_id,
            ));
            // Accurate delivery count requires direct BusAdapter access (not available here).
            let new_stack = ctx
                .arg_stack
                .clone()
                .set("broadcast.delivered_count".to_owned(), Variant::Int(0));
            (SubActionOutcome::Success, Some(new_stack))
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "server.broadcast".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated_stack,
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
