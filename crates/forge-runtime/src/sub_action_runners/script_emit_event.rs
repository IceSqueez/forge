use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use super::json_object_input::{
    EMPTY_JSON_OBJECT, plain_json_object, read_json_object, validate_json_object,
};

const PAYLOAD_KEY: &str = "payload";

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
        cfg.insert(
            PAYLOAD_KEY.to_owned(),
            Variant::String(EMPTY_JSON_OBJECT.to_owned()),
        );
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
                key: PAYLOAD_KEY,
                label: "Payload (JSON object)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("event_name")?;
        validate_json_object(config, PAYLOAD_KEY)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "script.emit_event");

        let name_template = config.str("event_name").unwrap_or_default();
        let event_name = ctx.arg_stack.interpolate(name_template);

        let outcome = if event_name.is_empty() {
            SubActionOutcome::Failed("event_name is required".to_owned())
        } else {
            match config_payload(config) {
                Ok(payload) => {
                    self.publisher.publish(Event::caused_by(
                        EventSource::Server,
                        format!("custom.{event_name}"),
                        payload,
                        ctx.parent_event_id,
                    ));
                    SubActionOutcome::Success
                }
                Err(reason) => SubActionOutcome::Failed(reason),
            }
        };

        (timer.finish(outcome), None)
    }
}

fn config_payload(config: &SubActionConfig) -> Result<serde_json::Value, String> {
    read_json_object(config, PAYLOAD_KEY).map(|map| plain_json_object(&map))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_registry::{ConfigProblem, ConfigReason};
    use forge_types::EventId;
    use std::collections::BTreeMap;
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
    ) -> (Arc<RecordingPublisher>, SubActionOutcome) {
        let recorder = Arc::new(RecordingPublisher::default());
        let runner = ScriptEmitEventRunner::new(recorder.clone());
        let ctx = RunContext::leaf(arg_stack, 0, parent, &NullPublisher);
        let (telemetry, _) = runner.execute(config, &ctx).await;
        (recorder, telemetry.outcome)
    }

    #[tokio::test]
    async fn emit_publishes_custom_prefixed_event_from_server_source() {
        let (recorder, outcome) = run(
            &cfg("foo", empty_object()),
            &ArgStack::new(),
            EventId::new(),
        )
        .await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        let events = recorder.captured();
        assert_eq!(events.len(), 1, "exactly one event must be published");
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
    async fn published_payload_is_plain_json_for_every_accepted_config_shape() {
        let legacy = Variant::Object(BTreeMap::from([
            ("greeting".to_owned(), Variant::String("hi".to_owned())),
            (
                "tags".to_owned(),
                Variant::Array(vec![Variant::Int(5), Variant::Bool(true)]),
            ),
        ]));
        for (payload, expected) in [
            (
                legacy,
                serde_json::json!({"greeting": "hi", "tags": [5, true]}),
            ),
            (
                Variant::String(r#"{"greeting": "hi", "nested": {"n": 2}}"#.to_owned()),
                serde_json::json!({"greeting": "hi", "nested": {"n": 2}}),
            ),
            (Variant::String(String::new()), serde_json::json!({})),
        ] {
            let (recorder, ..) = run(
                &cfg("foo", payload.clone()),
                &ArgStack::new(),
                EventId::new(),
            )
            .await;
            assert_eq!(recorder.captured()[0].payload, expected, "{payload:?}");
        }
    }

    #[tokio::test]
    async fn invalid_payload_fails_naming_the_field_without_publishing() {
        for payload in ["object{0}", "[1]", "null", r#"{"a": null}"#] {
            let (recorder, outcome) = run(
                &cfg("foo", Variant::String(payload.to_owned())),
                &ArgStack::new(),
                EventId::new(),
            )
            .await;
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if m.starts_with("payload")),
                "{payload}: {outcome:?}"
            );
            assert!(recorder.captured().is_empty(), "{payload}");
        }
    }

    #[test]
    fn validate_config_rejects_a_non_object_payload_as_invalid_format() {
        let err = ScriptEmitEventRunner::new(Arc::new(NullPublisher))
            .validate_config(&cfg("foo", Variant::String("object{0}".to_owned())))
            .unwrap_err();
        assert!(
            matches!(
                &err,
                RegistryError::InvalidConfig(ConfigProblem {
                    field: Some(field),
                    reason: ConfigReason::InvalidFormat(_),
                }) if field == "payload"
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn emit_interpolates_event_name_from_arg_stack_before_prefixing() {
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
