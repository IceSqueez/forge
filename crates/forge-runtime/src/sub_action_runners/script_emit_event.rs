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
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_types::EventId;
    use std::sync::Mutex;

    /// Captures every published event so tests can assert on the runner's output.
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

    /// The runner publishes through its OWN injected publisher, never `ctx.publisher`;
    /// this null sink in the context proves which channel carried the event.
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
    ) -> (Arc<RecordingPublisher>, SubActionOutcome) {
        let recorder = Arc::new(RecordingPublisher::default());
        let runner = ScriptEmitEventRunner::new(recorder.clone());
        let ctx = RunContext::leaf(arg_stack, 0, parent, &NullPublisher);
        let (telemetry, _) = runner.execute(config, &ctx).await;
        (recorder, telemetry.outcome)
    }

    #[tokio::test]
    async fn emit_publishes_custom_prefixed_event_from_server_source() {
        // Load-bearing round-trip contract: the script.event.custom trigger filters on
        // source == Server and kind.strip_prefix("custom."). If either side drifts the
        // emit -> trigger round-trip silently breaks, so pin BOTH exactly.
        let (recorder, outcome) = run(
            &cfg("foo", empty_object()),
            &ArgStack::new(),
            EventId::new(),
        )
        .await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        let events = recorder.captured();
        assert_eq!(events.len(), 1, "exactly one event must be published");
        // Exact string: not "foo" (missing prefix) and not "custom." (missing name).
        assert_eq!(events[0].kind, "custom.foo");
        assert!(matches!(events[0].source, EventSource::Server));
    }

    #[tokio::test]
    async fn emit_links_published_event_to_parent_event_id() {
        let parent = EventId::new();
        let (recorder, _) = run(&cfg("foo", empty_object()), &ArgStack::new(), parent).await;

        assert_eq!(recorder.captured()[0].caused_by, Some(parent));
    }

    #[tokio::test]
    async fn emit_carries_config_object_payload_into_event() {
        let mut payload = BTreeMap::new();
        payload.insert("greeting".to_owned(), Variant::String("hi".to_owned()));
        let (recorder, _) = run(
            &cfg("foo", Variant::Object(payload)),
            &ArgStack::new(),
            EventId::new(),
        )
        .await;

        // The config payload object lands in event.payload; each value is carried through
        // Variant::to_json, so it decodes back to the original Variant under its key.
        let landed = recorder.captured()[0]
            .payload
            .get("greeting")
            .cloned()
            .expect("greeting key must be present in the published payload");
        assert_eq!(
            Variant::from_json(landed).unwrap(),
            Variant::String("hi".to_owned())
        );
    }

    #[tokio::test]
    async fn emit_interpolates_event_name_from_arg_stack_before_prefixing() {
        // %who% resolves from the stack, THEN the "custom." prefix is applied -> "custom.bar".
        let stack = ArgStack::new().set("who".to_owned(), Variant::String("bar".to_owned()));
        let (recorder, _) = run(&cfg("%who%", empty_object()), &stack, EventId::new()).await;

        assert_eq!(recorder.captured()[0].kind, "custom.bar");
    }

    #[tokio::test]
    async fn empty_event_name_fails_without_publishing() {
        let (recorder, outcome) =
            run(&cfg("", empty_object()), &ArgStack::new(), EventId::new()).await;

        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(
            recorder.captured().is_empty(),
            "no event may be published when event_name is empty"
        );
    }
}
