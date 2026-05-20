//! Concurrent playback across multiple audio sinks.
//!
//! All sinks receive `play` calls in a single `join_all`, so the first and last
//! sink begin playback within a single async scheduling quantum. Target drift
//! between sink starts is <50 ms under normal system load.

use std::sync::Arc;

use crate::error::AudioError;
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

pub async fn fan_out(
    buffer: PcmBuffer,
    sinks: &[Arc<dyn AudioSink>],
) -> Vec<Result<(), AudioError>> {
    let futures: Vec<_> = sinks
        .iter()
        .map(|sink| {
            let sink = Arc::clone(sink);
            let buf = buffer.clone();
            async move { sink.play(buf).await }
        })
        .collect();
    futures::future::join_all(futures).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sink::NullSink;

    #[tokio::test]
    async fn fan_out_two_null_sinks_both_succeed() {
        let sinks: Vec<Arc<dyn AudioSink>> = vec![Arc::new(NullSink), Arc::new(NullSink)];
        let buf = PcmBuffer::new(vec![0i16; 100], 44_100, 1);
        let results = fan_out(buf, &sinks).await;
        assert_eq!(results.len(), 2);
        for r in results {
            assert!(r.is_ok());
        }
    }

    #[tokio::test]
    async fn fan_out_empty_sinks_returns_empty() {
        let buf = PcmBuffer::new(vec![0i16; 100], 44_100, 1);
        let results = fan_out(buf, &[]).await;
        assert!(results.is_empty());
    }
}
