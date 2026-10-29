use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::event_payload::{EVENT_NAME_KEY, PAYLOAD};

const BROADCAST_KIND_ID: &str = "server.broadcast";
const BROADCAST_EVENT_PREFIX: &str = "broadcast.";

const EVENT_NAME: TextField = TextField {
    key: EVENT_NAME_KEY,
    label: "Event Name",
    editor: TextEditor::Line {
        placeholder: "alert_donation",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&EVENT_NAME, &PAYLOAD]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, serde_json::Value), ConfigProblem> {
    Ok((
        EVENT_NAME.read(config, args)?,
        serde_json::Value::Object(PAYLOAD.read(config, args)?),
    ))
}

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
        BROADCAST_KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, BROADCAST_KIND_ID);
        let (event_name, payload) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if event_name.is_empty() {
            return (
                timer.finish(SubActionOutcome::Failed(format!(
                    "{EVENT_NAME_KEY} is required"
                ))),
                None,
            );
        }
        self.publisher.publish(Event::caused_by(
            EventSource::Server,
            format!("{BROADCAST_EVENT_PREFIX}{event_name}"),
            payload,
            ctx.parent_event_id,
        ));
        (timer.success(), None)
    }
}

#[cfg(test)]
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
                &cfg("alert", payload.clone()),
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
            let (recorder, outcome, updated_stack) = run(
                &cfg("alert", Variant::String(payload.to_owned())),
                &ArgStack::new(),
                EventId::new(),
            )
            .await;
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if m.starts_with("payload")),
                "{payload}: {outcome:?}"
            );
            assert!(recorder.captured().is_empty(), "{payload}");
            assert!(updated_stack.is_none(), "{payload:?}");
        }
    }

    #[test]
    fn validate_config_rejects_a_non_object_payload_as_invalid_format() {
        let err = ServerBroadcastRunner::new(Arc::new(NullPublisher))
            .validate_config(&cfg("alert", Variant::String("object{0}".to_owned())))
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
    async fn a_stored_config_still_carrying_the_removed_client_filters_validates_and_broadcasts() {
        let mut config = cfg("alert", empty_object());
        config.insert(
            "client_filter".to_owned(),
            Variant::Array(vec![Variant::String("client-1".to_owned())]),
        );
        config.insert(
            "client_type_filter".to_owned(),
            Variant::String(r#"["overlay"]"#.to_owned()),
        );
        let runner = ServerBroadcastRunner::new(Arc::new(NullPublisher));

        let validation = runner.validate_config(&config);
        let (recorder, outcome, _) = run(&config, &ArgStack::new(), EventId::new()).await;

        assert!(validation.is_ok(), "{validation:?}");
        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        assert_eq!(recorder.captured().len(), 1);
    }
}
