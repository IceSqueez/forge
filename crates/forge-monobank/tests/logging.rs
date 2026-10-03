#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};

use serde_json::json;
use support::{Bank, Harness, JAR, TOKEN, now_unix, top_up};

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
async fn logs_never_carry_the_token_donor_names_or_comments() {
    use tracing_subscriber::layer::SubscriberExt;

    let lines = Arc::new(Mutex::new(Vec::new()));
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::registry().with(Capture(Arc::clone(&lines))),
    );
    let bank = Bank::new(&[JAR]);
    let mut rejected = top_up("BAD", now_unix() - 20);
    rejected["currencyCode"] = json!(643);
    rejected["counterName"] = json!("donor BAD");
    bank.add(JAR, rejected);
    bank.add(JAR, top_up("GOOD", now_unix() - 10));
    let mut h = Harness::start(bank).await;
    h.next().await.unwrap();
    let _rejection = h.next().await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    let lines = lines.lock().unwrap().clone();
    assert!(
        lines.iter().any(|line| line.contains("BAD")),
        "the skipped transaction must be logged: {lines:?}"
    );
    for secret in [
        TOKEN,
        "donor GOOD",
        "comment GOOD",
        "donor BAD",
        "comment BAD",
    ] {
        assert!(
            lines.iter().all(|line| !line.contains(secret)),
            "{secret:?} leaked: {lines:?}"
        );
    }
}
