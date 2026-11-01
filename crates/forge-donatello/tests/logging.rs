mod support;

use forge_test_support::log_capture::LogRecording;
use serde_json::json;
use support::{Harness, Order, donate};
use tracing::Level;

#[tokio::test]
async fn logs_never_carry_donor_names_or_messages() {
    let recording = LogRecording::start(Level::TRACE);
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

    let lines = recording.lines();
    assert!(
        lines.iter().any(|line| line.mentions("BAD")),
        "the skipped donation must be logged: {lines:?}"
    );
    for pii in ["donor GOOD", "message GOOD", "donor BAD", "message BAD"] {
        assert!(
            lines.iter().all(|line| !line.mentions(pii)),
            "{pii:?} leaked: {lines:?}"
        );
    }
}
