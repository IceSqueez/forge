use std::collections::VecDeque;
use std::future::pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forge_net::{
    BodyProgress, BodySink, BodySource, CappedReadError, StreamReadError, TruncatedBody,
    read_capped, read_streaming, read_truncated,
};
use tokio_util::sync::CancellationToken;

const SIXTY_FOUR_MIB: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceFault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SinkFault;

enum Step {
    Chunk(Vec<u8>),
    Fail,
    Hang,
}

struct ScriptedSource {
    declared: Option<u64>,
    steps: VecDeque<Step>,
    pulls: Arc<AtomicUsize>,
}

impl ScriptedSource {
    fn new(declared: Option<u64>, steps: Vec<Step>) -> Self {
        Self {
            declared,
            steps: steps.into(),
            pulls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn chunks(declared: Option<u64>, chunks: &[&[u8]]) -> Self {
        Self::new(
            declared,
            chunks
                .iter()
                .map(|chunk| Step::Chunk(chunk.to_vec()))
                .collect(),
        )
    }

    fn pull_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.pulls)
    }
}

impl BodySource for ScriptedSource {
    type Chunk = Vec<u8>;
    type Error = SourceFault;

    fn declared_length(&self) -> Option<u64> {
        self.declared
    }

    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SourceFault> {
        self.pulls.fetch_add(1, Ordering::SeqCst);
        match self.steps.pop_front() {
            None => Ok(None),
            Some(Step::Chunk(bytes)) => Ok(Some(bytes)),
            Some(Step::Fail) => Err(SourceFault),
            Some(Step::Hang) => pending().await,
        }
    }
}

enum SinkMode {
    Accept,
    FailOnCall(usize),
    HangOnCall(usize),
}

struct RecordingSink {
    mode: SinkMode,
    calls: usize,
    received: Vec<u8>,
}

impl RecordingSink {
    fn new(mode: SinkMode) -> Self {
        Self {
            mode,
            calls: 0,
            received: Vec::new(),
        }
    }
}

impl BodySink for RecordingSink {
    type Error = SinkFault;

    async fn accept(&mut self, chunk: &[u8]) -> Result<(), SinkFault> {
        self.calls += 1;
        match self.mode {
            SinkMode::FailOnCall(call) if call == self.calls => return Err(SinkFault),
            SinkMode::HangOnCall(call) if call == self.calls => pending::<()>().await,
            _ => {}
        }
        self.received.extend_from_slice(chunk);
        Ok(())
    }
}

fn progress(received: u64, declared_length: Option<u64>) -> BodyProgress {
    BodyProgress {
        received,
        declared_length,
    }
}

async fn stream(
    source: ScriptedSource,
    cap: u64,
    sink: &mut RecordingSink,
    cancel: &CancellationToken,
) -> (
    Result<u64, StreamReadError<SourceFault, SinkFault>>,
    Vec<BodyProgress>,
) {
    let mut seen = Vec::new();
    let result = read_streaming(source, cap, sink, |p| seen.push(p), cancel).await;
    (result, seen)
}

#[tokio::test]
async fn capped_read_concatenates_chunks_within_the_cap() {
    let source = ScriptedSource::chunks(Some(6), &[b"ab", b"", b"cde", b"f"]);
    assert_eq!(read_capped(source, 10).await, Ok(b"abcdef".to_vec()));
}

