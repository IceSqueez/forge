mod gateway_support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use forge_net::{CallClass, GatewayRequest, Secret};
use gateway_support::{Harness, platform};
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::subscriber::Interest;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const BEARER: &str = "BearerTokenQp47";
const BODY_MARKER: &str = "invalid_form_body_marker";
const REFUSED_MESSAGE: &str = "upstream refused the request";

type Fields = BTreeMap<String, String>;

#[derive(Default)]
struct Collector(Fields);

impl Visit for Collector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

struct Capture(Arc<Mutex<Vec<Fields>>>);

impl tracing::Subscriber for Capture {
    fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = Collector::default();
        event.record(&mut collector);
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(collector.0);
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

struct InterestFloor;

impl tracing::Subscriber for InterestFloor {
    fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        false
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, _: &tracing::Event<'_>) {}

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

async fn refused_call(request: GatewayRequest) -> Vec<Fields> {
    static FLOOR: OnceLock<()> = OnceLock::new();
    FLOOR.get_or_init(|| {
        drop(tracing::subscriber::set_global_default(InterestFloor));
    });
    let lines = Arc::new(Mutex::new(Vec::new()));
    let _guard = tracing::subscriber::set_default(Capture(Arc::clone(&lines)));
    tracing::callsite::rebuild_interest_cache();

    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(400).set_body_string(format!("{BODY_MARKER} {BEARER}")))
        .mount(&server)
        .await;
    let harness = Harness::new(&server.uri(), platform());
    assert!(harness.gateway.send(request).await.is_err());

    let captured = lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    captured
        .into_iter()
        .filter(|fields| fields.get("message").map(String::as_str) == Some(REFUSED_MESSAGE))
        .collect()
}

#[tokio::test]
async fn a_refused_platform_call_logs_status_and_body_excerpt_at_debug() {
    let lines = refused_call(GatewayRequest::get("x").bearer(Secret::new(BEARER))).await;

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].get("status").map(String::as_str), Some("400"));
    assert!(lines[0]["body"].contains(BODY_MARKER), "{:?}", lines[0]);
}

#[tokio::test]
async fn the_refusal_log_never_carries_the_request_secret() {
    let lines = refused_call(GatewayRequest::get("x").bearer(Secret::new(BEARER))).await;

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].values().all(|value| !value.contains(BEARER)),
        "{:?}",
        lines[0]
    );
}

#[tokio::test]
async fn an_oauth_class_refusal_is_not_logged() {
    let lines = refused_call(
        GatewayRequest::get("x")
            .class(CallClass::OAuth)
            .bearer(Secret::new(BEARER)),
    )
    .await;

    assert!(lines.is_empty(), "{lines:?}");
}
