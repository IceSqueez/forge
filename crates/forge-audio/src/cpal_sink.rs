use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::oneshot;

use crate::convert;
use crate::device::{DeviceId, forget_output_devices};
use crate::error::AudioError;
use crate::events::{AudioEvent, AudioEventSink};
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

type PlaybackTask = tokio::task::JoinHandle<Result<(), AudioError>>;

type StreamFailure = Arc<OnceLock<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputCandidate {
    Requested,
    HostDefault,
}

const OUTPUT_CANDIDATES: [OutputCandidate; 2] =
    [OutputCandidate::Requested, OutputCandidate::HostDefault];

pub struct CpalSink {
    device_id: DeviceId,
    target_sample_rate: Option<u32>,
    target_channels: Option<u16>,
    event_sink: Arc<dyn AudioEventSink>,
}

impl CpalSink {
    pub fn new(
        device_id: DeviceId,
        target_sample_rate: Option<u32>,
        target_channels: Option<u16>,
        event_sink: Arc<dyn AudioEventSink>,
    ) -> Self {
        Self {
            device_id,
            target_sample_rate,
            target_channels,
            event_sink,
        }
    }

    /// Resolves once a device accepted the stream; `None` means the playback already ran to its end.
    async fn start_playback(
        &self,
        buffer: PcmBuffer,
        stop: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<Option<PlaybackTask>, AudioError> {
        let (started_tx, started_rx) = oneshot::channel();
        let request = PlaybackRequest {
            device_id: self.device_id.0.clone(),
            buffer,
            target_sr: self.target_sample_rate,
            target_ch: self.target_channels,
            event_sink: Arc::clone(&self.event_sink),
            stop,
            paused,
            started: started_tx,
        };
        let task = tokio::task::spawn_blocking(move || run_playback(request));
        if started_rx.await.is_ok() {
            return Ok(Some(task));
        }
        task.await
            .map_err(|e| AudioError::JoinFailed(e.to_string()))??;
        Ok(None)
    }
}

#[async_trait]
impl AudioSink for CpalSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        match self.start_playback(buffer, stop, paused).await? {
            Some(task) => task
                .await
                .map_err(|e| AudioError::JoinFailed(e.to_string()))?,
            None => Ok(()),
        }
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        self.start_playback(buffer, Arc::clone(&stop), Arc::clone(&paused))
            .await?;
        Ok(PlaybackHandle::from_flags(stop, paused))
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let handle = PlaybackHandle::from_flags(Arc::clone(&stop), Arc::clone(&paused));
        match self.start_playback(buffer, stop, paused).await? {
            Some(task) => Ok(ControlledPlayback::from_handle(handle, task)),
            None => Ok(ControlledPlayback::resolved(Ok(()))),
        }
    }
}

struct PlaybackRequest {
    device_id: String,
    buffer: PcmBuffer,
    target_sr: Option<u32>,
    target_ch: Option<u16>,
    event_sink: Arc<dyn AudioEventSink>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    started: oneshot::Sender<()>,
}

struct StartedStream {
    stream: cpal::Stream,
    duration_ms: u64,
    device_name: String,
}