#[tokio::test]
async fn capped_read_rejects_an_oversized_declared_length_before_reading() {
    let source = ScriptedSource::chunks(Some(11), &[b"ab"]);
    let pulls = source.pull_counter();
    assert_eq!(
        read_capped(source, 10).await,
        Err(CappedReadError::TooLarge { cap: 10 })
    );
    assert_eq!(pulls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn capped_read_enforces_the_cap_on_cumulative_bytes() {
    for (declared, chunks, expected) in [
        (
            None,
            vec![&b"12345"[..], b"67890"],
            Ok(b"1234567890".to_vec()),
        ),
        (
            Some(10),
            vec![&b"1234567890"[..]],
            Ok(b"1234567890".to_vec()),
        ),
        (Some(10), vec![&b""[..]], Ok(Vec::new())),
        (
            None,
            vec![&b"12345"[..], b"67890", b"X"],
            Err(CappedReadError::TooLarge { cap: 10 }),
        ),
        (
            Some(4),
            vec![&b"12345678901"[..]],
            Err(CappedReadError::TooLarge { cap: 10 }),
        ),
        (
            Some(10),
            vec![&b"1234567890"[..], b"1"],
            Err(CappedReadError::TooLarge { cap: 10 }),
        ),
    ] {
        let source = ScriptedSource::chunks(declared, &chunks);
        assert_eq!(
            read_capped(source, 10).await,
            expected,
            "declared {declared:?}"
        );
    }
}

#[tokio::test]
async fn capped_read_with_zero_cap_accepts_only_an_empty_body() {
    assert_eq!(
        read_capped(ScriptedSource::chunks(Some(0), &[b""]), 0).await,
        Ok(Vec::new())
    );
    assert_eq!(
        read_capped(ScriptedSource::chunks(None, &[b"x"]), 0).await,
        Err(CappedReadError::TooLarge { cap: 0 })
    );
}

#[tokio::test]
async fn capped_read_surfaces_a_source_failure() {
    let source = ScriptedSource::new(None, vec![Step::Chunk(b"ab".to_vec()), Step::Fail]);
    assert_eq!(
        read_capped(source, 10).await,
        Err(CappedReadError::Source(SourceFault))
    );
}

#[tokio::test]
async fn capped_read_does_not_trust_a_huge_declared_length_for_allocation() {
    let source = ScriptedSource::chunks(Some(SIXTY_FOUR_MIB), &[b"abc"]);
    let body = read_capped(source, u64::MAX).await;
    assert!(
        body.as_ref()
            .is_ok_and(|body| (body.capacity() as u64) < SIXTY_FOUR_MIB),
        "capacity {:?}",
        body.map(|body| body.capacity())
    );
}

fn kept(bytes: &[u8], truncated: bool) -> Result<TruncatedBody, SourceFault> {
    Ok(TruncatedBody {
        bytes: bytes.to_vec(),
        truncated,
    })
}

#[tokio::test]
async fn truncated_read_keeps_the_first_cap_bytes_and_flags_only_real_overflow() {
    for (declared, chunks, expected) in [
        (
            None,
            vec![&b"12345"[..], b"6789"],
            kept(b"123456789", false),
        ),
        (
            Some(10),
            vec![&b"1234567890"[..]],
            kept(b"1234567890", false),
        ),
        (None, vec![&b""[..], b"ab", b""], kept(b"ab", false)),
        (
            None,
            vec![&b"12345"[..], b"67890", b"X"],
            kept(b"1234567890", true),
        ),
        (
            None,
            vec![&b"1234567"[..], b"890AB"],
            kept(b"1234567890", true),
        ),
        (
            Some(4),
            vec![&b"12345678901"[..]],
            kept(b"1234567890", true),
        ),
        (Some(SIXTY_FOUR_MIB), vec![&b"abc"[..]], kept(b"abc", false)),
    ] {
        let source = ScriptedSource::chunks(declared, &chunks);
        assert_eq!(
            read_truncated(source, 10).await,
            expected,
            "declared {declared:?} chunks {chunks:?}"
        );
    }
}

#[tokio::test]
async fn truncated_read_with_zero_cap_flags_any_byte_as_overflow() {
    assert_eq!(
        read_truncated(ScriptedSource::chunks(Some(0), &[b""]), 0).await,
        kept(b"", false)
    );
    assert_eq!(
        read_truncated(ScriptedSource::chunks(None, &[b"x"]), 0).await,
        kept(b"", true)
    );
}

#[tokio::test]
async fn truncated_read_stops_pulling_once_the_cap_is_passed() {
    let source = ScriptedSource::new(None, vec![Step::Chunk(b"12345678901".to_vec()), Step::Hang]);
    let pulls = source.pull_counter();

    assert_eq!(read_truncated(source, 10).await, kept(b"1234567890", true));
    assert_eq!(pulls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn truncated_read_surfaces_a_source_failure() {
    let source = ScriptedSource::new(None, vec![Step::Chunk(b"ab".to_vec()), Step::Fail]);
    assert_eq!(read_truncated(source, 10).await, Err(SourceFault));
}

#[tokio::test]
async fn streaming_read_delivers_every_byte_and_returns_the_total() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(Some(6), &[b"ab", b"cde", b"f"]);
    let (result, _) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(result, Ok(6));
    assert_eq!(sink.received, b"abcdef");
}

#[tokio::test]
async fn streaming_read_reports_progress_at_zero_and_after_each_non_empty_chunk() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(Some(6), &[b"", b"ab", b"", b"", b"cde", b"f", b""]);
    let (_, seen) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(
        seen,
        vec![
            progress(0, Some(6)),
            progress(2, Some(6)),
            progress(5, Some(6)),
            progress(6, Some(6)),
        ]
    );
}

#[tokio::test]
async fn streaming_read_never_hands_the_sink_empty_chunks() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(None, &[b"", b"ab", b"", b"c"]);
    let (result, _) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!((result, sink.calls), (Ok(3), 2));
}

#[tokio::test]
async fn streaming_read_accepts_a_body_exactly_at_the_cap() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(Some(10), &[b"12345", b"67890"]);
    let (result, _) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(result, Ok(10));
}

