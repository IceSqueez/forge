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