fn run_playback(request: PlaybackRequest) -> Result<(), AudioError> {
    let PlaybackRequest {
        device_id,
        buffer,
        target_sr,
        target_ch,
        event_sink,
        stop,
        paused,
        started,
    } = request;
    let host = cpal::default_host();
    let failure: StreamFailure = Arc::new(OnceLock::new());

    let mut opened = None;
    let mut last_error = AudioError::NoDefaultDevice;
    for candidate in OUTPUT_CANDIDATES {
        let attempt = locate_device(&host, candidate, &device_id).and_then(|device| {
            try_start_stream(
                &device, &buffer, target_sr, target_ch, &stop, &paused, &failure,
            )
        });
        match attempt {
            Ok(stream) => {
                opened = Some((candidate, stream));
                break;
            }
            Err(e) => {
                if candidate == OutputCandidate::Requested {
                    forget_output_devices();
                }
                last_error = e;
            }
        }
    }

    let Some((candidate, started_stream)) = opened else {
        let reason = last_error.to_string();
        tracing::warn!(
            requested = %device_id,
            error = %reason,
            "no output device accepted the stream; nothing was played"
        );
        event_sink.emit(AudioEvent::PlaybackFailed {
            clip_id: None,
            clip_label: None,
            error: reason,
        });
        return Err(last_error);
    };

    if candidate == OutputCandidate::HostDefault {
        tracing::warn!(
            requested = %device_id,
            using = %started_stream.device_name,
            "requested output device did not open; playing on the system default output"
        );
    }
    let _ = started.send(());
    event_sink.emit(AudioEvent::PlaybackStarted {
        clip_id: None,
        clip_label: None,
        device: started_stream.device_name,
        duration_secs: Some(started_stream.duration_ms as f64 / 1000.0),
        looped: false,
    });
    wait_for_completion(started_stream.duration_ms, &stop, &paused, &failure);
    drop(started_stream.stream);

    if let Some(reason) = failure.get() {
        forget_output_devices();
        tracing::warn!(
            requested = %device_id,
            error = %reason,
            "output device failed during playback; the clip was cut short"
        );
        event_sink.emit(AudioEvent::PlaybackFailed {
            clip_id: None,
            clip_label: None,
            error: reason.clone(),
        });
        return Err(AudioError::DeviceLost(reason.clone()));
    }

    event_sink.emit(AudioEvent::PlaybackFinished {
        clip_id: None,
        clip_label: None,
    });
    Ok(())
}

fn locate_device(
    host: &cpal::Host,
    candidate: OutputCandidate,
    requested: &str,
) -> Result<cpal::Device, AudioError> {
    match candidate {
        OutputCandidate::Requested => find_device(host, requested)
            .ok_or_else(|| AudioError::Host(format!("device '{}' not found", requested))),
        OutputCandidate::HostDefault => host
            .default_output_device()
            .ok_or(AudioError::NoDefaultDevice),
    }
}

fn try_start_stream(
    device: &cpal::Device,
    buffer: &PcmBuffer,
    target_sr: Option<u32>,
    target_ch: Option<u16>,
    stop: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    failure: &StreamFailure,
) -> Result<StartedStream, AudioError> {
    let config = device
        .default_output_config()
        .map_err(|e| AudioError::Host(e.to_string()))?;

    let device_sr = config.sample_rate();
    let device_ch = config.channels();
    let sample_format = config.sample_format();

    let dst_sr = target_sr.unwrap_or(device_sr);
    let dst_ch = target_ch.unwrap_or(device_ch);

    let converted = prepare_samples(buffer, dst_sr, dst_ch)?;

    let duration_ms = if dst_sr > 0 {
        (converted.len() as u64 / u64::from(dst_ch)) * 1000 / u64::from(dst_sr)
    } else {
        0
    };

    let (tx, rx) = crossbeam_channel::bounded::<i16>(converted.len().max(1));
    for s in converted {
        let _ = tx.send(s);
    }
    drop(tx);

    let stream_config = cpal::StreamConfig {
        channels: dst_ch,
        sample_rate: dst_sr,
        buffer_size: cpal::BufferSize::Default,
    };

    let device_name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_default();

    let stream = build_output_stream(
        device,
        stream_config,
        sample_format,
        rx,
        Arc::clone(stop),
        Arc::clone(paused),
        failure,
    )?;

    stream.play().map_err(|e| AudioError::Host(e.to_string()))?;

    Ok(StartedStream {
        stream,
        duration_ms,
        device_name,
    })
}

