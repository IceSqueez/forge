use std::sync::Arc;

use crate::error::AudioError;
use crate::handle::PlaybackHandle;
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

/// Started via a single `join_all` to keep cross-sink start drift low; `stop` on the
/// returned handle cancels every child clip that reported one.
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
