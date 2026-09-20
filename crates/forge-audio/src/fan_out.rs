use std::sync::Arc;

use async_trait::async_trait;

use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

const ROUTE_REASON_SEPARATOR: &str = "; ";

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

pub struct FanOutSink {
    sinks: Vec<Arc<dyn AudioSink>>,
}

impl FanOutSink {
    pub fn new(sinks: Vec<Arc<dyn AudioSink>>) -> Self {
        Self { sinks }
    }
}

#[async_trait]
impl AudioSink for FanOutSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        let futures: Vec<_> = self
            .sinks
            .iter()
            .map(|sink| {
                let sink = Arc::clone(sink);
                let buf = buffer.clone();
                async move { sink.play(buf).await }
            })
            .collect();
        settle(futures::future::join_all(futures).await)
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let (handle, outcomes) = fan_out_stoppable(buffer, &self.sinks).await;
        settle(outcomes)?;
        Ok(handle)
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let futures: Vec<_> = self
            .sinks
            .iter()
            .map(|sink| {
                let sink = Arc::clone(sink);
                let buf = buffer.clone();
                async move { sink.play_controlled(buf).await }
            })
            .collect();

        let mut started = Vec::new();
        let mut reasons = Vec::new();
        for result in futures::future::join_all(futures).await {
            match result {
                Ok(playback) => started.push(playback),
                Err(e) => reasons.push(e.to_string()),
            }
        }

        if started.is_empty() {
            return Err(route_failure(reasons));
        }
        warn_failed_routes(&reasons);

        let handle = PlaybackHandle::merge(started.iter().map(ControlledPlayback::handle));
        let completion = async move { settle(futures::future::join_all(started).await) };
        Ok(ControlledPlayback::merged(handle, Box::pin(completion)))
    }
}

fn settle(outcomes: Vec<Result<(), AudioError>>) -> Result<(), AudioError> {
    if outcomes.is_empty() {
        return Err(AudioError::NoRoute);
    }

    let mut played = false;
    let mut reasons = Vec::new();
    for outcome in outcomes {
        match outcome {
            Ok(()) => played = true,
            Err(e) => reasons.push(e.to_string()),
        }
    }

    if played {
        warn_failed_routes(&reasons);
        Ok(())
    } else {
        Err(route_failure(reasons))
    }
}

fn warn_failed_routes(reasons: &[String]) {
    for reason in reasons {
        tracing::warn!(error = %reason, "audio route failed; playback continues on the surviving routes");
    }
}

fn route_failure(reasons: Vec<String>) -> AudioError {
    if reasons.is_empty() {
        AudioError::NoRoute
    } else {
        AudioError::AllRoutesFailed(reasons.join(ROUTE_REASON_SEPARATOR))
    }
}
