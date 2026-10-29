#![allow(clippy::unwrap_used)]
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use forge_events::{Event, EventSource, RequestFailed, TypedEvent};
use serde_json::json;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Metadata, Subscriber};

const SECRET_VALUE: &str = "oauth:leaky-secret-token";

#[derive(Default)]
struct FieldText(String);

impl Visit for FieldText {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.push_str(field.name());
        self.0.push_str(&format!("={value:?};"));
    }
}

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<String>>);

impl Subscriber for CapturedLogs {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = FieldText::default();
        event.record(&mut fields);
        self.0.lock().unwrap().push_str(&fields.0);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

#[test]
fn a_malformed_payload_warning_names_the_kind_but_never_echoes_payload_values() {
    let logs = CapturedLogs::default();
    tracing::subscriber::set_global_default(logs.clone()).unwrap();
    let event = Event::new(
        EventSource::Twitch,
        RequestFailed::KIND,
        json!({
            "endpoint": "helix.users",
            "status_code": SECRET_VALUE,
            "body_snippet": "",
            "retry_after_secs": null,
        }),
    );

    let decoded = RequestFailed::from_event(&event);

    let captured = logs.0.lock().unwrap().clone();
    assert_eq!(
        (
            decoded,
            captured.contains(RequestFailed::KIND),
            captured.contains(SECRET_VALUE)
        ),
        (None, true, false),
        "{captured}"
    );
}
