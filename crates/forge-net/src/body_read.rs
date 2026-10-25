use std::future::Future;

use thiserror::Error;
use tokio_util::sync::CancellationToken;

const MAX_PREALLOCATION: u64 = 1024 * 1024;

pub trait BodySource: Send {
    type Chunk: AsRef<[u8]> + Send;
    type Error: Send;

    fn declared_length(&self) -> Option<u64>;

    fn next_chunk(
        &mut self,
    ) -> impl Future<Output = Result<Option<Self::Chunk>, Self::Error>> + Send;
}

pub trait BodySink: Send {
    type Error: Send;

    fn accept(&mut self, chunk: &[u8]) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyProgress {
    pub received: u64,
    pub declared_length: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CappedReadError<E> {
    #[error("response body exceeds the {cap}-byte cap")]
    TooLarge { cap: u64 },
    #[error("response body could not be read")]
    Source(E),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StreamReadError<E, S> {
    #[error("response body exceeds the {cap}-byte cap")]
    TooLarge { cap: u64 },
    #[error("response body could not be read")]
    Source(E),
    #[error("response body consumer rejected a chunk")]
    Sink(S),
    #[error("response body read was cancelled")]
    Cancelled,
}

pub async fn read_capped<B: BodySource>(
    mut source: B,
    cap: u64,
) -> Result<Vec<u8>, CappedReadError<B::Error>> {
    let declared_length = source.declared_length();
    if declared_length.is_some_and(|length| length > cap) {
        return Err(CappedReadError::TooLarge { cap });
    }
    let mut body = Vec::with_capacity(preallocation(declared_length, cap));
    let mut received: u64 = 0;
    while let Some(chunk) = source.next_chunk().await.map_err(CappedReadError::Source)? {
        let chunk = chunk.as_ref();
        received = advance(received, chunk, cap).ok_or(CappedReadError::TooLarge { cap })?;
        body.extend_from_slice(chunk);
    }
    Ok(body)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedBody {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

pub async fn read_truncated<B: BodySource>(
    mut source: B,
    cap: u64,
) -> Result<TruncatedBody, B::Error> {
    let mut bytes = Vec::with_capacity(preallocation(source.declared_length(), cap));
    let mut received: u64 = 0;
    while let Some(chunk) = source.next_chunk().await? {
        let chunk = chunk.as_ref();
        if chunk.is_empty() {
            continue;
        }
        match advance(received, chunk, cap) {
            Some(total) => {
                received = total;
                bytes.extend_from_slice(chunk);
            }
            None => {
                let room = usize::try_from(cap.saturating_sub(received)).unwrap_or(chunk.len());
                bytes.extend_from_slice(chunk.get(..room).unwrap_or(chunk));
                return Ok(TruncatedBody {
                    bytes,
                    truncated: true,
                });
            }
        }
    }
    Ok(TruncatedBody {
        bytes,
        truncated: false,
    })
}

pub async fn read_streaming<B, S, P>(
    mut source: B,
    cap: u64,
    sink: &mut S,
    mut on_progress: P,
    cancel: &CancellationToken,
) -> Result<u64, StreamReadError<B::Error, S::Error>>
where
    B: BodySource,
    S: BodySink,
    P: FnMut(BodyProgress) + Send,
{
    let declared_length = source.declared_length();
    if declared_length.is_some_and(|length| length > cap) {
        return Err(StreamReadError::TooLarge { cap });
    }
    let mut received: u64 = 0;
    on_progress(BodyProgress {
        received,
        declared_length,
    });
    loop {
        let next = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(StreamReadError::Cancelled),
            next = source.next_chunk() => next.map_err(StreamReadError::Source)?,
        };
        let Some(chunk) = next else {
            return Ok(received);
        };
        let chunk = chunk.as_ref();
        if chunk.is_empty() {
            continue;
        }
        received = advance(received, chunk, cap).ok_or(StreamReadError::TooLarge { cap })?;
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(StreamReadError::Cancelled),
            accepted = sink.accept(chunk) => accepted.map_err(StreamReadError::Sink)?,
        }
        on_progress(BodyProgress {
            received,
            declared_length,
        });
    }
}

fn advance(received: u64, chunk: &[u8], cap: u64) -> Option<u64> {
    let chunk_len = u64::try_from(chunk.len()).ok()?;
    received
        .checked_add(chunk_len)
        .filter(|&total| total <= cap)
}

fn preallocation(declared_length: Option<u64>, cap: u64) -> usize {
    let bytes = declared_length.unwrap_or(0).min(cap).min(MAX_PREALLOCATION);
    usize::try_from(bytes).unwrap_or(0)
}
