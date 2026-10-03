#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};

use serde_json::json;
use support::{Harness, Order, donate};

struct Capture(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct Fields(String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(&format!("{}={value:?} ", field.name()));
            }
        }
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

#[tokio::test]
async fn logs_never_carry_donor_names_or_messages() {
    use tracing_subscriber::layer::SubscriberExt;

    let lines = Arc::new(Mutex::new(Vec::new()));
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::registry().with(Capture(Arc::clone(&lines))),
    );
    let mut rejected = donate("BAD", "2026-07-01 10:00:00");
    rejected["currency"] = json!("UAHX");
    let mut h = Harness::start(
        Order::NewestFirst,
        vec![donate("GOOD", "2026-07-01 10:01:00"), rejected],
    )
    .await;
    h.next().await.unwrap();
    let _rejection = h.next().await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    let lines = lines.lock().unwrap().clone();
    assert!(
        lines.iter().any(|line| line.contains("BAD")),
        "the skipped donation must be logged: {lines:?}"
    );
    for pii in ["donor GOOD", "message GOOD", "donor BAD", "message BAD"] {
        assert!(
            lines.iter().all(|line| !line.contains(pii)),
            "{pii:?} leaked: {lines:?}"
        );
    }
}
