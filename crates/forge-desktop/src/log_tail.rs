use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use time::OffsetDateTime;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

const CAPACITY: usize = 500;

#[derive(Clone, Debug)]
pub struct LogLine {
    pub at: OffsetDateTime,
    pub level: Level,
    pub target: &'static str,
    pub message: String,
}

#[derive(Clone)]
pub struct LogTail {
    lines: Arc<Mutex<VecDeque<LogLine>>>,
}

impl LogTail {
    pub fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(VecDeque::with_capacity(CAPACITY))),
        }
    }

    pub fn layer(&self) -> LogTailLayer {
        LogTailLayer { tail: self.clone() }
    }

    /// Oldest first; the caller gets an owned copy, never the lock.
    pub fn snapshot(&self) -> Vec<LogLine> {
        let lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        lines.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        lines.clear();
    }

    fn push(&self, line: LogLine) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() == CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line);
    }
}

pub struct LogTailLayer {
    tail: LogTail,
}

impl<S: Subscriber> Layer<S> for LogTailLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor { message: None };
        event.record(&mut visitor);
        let Some(message) = visitor.message else {
            return;
        };
        let metadata = event.metadata();
        self.tail.push(LogLine {
            at: OffsetDateTime::now_utc(),
            level: *metadata.level(),
            target: metadata.target(),
            message,
        });
    }
}

struct MessageVisitor {
    message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    fn line(message: &str) -> LogLine {
        LogLine {
            at: OffsetDateTime::now_utc(),
            level: Level::INFO,
            target: "qa_probe",
            message: message.to_owned(),
        }
    }

    fn messages(tail: &LogTail) -> Vec<String> {
        tail.snapshot().into_iter().map(|l| l.message).collect()
    }

    fn capture(emit: impl FnOnce()) -> Vec<LogLine> {
        let tail = LogTail::new();
        let subscriber = tracing_subscriber::registry().with(tail.layer());
        tracing::subscriber::with_default(subscriber, emit);
        tail.snapshot()
    }

    #[test]
    fn the_buffer_keeps_the_newest_lines_and_drops_the_oldest_past_capacity() {
        for (pushes, first_kept, last_kept) in [
            (CAPACITY - 1, 0, CAPACITY - 2),
            (CAPACITY, 0, CAPACITY - 1),
            (CAPACITY + 1, 1, CAPACITY),
            (CAPACITY + 2, 2, CAPACITY + 1),
        ] {
            let tail = LogTail::new();
            for i in 0..pushes {
                tail.push(line(&format!("line {i}")));
            }

            let expected: Vec<String> = (first_kept..=last_kept)
                .map(|i| format!("line {i}"))
                .collect();
            assert_eq!(messages(&tail), expected, "after {pushes} pushes");
        }
    }

    #[test]
    fn clear_empties_the_buffer_shared_with_every_clone_and_leaves_it_capturing() {
        let tail = LogTail::new();
        let clone = tail.clone();
        for i in 0..3 {
            tail.push(line(&format!("stale {i}")));
        }

        clone.clear();
        assert!(
            messages(&tail).is_empty(),
            "clearing through a clone must empty the one shared buffer",
        );

        tail.push(line("fresh"));
        assert_eq!(messages(&clone), vec!["fresh".to_owned()]);
    }

    #[test]
    fn a_captured_line_records_the_level_target_and_message_in_emission_order() {
        let captured = capture(|| {
            tracing::info!(target: "qa_probe", "first");
            tracing::warn!(target: "qa_probe", "second");
            tracing::error!(target: "qa_other", "third");
        });

        let seen: Vec<(Level, &str, String)> = captured
            .into_iter()
            .map(|l| (l.level, l.target, l.message))
            .collect();
        assert_eq!(
            seen,
            vec![
                (Level::INFO, "qa_probe", "first".to_owned()),
                (Level::WARN, "qa_probe", "second".to_owned()),
                (Level::ERROR, "qa_other", "third".to_owned()),
            ],
        );
    }

    #[test]
    fn events_without_a_message_field_are_skipped() {
        let captured = capture(|| {
            tracing::info!(target: "qa_probe", counter = 7);
            tracing::info!(target: "qa_probe", counter = 8, "carries a message");
        });

        let seen: Vec<String> = captured.into_iter().map(|l| l.message).collect();
        assert_eq!(seen, vec!["carries a message".to_owned()]);
    }
}
