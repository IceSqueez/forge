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
fn expose_returns_raw_sentinel() {
    let token = OAuthToken::new(SENTINEL);
    assert_eq!(
        token.expose(),
        SENTINEL,
        "expose() must return the raw value for callers that legitimately need it"
    );
}

#[test]
fn tracing_debug_field_does_not_log_sentinel() {
    let (lines, sub) = capture_subscriber();
    let token = OAuthToken::new(SENTINEL);

    with_default(sub, || {
        tracing::debug!(?token, "token field logged via debug format");
    });

    let captured = lines.lock().unwrap();
    for line in captured.iter() {
        assert!(
            !line.contains(SENTINEL),
            "tracing::debug!(?token, ...) must not emit the raw token value.\n\
             Captured line: {line}"
        );
    }
}

#[test]
fn tracing_info_field_does_not_log_sentinel() {
    let (lines, sub) = capture_subscriber();
    let token = OAuthToken::new(SENTINEL);

    with_default(sub, || {
        tracing::info!(?token, "token field logged via info");
    });

    let captured = lines.lock().unwrap();
    for line in captured.iter() {
        assert!(
            !line.contains(SENTINEL),
            "tracing::info!(?token, ...) must not emit the raw token value.\n\
             Captured line: {line}"
        );
    }
}

#[test]
fn tracing_warn_field_does_not_log_sentinel() {
    let (lines, sub) = capture_subscriber();
    let token = OAuthToken::new(SENTINEL);

    with_default(sub, || {
        tracing::warn!(?token, "token field logged via warn");
    });

    let captured = lines.lock().unwrap();
    for line in captured.iter() {
        assert!(
            !line.contains(SENTINEL),
            "tracing::warn!(?token, ...) must not emit the raw token value.\n\
             Captured line: {line}"
        );
    }
}

#[test]
fn authorization_header_value_contains_raw_token() {
    let token = OAuthToken::new(SENTINEL);
    let header_value = format!("Bearer {}", token.expose());
    assert!(
        header_value.contains(SENTINEL),
        "Authorization header construction must use expose() to include raw token"
    );
}