#[tokio::test]
async fn streaming_read_never_hands_the_sink_bytes_past_the_cap() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(None, &[b"12345", b"6789", b"0X", b"tail"]);
    let (result, seen) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(result, Err(StreamReadError::TooLarge { cap: 10 }));
    assert_eq!(sink.received, b"123456789");
    assert_eq!(seen.last(), Some(&progress(9, None)));
}

#[tokio::test]
async fn streaming_read_rejects_an_oversized_declared_length_before_any_work() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(Some(11), &[b"ab"]);
    let pulls = source.pull_counter();
    let (result, seen) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(result, Err(StreamReadError::TooLarge { cap: 10 }));
    assert_eq!(
        (pulls.load(Ordering::SeqCst), sink.calls, seen.len()),
        (0, 0, 0)
    );
}

#[tokio::test]
async fn streaming_read_with_a_pre_cancelled_token_never_pulls() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::chunks(Some(2), &[b"ab"]);
    let pulls = source.pull_counter();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let (result, _) = stream(source, 10, &mut sink, &cancel).await;
    assert_eq!(result, Err(StreamReadError::Cancelled));
    assert_eq!((pulls.load(Ordering::SeqCst), sink.calls), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn streaming_read_cancels_while_waiting_for_a_chunk() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::new(None, vec![Step::Chunk(b"ab".to_vec()), Step::Hang]);
    let pulls = source.pull_counter();
    let cancel = CancellationToken::new();
    let canceller = async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        cancel.cancel();
    };
    let ((result, seen), ()) = tokio::join!(stream(source, 10, &mut sink, &cancel), canceller);
    assert_eq!(result, Err(StreamReadError::Cancelled));
    assert_eq!(pulls.load(Ordering::SeqCst), 2);
    assert_eq!(seen, vec![progress(0, None), progress(2, None)]);
}

#[tokio::test(start_paused = true)]
async fn streaming_read_cancels_while_the_sink_is_busy() {
    let mut sink = RecordingSink::new(SinkMode::HangOnCall(2));
    let source = ScriptedSource::chunks(None, &[b"ab", b"cd", b"ef"]);
    let pulls = source.pull_counter();
    let cancel = CancellationToken::new();
    let canceller = async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        cancel.cancel();
    };
    let ((result, seen), ()) = tokio::join!(stream(source, 10, &mut sink, &cancel), canceller);
    assert_eq!(result, Err(StreamReadError::Cancelled));
    assert_eq!((pulls.load(Ordering::SeqCst), sink.calls), (2, 2));
    assert_eq!(seen, vec![progress(0, None), progress(2, None)]);
}

#[tokio::test]
async fn streaming_read_surfaces_a_source_failure() {
    let mut sink = RecordingSink::new(SinkMode::Accept);
    let source = ScriptedSource::new(None, vec![Step::Chunk(b"ab".to_vec()), Step::Fail]);
    let (result, _) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(result, Err(StreamReadError::Source(SourceFault)));
}

#[tokio::test]
async fn streaming_read_surfaces_a_sink_rejection_and_stops_pulling() {
    let mut sink = RecordingSink::new(SinkMode::FailOnCall(2));
    let source = ScriptedSource::chunks(None, &[b"ab", b"cd", b"ef"]);
    let pulls = source.pull_counter();
    let (result, seen) = stream(source, 10, &mut sink, &CancellationToken::new()).await;
    assert_eq!(result, Err(StreamReadError::Sink(SinkFault)));
    assert_eq!(pulls.load(Ordering::SeqCst), 2);
    assert_eq!(seen, vec![progress(0, None), progress(2, None)]);
}
