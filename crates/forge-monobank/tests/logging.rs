mod support;

use forge_test_support::log_capture::LogRecording;
use serde_json::json;
use support::{Bank, Harness, JAR, TOKEN, now_unix, top_up};
use tracing::Level;

#[tokio::test]
async fn logs_never_carry_the_token_donor_names_or_comments() {
    let recording = LogRecording::start(Level::TRACE);
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

    let lines = recording.lines();
    assert!(
        lines.iter().any(|line| line.mentions("BAD")),
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
            lines.iter().all(|line| !line.mentions(secret)),
            "{secret:?} leaked: {lines:?}"
        );
    }
}