fn build_output_stream(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    sample_format: SampleFormat,
    rx: crossbeam_channel::Receiver<i16>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    failure: &StreamFailure,
) -> Result<cpal::Stream, AudioError> {
    let stream = match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                    data.fill(0.0);
                    return;
                }
                for s in data.iter_mut() {
                    *s = rx.try_recv().map(|v| v as f32 / 32767.0).unwrap_or(0.0);
                }
            },
            stream_error_callback(Arc::clone(failure)),
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            stream_config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                    data.fill(0);
                    return;
                }
                for s in data.iter_mut() {
                    *s = rx.try_recv().unwrap_or(0);
                }
            },
            stream_error_callback(Arc::clone(failure)),
            None,
        ),
        SampleFormat::I32 => device.build_output_stream(
            stream_config,
            move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                    data.fill(0);
                    return;
                }
                for s in data.iter_mut() {
                    *s = rx.try_recv().map(|v| v as i32).unwrap_or(0);
                }
            },
            stream_error_callback(Arc::clone(failure)),
            None,
        ),
        other => {
            return Err(AudioError::Host(format!(
                "unsupported sample format {:?}",
                other
            )));
        }
    };

    stream.map_err(|e| AudioError::Host(e.to_string()))
}

fn wait_for_completion(
    duration_ms: u64,
    stop: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    failure: &StreamFailure,
) {
    let total_ms = duration_ms + 50;
    let mut elapsed_ms = 0u64;
    while elapsed_ms < total_ms || paused.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) || failure.get().is_some() {
            break;
        }
        let step = if elapsed_ms < total_ms {
            (total_ms - elapsed_ms).min(20)
        } else {
            20
        };
        std::thread::sleep(Duration::from_millis(step));
        if !paused.load(Ordering::Relaxed) {
            elapsed_ms += step;
        }
    }
}

fn find_device(host: &cpal::Host, id_str: &str) -> Option<cpal::Device> {
    host.output_devices()
        .ok()?
        .find(|d| d.id().ok().map(|id| id.id() == id_str).unwrap_or(false))
}

fn prepare_samples(buffer: &PcmBuffer, dst_sr: u32, dst_ch: u16) -> Result<Vec<i16>, AudioError> {
    let resampled =
        convert::resample(&buffer.samples, buffer.sample_rate, dst_sr, buffer.channels)?;
    Ok(convert::remix(&resampled, buffer.channels, dst_ch))
}

fn stream_error_callback(failure: StreamFailure) -> impl FnMut(cpal::Error) + Send + 'static {
    move |err: cpal::Error| {
        tracing::error!("cpal stream error: {}", err);
        if ends_playback(err.kind()) {
            let _ = failure.set(err.to_string());
        }
    }
}

fn ends_playback(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const LONG_CLIP_MS: u64 = 10_000;
    const EARLY_EXIT_BUDGET: Duration = Duration::from_secs(2);

    #[test]
    fn a_stream_error_ends_playback_only_when_the_device_can_no_longer_play() {
        for (kind, ends) in [
            (cpal::ErrorKind::Xrun, false),
            (cpal::ErrorKind::DeviceChanged, false),
            (cpal::ErrorKind::RealtimeDenied, false),
            (cpal::ErrorKind::DeviceNotAvailable, true),
            (cpal::ErrorKind::StreamInvalidated, true),
            (cpal::ErrorKind::BackendError, true),
        ] {
            let failure: StreamFailure = Arc::new(OnceLock::new());
            let mut on_error = stream_error_callback(Arc::clone(&failure));

            on_error(cpal::Error::new(kind));

            assert_eq!(failure.get().is_some(), ends, "wrong verdict for {kind:?}");
        }
    }

    #[test]
    fn the_playback_wait_ends_as_soon_as_the_stream_reports_a_failure() {
        let failure: StreamFailure = Arc::new(OnceLock::new());
        failure.set("device unplugged".to_owned()).unwrap();
        let started = std::time::Instant::now();

        wait_for_completion(
            LONG_CLIP_MS,
            &Arc::new(AtomicBool::new(false)),
            &Arc::new(AtomicBool::new(false)),
            &failure,
        );

        assert!(
            started.elapsed() < EARLY_EXIT_BUDGET,
            "a failed stream kept the playback thread waiting for the whole clip"
        );
    }
}
