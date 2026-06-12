#![allow(clippy::unwrap_used)]

use forge_types::OAuthToken;
use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry;

const SENTINEL: &str = "FAKE_TOKEN_SENTINEL_abc123xyz";

struct CaptureLayer {
    lines: Arc<Mutex<Vec<String>>>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = MessageCapture { buf: String::new() };
        event.record(&mut visitor);
        self.lines.lock().unwrap().push(visitor.buf);
    }
}

struct MessageCapture {
    buf: String,
}

impl tracing::field::Visit for MessageCapture {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.buf
            .push_str(&format!("{} = {:?}; ", field.name(), value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.buf
            .push_str(&format!("{} = {}; ", field.name(), value));
    }
}

fn capture_subscriber() -> (
    Arc<Mutex<Vec<String>>>,
    impl tracing::Subscriber + Send + Sync,
) {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let layer = CaptureLayer {
        lines: lines.clone(),
    };
    let sub = registry().with(layer);
    (lines, sub)
}

#[test]
fn oauth_token_debug_does_not_expose_sentinel() {
    let token = OAuthToken::new(SENTINEL);
    let debug_str = format!("{:?}", token);
    assert!(
        !debug_str.contains(SENTINEL),
        "OAuthToken Debug must not emit raw token; got: {debug_str}"
    );
    assert!(
        debug_str.contains("<redacted>"),
        "OAuthToken Debug must contain '<redacted>'; got: {debug_str}"
    );
}

#[test]
fn tracing_token_field_never_logs_sentinel_at_any_level() {
    let (lines, sub) = capture_subscriber();
    let token = OAuthToken::new(SENTINEL);

    with_default(sub, || {
        tracing::debug!(?token, "token field logged via debug");
        tracing::info!(?token, "token field logged via info");
        tracing::warn!(?token, "token field logged via warn");
    });

    let captured = lines.lock().unwrap();
    assert!(
        !captured.is_empty(),
        "capture layer must have observed the log events"
    );
    for line in captured.iter() {
        assert!(
            !line.contains(SENTINEL),
            "tracing field capture must not emit the raw token value.\n\
             Captured line: {line}"
        );
    }
}
