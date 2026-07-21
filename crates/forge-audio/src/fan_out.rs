//! Concurrent playback across multiple audio sinks.
//!
//! All sinks receive `play` calls in a single `join_all`, so the first and last
//! sink begin playback within a single async scheduling quantum. Target drift
//! between sink starts is <50 ms under normal system load.

use std::sync::Arc;

use crate::error::AudioError;
use crate::handle::PlaybackHandle;
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

/// Fans playback across sinks and returns one handle whose `stop` cancels every
/// child clip that reported a handle. Per-sink `Err` outcomes are preserved
/// positionally; sinks on the no-op default contribute nothing to the handle.
pub async fn fan_out_stoppable(
    buffer: PcmBuffer,
    sinks: &[Arc<dyn AudioSink>],
) -> (PlaybackHandle, Vec<Result<(), AudioError>>) {
    let futures: Vec<_> = sinks
        .iter()
        .map(|sink| {
            let sink = Arc::clone(sink);
            let buf = buffer.clone();
            async move { sink.play_stoppable(buf).await }
        })
        .collect();

    let mut handles = Vec::new();
    let mut outcomes = Vec::with_capacity(sinks.len());
    for result in futures::future::join_all(futures).await {
        match result {
            Ok(handle) => {
                handles.push(handle);
                outcomes.push(Ok(()));
            }
            Err(e) => outcomes.push(Err(e)),
        }
    }
    (PlaybackHandle::merge(handles), outcomes)
}
