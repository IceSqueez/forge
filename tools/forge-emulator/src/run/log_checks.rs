use std::time::Duration;

use tokio::time::Instant;

use super::log_tail::LogTail;
use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, LogEvidence, Verdict};
use crate::scenario::LogLine;

/// The file log has no change notification, so it is re-read on this period until the deadline.
const POLL_PERIOD: Duration = Duration::from_millis(25);

/// Reads once more at the deadline, so a line flushed just before it still counts.
pub(crate) async fn observe_log_line(
    mut tail: LogTail,
    deadline: Instant,
    expected: &LogLine,
) -> (Verdict, Evidence) {
    let mut near_misses = Vec::new();
    let unreadable = loop {
        let read_failure = match tail.poll() {
            Ok(records) => {
                for record in records.into_iter().filter(|r| r.target == expected.target) {
                    if record.has_fields(&expected.fields.0) {
                        let evidence = LogEvidence {
                            matched: Some(record),
                            near_misses,
                            files: tail.files(),
                        };
                        return (Verdict::Passed, Evidence::Log(evidence));
                    }
                    if near_misses.len() < EVIDENCE_LIMIT {
                        near_misses.push(record);
                    }
                }
                None
            }
            Err(e) => Some(e.to_string()),
        };
        let now = Instant::now();
        if now >= deadline {
            break read_failure;
        }
        tokio::time::sleep_until((now + POLL_PERIOD).min(deadline)).await;
    };
    let cause = match unreadable {
        Some(reason) => FailureCause::LogUnreadable { reason },
        None => FailureCause::NoLogLine,
    };
    let evidence = LogEvidence {
        matched: None,
        near_misses,
        files: tail.files(),
    };
    (Verdict::Failed(cause), Evidence::Log(evidence))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::io::Write;
    use std::path::Path;

    use serde_json::json;

    use super::*;

    const FILE: &str = "forge.log.2026-09-13";
    const WITHIN: Duration = Duration::from_millis(500);

    fn append(dir: &Path, text: &str) {
        let mut handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(FILE))
            .unwrap();
        handle.write_all(text.as_bytes()).unwrap();
    }

    fn declined(reason: &str) -> LogLine {
        serde_json::from_value(json!({
            "target": "forge::trigger",
            "fields": { "reason": reason, "kind": "twitch.chat.command" },
            "within_ms": WITHIN.as_millis(),
        }))
        .unwrap()
    }

    fn decision(target: &str, reason: &str) -> String {
        format!(
            "2026-09-13T10:00:00Z DEBUG {target}: command trigger did not fire kind=twitch.chat.command reason=\"{reason}\"\n"
        )
    }

    #[tokio::test(start_paused = true)]
    async fn a_matching_line_written_inside_the_window_passes() {
        let dir = tempfile::tempdir().unwrap();
        let tail = LogTail::from_now(dir.path());
        let writer_dir = dir.path().to_owned();
        tokio::spawn(async move {
            tokio::time::sleep(WITHIN / 2).await;
            append(&writer_dir, &decision("forge::trigger", "phrase_mismatch"));
        });

        let (verdict, _) =
            observe_log_line(tail, Instant::now() + WITHIN, &declined("phrase_mismatch")).await;

        assert_eq!(verdict, Verdict::Passed);
    }

    #[tokio::test(start_paused = true)]
    async fn lines_on_the_target_with_other_values_fail_as_near_misses() {
        let dir = tempfile::tempdir().unwrap();
        let tail = LogTail::from_now(dir.path());
        append(dir.path(), &decision("forge::trigger", "cooldown"));
        append(dir.path(), &decision("forge::command", "phrase_mismatch"));

        let (verdict, evidence) =
            observe_log_line(tail, Instant::now() + WITHIN, &declined("phrase_mismatch")).await;

        assert_eq!(verdict, Verdict::Failed(FailureCause::NoLogLine));
        let Evidence::Log(evidence) = evidence else {
            panic!("log evidence expected");
        };
        let near: Vec<&str> = evidence
            .near_misses
            .iter()
            .map(|record| record.fields["reason"].as_str())
            .collect();
        assert_eq!(near, ["cooldown"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_line_already_logged_before_the_step_does_not_count() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &decision("forge::trigger", "phrase_mismatch"));
        let tail = LogTail::from_now(dir.path());

        let (verdict, _) =
            observe_log_line(tail, Instant::now() + WITHIN, &declined("phrase_mismatch")).await;

        assert_eq!(verdict, Verdict::Failed(FailureCause::NoLogLine));
    }
}
