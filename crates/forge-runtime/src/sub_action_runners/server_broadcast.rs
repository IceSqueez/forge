use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

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
        config.require_str("event_name").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "server.broadcast");

        let name_template = config.str("event_name").unwrap_or_default();
        let event_name = ctx.arg_stack.interpolate(name_template);

        let (outcome, updated_stack) = if event_name.is_empty() {
            (
                SubActionOutcome::Failed("event_name is required".to_owned()),
                None,
            )
        } else {
            let payload = config_payload(config);
            // Overlays subscribe on source == Server, kind "broadcast.*"; forge-server forwards it.
            self.publisher.publish(Event::caused_by(
                EventSource::Server,
                format!("broadcast.{event_name}"),
                payload,
                ctx.parent_event_id,
            ));
            // Hard-wired to 0: an accurate count needs direct BusAdapter access this runner lacks.
            let new_stack = ctx
                .arg_stack
                .clone()
                .set("broadcast.delivered_count".to_owned(), Variant::Int(0));
            (SubActionOutcome::Success, Some(new_stack))
        };

        (timer.finish(outcome), updated_stack)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_types::EventId;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingPublisher {
        events: Mutex<Vec<Event>>,
    }

    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    impl RecordingPublisher {
        fn captured(&self) -> Vec<Event> {
            self.events.lock().unwrap().clone()
        }
    }

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn cfg(event_name: &str, payload: Variant) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert(
            "event_name".to_owned(),
            Variant::String(event_name.to_owned()),
        );
        c.insert("payload".to_owned(), payload);
        c
    }

    fn empty_object() -> Variant {
        Variant::Object(BTreeMap::new())
    }

    async fn run(
        config: &SubActionConfig,
        arg_stack: &ArgStack,
        parent: EventId,
    ) -> (Arc<RecordingPublisher>, SubActionOutcome, Option<ArgStack>) {
        let recorder = Arc::new(RecordingPublisher::default());
        let runner = ServerBroadcastRunner::new(recorder.clone());
        let ctx = RunContext::leaf(arg_stack, 0, parent, &NullPublisher);
        let (telemetry, updated_stack) = runner.execute(config, &ctx).await;
        (recorder, telemetry.outcome, updated_stack)
    }

    #[tokio::test]
    async fn broadcast_publishes_prefixed_event_from_server_source() {
        let (recorder, outcome, _) = run(
            &cfg("alert", empty_object()),
            &ArgStack::new(),
            EventId::new(),
        )
        .await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        let events = recorder.captured();
        assert_eq!(events.len(), 1, "exactly one event must be published");
        assert_eq!(events[0].kind, "broadcast.alert");
        assert!(matches!(events[0].source, EventSource::Server));
    }

    #[tokio::test]
    async fn broadcast_links_published_event_to_parent_event_id() {
        let parent = EventId::new();
        let (recorder, _, _) = run(&cfg("alert", empty_object()), &ArgStack::new(), parent).await;

        assert_eq!(recorder.captured()[0].caused_by, Some(parent));
    }

    #[tokio::test]
    async fn broadcast_carries_config_object_payload_into_event() {
        let mut payload = BTreeMap::new();
        payload.insert("amount".to_owned(), Variant::Int(5));
        let (recorder, _, _) = run(
            &cfg("alert", Variant::Object(payload)),
            &ArgStack::new(),
            EventId::new(),
        )
        .await;

        let landed = recorder.captured()[0]
            .payload
            .get("amount")
            .cloned()
            .expect("amount key must be present in the published payload");
        assert_eq!(Variant::from_json(landed).unwrap(), Variant::Int(5));
    }

    #[tokio::test]
    async fn broadcast_interpolates_event_name_from_arg_stack_before_prefixing() {
        let stack = ArgStack::new().set("kind".to_owned(), Variant::String("raid".to_owned()));
        let (recorder, _, _) = run(&cfg("%kind%", empty_object()), &stack, EventId::new()).await;

        assert_eq!(recorder.captured()[0].kind, "broadcast.raid");
    }

    #[tokio::test]
    async fn empty_event_name_fails_without_publishing() {
        let (recorder, outcome, updated_stack) =
            run(&cfg("", empty_object()), &ArgStack::new(), EventId::new()).await;

        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(
            recorder.captured().is_empty(),
            "no event may be published when event_name is empty"
        );
        assert!(
            updated_stack.is_none(),
            "a failed broadcast must not write output vars"
        );
    }

    #[tokio::test]
    async fn broadcast_writes_delivered_count_output_var() {
        let (_, _, updated_stack) = run(
            &cfg("alert", empty_object()),
            &ArgStack::new(),
            EventId::new(),
        )
        .await;

        let stack = updated_stack.expect("a successful broadcast must return an updated stack");
        assert_eq!(
            stack.get("broadcast.delivered_count"),
            Some(&Variant::Int(0))
        );
    }
}
