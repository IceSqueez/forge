use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::watch;

use super::signals::{is_endpoint_override_refusal, is_server_bind_failure, strip_ansi};
use crate::EmulatorError;

const RETAINED_LINES: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// ANSI colour sequences are already stripped from `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    pub stream: OutputStream,
    pub text: String,
}

/// Keeps the newest lines only; `dropped` counts how many older ones were discarded.
#[derive(Clone)]
pub struct CapturedOutput {
    inner: Arc<Mutex<Captured>>,
    changes: Arc<watch::Sender<u64>>,
}

struct Captured {
    capacity: usize,
    lines: VecDeque<OutputLine>,
    dropped: u64,
    server_bind_failed: bool,
    override_refusal: Option<String>,
}

impl CapturedOutput {
    pub(crate) fn new() -> Self {
        Self::with_capacity(RETAINED_LINES)
    }

    fn with_capacity(capacity: usize) -> Self {
        let (changes, _) = watch::channel(0);
        Self {
            inner: Arc::new(Mutex::new(Captured {
                capacity,
                lines: VecDeque::new(),
                dropped: 0,
                server_bind_failed: false,
                override_refusal: None,
            })),
            changes: Arc::new(changes),
        }
    }

    pub fn lines(&self) -> Vec<OutputLine> {
        self.lock().lines.iter().cloned().collect()
    }

    pub fn dropped(&self) -> u64 {
        self.lock().dropped
    }

    pub fn tail(&self, stream: OutputStream, count: usize) -> Vec<String> {
        let captured = self.lock();
        let mut tail: Vec<String> = captured
            .lines
            .iter()
            .rev()
            .filter(|line| line.stream == stream)
            .take(count)
            .map(|line| line.text.clone())
            .collect();
        tail.reverse();
        tail
    }

    /// Re-runs `probe` on every new line until it yields a value or `timeout` elapses.
    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&[OutputLine]) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        let mut changes = self.changes.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let found = {
                let mut captured = self.lock();
                probe(captured.lines.make_contiguous())
            };
            if let Some(found) = found {
                return Ok(found);
            }
            if !matches!(
                tokio::time::timeout_at(deadline, changes.changed()).await,
                Ok(Ok(()))
            ) {
                return Err(EmulatorError::WaitTimeout {
                    what: what.to_owned(),
                });
            }
        }
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub(crate) fn server_bind_failed(&self) -> bool {
        self.lock().server_bind_failed
    }

    pub(crate) fn override_refusal(&self) -> Option<String> {
        self.lock().override_refusal.clone()
    }

    pub(crate) fn push(&self, stream: OutputStream, raw: &str) {
        let text = strip_ansi(raw.trim_end_matches(['\n', '\r']));
        {
            let mut captured = self.lock();
            if stream == OutputStream::Stderr && is_server_bind_failure(&text) {
                captured.server_bind_failed = true;
            }
            if captured.override_refusal.is_none() && is_endpoint_override_refusal(&text) {
                captured.override_refusal = Some(text.clone());
            }
            if captured.lines.len() == captured.capacity {
                captured.lines.pop_front();
                captured.dropped += 1;
            }
            captured.lines.push_back(OutputLine { stream, text });
        }
        self.changes.send_modify(|generation| *generation += 1);
    }

    pub(crate) async fn capture(self, stream: OutputStream, pipe: impl AsyncRead + Unpin) {
        let mut reader = BufReader::new(pipe);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => self.push(stream, &String::from_utf8_lossy(&buffer)),
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Captured> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn texts(output: &CapturedOutput) -> Vec<String> {
        output.lines().into_iter().map(|line| line.text).collect()
    }

    #[test]
    fn output_at_capacity_keeps_every_line() {
        let output = CapturedOutput::with_capacity(3);
        for text in ["a", "b", "c"] {
            output.push(OutputStream::Stdout, text);
        }
        assert_eq!(texts(&output), ["a", "b", "c"]);
        assert_eq!(output.dropped(), 0);
    }

    #[test]
    fn output_one_over_capacity_drops_the_oldest_line() {
        let output = CapturedOutput::with_capacity(3);
        for text in ["a", "b", "c", "d"] {
            output.push(OutputStream::Stdout, text);
        }
        assert_eq!(texts(&output), ["b", "c", "d"]);
        assert_eq!(output.dropped(), 1);
    }

    #[test]
    fn tail_is_per_stream_in_arrival_order() {
        let output = CapturedOutput::new();
        for (stream, text) in [
            (OutputStream::Stderr, "e1"),
            (OutputStream::Stdout, "o1"),
            (OutputStream::Stderr, "e2"),
            (OutputStream::Stderr, "e3"),
        ] {
            output.push(stream, text);
        }
        assert_eq!(output.tail(OutputStream::Stderr, 2), ["e2", "e3"]);
        assert_eq!(output.tail(OutputStream::Stdout, 5), ["o1"]);
    }

    #[test]
    fn bind_failure_counts_only_on_stderr() {
        let line = "forge-desktop: server failed to start, leaving it off: could not bind to 127.0.0.1:1: in use";
        let on_stdout = CapturedOutput::new();
        on_stdout.push(OutputStream::Stdout, line);
        let on_stderr = CapturedOutput::new();
        on_stderr.push(OutputStream::Stderr, line);
        assert!(!on_stdout.server_bind_failed());
        assert!(on_stderr.server_bind_failed());
    }

    #[test]
    fn override_refusal_is_remembered_after_its_line_is_evicted() {
        let output = CapturedOutput::with_capacity(1);
        output.push(
            OutputStream::Stdout,
            "\u{1b}[31mERROR\u{1b}[0m error=endpoint override FORGE_TWITCH_API_BASE_URL refused: malformed\r\n",
        );
        output.push(OutputStream::Stdout, "later");
        assert_eq!(
            output.override_refusal().as_deref(),
            Some("ERROR error=endpoint override FORGE_TWITCH_API_BASE_URL refused: malformed")
        );
    }

    #[tokio::test]
    async fn capture_splits_lines_and_survives_invalid_utf8() {
        let output = CapturedOutput::new();
        let pipe: &[u8] = b"first\r\nbad \xff byte\nunterminated";
        output.clone().capture(OutputStream::Stderr, pipe).await;
        assert_eq!(
            texts(&output),
            ["first", "bad \u{fffd} byte", "unterminated"]
        );
    }

    #[tokio::test]
    async fn wait_for_wakes_on_a_line_pushed_while_it_is_parked() {
        let output = CapturedOutput::new();
        let pusher = output.clone();
        let waiter = tokio::spawn(async move {
            output
                .wait_for("ready line", Duration::from_secs(10), |lines| {
                    lines.iter().position(|line| line.text == "ready")
                })
                .await
        });
        tokio::task::yield_now().await;
        pusher.push(OutputStream::Stdout, "noise");
        pusher.push(OutputStream::Stdout, "ready");
        assert_eq!(waiter.await.unwrap().unwrap(), 1);
    }

    #[tokio::test]
    async fn wait_for_times_out_when_no_line_matches() {
        let output = CapturedOutput::new();
        output.push(OutputStream::Stdout, "noise");
        let outcome = output
            .wait_for("absent line", Duration::from_millis(20), |lines| {
                lines.iter().find(|line| line.text == "absent").map(drop)
            })
            .await;
        assert!(
            matches!(outcome, Err(EmulatorError::WaitTimeout { what }) if what == "absent line")
        );
    }
}
