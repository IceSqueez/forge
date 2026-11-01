use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, I24, SampleFormat, SizedSample, U24};
use tokio::sync::oneshot;

use crate::device::catalog::DeviceId;
use crate::device::enumerate::forget_output_devices;
use crate::device::output_feed::{OutputWatch, PlaybackFeed, wait_for_completion};
use crate::error::AudioError;
use crate::pcm::convert;
use crate::sink::AudioSink;
use crate::sink::playback::{ControlledPlayback, PlaybackControl};
use forge_speech_types::PcmBuffer;

type PlaybackTask = tokio::task::JoinHandle<Result<(), AudioError>>;

struct FinishOnDrop(Arc<PlaybackControl>);

impl Drop for FinishOnDrop {
    fn drop(&mut self) {
        self.0.finish();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputCandidate {
    Requested,
    HostDefault,
}

const OUTPUT_CANDIDATES: [OutputCandidate; 2] =
    [OutputCandidate::Requested, OutputCandidate::HostDefault];

const MILLIS_PER_SECOND: u64 = 1_000;
const OUTPUT_TAIL_MS: u32 = 150;
const STALL_GRACE_MS: u64 = 2_000;
const PLAYBACK_NEVER_STARTED: &str = "playback ended before its stream started";

pub(crate) struct CpalSink {
    device_id: DeviceId,
}

impl CpalSink {
    pub(crate) fn new(device_id: DeviceId) -> Self {
        Self { device_id }
    }

    async fn start_playback(
        &self,
        buffer: PcmBuffer,
        control: Arc<PlaybackControl>,
    ) -> Result<PlaybackTask, AudioError> {
        let (started_tx, started_rx) = oneshot::channel();
        let request = PlaybackRequest {
            device_id: self.device_id.as_str().to_owned(),
            buffer,
            control,
            started: started_tx,
        };
        let task = tokio::task::spawn_blocking(move || run_playback(request));
        if started_rx.await.is_ok() {
            return Ok(task);
        }
        task.await
            .map_err(|e| AudioError::JoinFailed(e.to_string()))??;
        Err(AudioError::Host(PLAYBACK_NEVER_STARTED.to_owned()))
    }
}

#[async_trait]
impl AudioSink for CpalSink {
    async fn start(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let control = PlaybackControl::new();
        let start_guard = StopIfAbandoned(Some(Arc::clone(&control)));
        let task = self.start_playback(buffer, Arc::clone(&control)).await?;
        start_guard.disarm();
        Ok(ControlledPlayback::from_task(control, task))
    }
}

struct StopIfAbandoned(Option<Arc<PlaybackControl>>);

impl StopIfAbandoned {
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for StopIfAbandoned {
    fn drop(&mut self) {
        if let Some(control) = self.0.take() {
            control.stop();
        }
    }
}

struct PlaybackRequest {
    device_id: String,
    buffer: PcmBuffer,
    control: Arc<PlaybackControl>,
    started: oneshot::Sender<()>,
}

struct StartedStream {
    stream: cpal::Stream,
    stall_limit: Duration,
    device_name: String,
}

fn run_playback(request: PlaybackRequest) -> Result<(), AudioError> {
    let PlaybackRequest {
        device_id,
        buffer,
        control,
        started,
    } = request;
    let _finish = FinishOnDrop(Arc::clone(&control));
    control.attach_worker(std::thread::current());
    let host = cpal::default_host();
    let watch = OutputWatch::new(control);

    let mut opened = None;
    let mut last_error = AudioError::NoDefaultDevice;
    for candidate in OUTPUT_CANDIDATES {
        let attempt = locate_device(&host, candidate, &device_id)
            .and_then(|device| try_start_stream(&device, &buffer, &watch));
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
        return Err(last_error);
    };

    if candidate == OutputCandidate::HostDefault {
        tracing::warn!(
            requested = %device_id,
            using = %started_stream.device_name,
            "requested output device did not open; playing on the system default output"
        );
    }
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the requester may stop waiting for the reply"
    )]
    let _ = started.send(());
    wait_for_completion(&watch, started_stream.stall_limit);
    drop(started_stream.stream);

    let hiccups = watch.hiccups();
    if hiccups > 0 {
        tracing::debug!(requested = %device_id, hiccups, "cpal output stream hiccups");
    }

    if let Some(reason) = watch.failure.get() {
        forget_output_devices();
        tracing::warn!(
            requested = %device_id,
            error = %reason,
            "output device failed during playback; the clip was cut short"
        );
        return Err(AudioError::DeviceLost(reason.clone()));
    }

    Ok(())
}

fn locate_device(
    host: &cpal::Host,
    candidate: OutputCandidate,
    requested: &str,
) -> Result<cpal::Device, AudioError> {
    match candidate {
        OutputCandidate::Requested => {
            find_device(host, requested).ok_or_else(|| AudioError::DeviceNotFound {
                id: requested.to_owned(),
            })
        }
        OutputCandidate::HostDefault => host
            .default_output_device()
            .ok_or(AudioError::NoDefaultDevice),
    }
}

fn try_start_stream(
    device: &cpal::Device,
    buffer: &PcmBuffer,
    watch: &Arc<OutputWatch>,
) -> Result<StartedStream, AudioError> {
    let config = device
        .default_output_config()
        .map_err(|e| AudioError::Host(e.to_string()))?;

    let dst_sr = config.sample_rate();
    let dst_ch = config.channels();
    let sample_format = config.sample_format();

    let converted = prepare_samples(buffer, dst_sr, dst_ch)?;

    let duration_ms = if dst_sr > 0 {
        (converted.len() as u64 / u64::from(dst_ch)) * MILLIS_PER_SECOND / u64::from(dst_sr)
    } else {
        0
    };

    let channels = usize::from(dst_ch.max(1));
    let tail_frames = u64::from(dst_sr) * u64::from(OUTPUT_TAIL_MS) / MILLIS_PER_SECOND;
    let tail_samples = usize::try_from(tail_frames)
        .unwrap_or(usize::MAX)
        .saturating_mul(channels);

    let stream_config = cpal::StreamConfig {
        channels: dst_ch,
        sample_rate: dst_sr,
        buffer_size: cpal::BufferSize::Default,
    };

    let device_name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_default();

    let feed = PlaybackFeed {
        samples: converted,
        cursor: 0,
        channels,
        tail_left: tail_samples,
        watch: Arc::clone(watch),
    };
    let format = OutputFormat {
        config: stream_config,
        sample_format,
    };
    let stream = build_output_stream(device, format, feed, watch)?;

    stream.play().map_err(|e| AudioError::Host(e.to_string()))?;

    Ok(StartedStream {
        stream,
        stall_limit: Duration::from_millis(duration_ms + STALL_GRACE_MS),
        device_name,
    })
}

struct OutputFormat {
    config: cpal::StreamConfig,
    sample_format: SampleFormat,
}

fn build_output_stream(
    device: &cpal::Device,
    format: OutputFormat,
    feed: PlaybackFeed,
    watch: &Arc<OutputWatch>,
) -> Result<cpal::Stream, AudioError> {
    let stream_config = format.config;
    let stream = match format.sample_format {
        SampleFormat::I8 => typed_output_stream::<i8>(device, stream_config, feed, watch),
        SampleFormat::I16 => typed_output_stream::<i16>(device, stream_config, feed, watch),
        SampleFormat::I24 => typed_output_stream::<I24>(device, stream_config, feed, watch),
        SampleFormat::I32 => typed_output_stream::<i32>(device, stream_config, feed, watch),
        SampleFormat::I64 => typed_output_stream::<i64>(device, stream_config, feed, watch),
        SampleFormat::U8 => typed_output_stream::<u8>(device, stream_config, feed, watch),
        SampleFormat::U16 => typed_output_stream::<u16>(device, stream_config, feed, watch),
        SampleFormat::U24 => typed_output_stream::<U24>(device, stream_config, feed, watch),
        SampleFormat::U32 => typed_output_stream::<u32>(device, stream_config, feed, watch),
        SampleFormat::U64 => typed_output_stream::<u64>(device, stream_config, feed, watch),
        SampleFormat::F32 => typed_output_stream::<f32>(device, stream_config, feed, watch),
        SampleFormat::F64 => typed_output_stream::<f64>(device, stream_config, feed, watch),
        other => {
            return Err(AudioError::UnsupportedSampleFormat(format!("{other:?}")));
        }
    };

    stream.map_err(|e| AudioError::Host(e.to_string()))
}

fn typed_output_stream<T: SizedSample + FromSample<i16>>(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    mut feed: PlaybackFeed,
    watch: &Arc<OutputWatch>,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_output_stream(
        stream_config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| feed.fill(data),
        stream_error_callback(Arc::clone(watch)),
        None,
    )
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

fn stream_error_callback(watch: Arc<OutputWatch>) -> impl FnMut(cpal::Error) + Send + 'static {
    move |err: cpal::Error| {
        if ends_playback(err.kind()) {
            watch.fail(err.to_string());
        } else {
            watch.note_hiccup();
        }
    }
}

pub(crate) fn ends_playback(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let watch = OutputWatch::new(PlaybackControl::new());
            let mut on_error = stream_error_callback(Arc::clone(&watch));

            on_error(cpal::Error::new(kind));

            assert_eq!(
                watch.failure.get().is_some(),
                ends,
                "wrong verdict for {kind:?}"
            );
            assert_eq!(
                watch.hiccups(),
                u64::from(!ends),
                "wrong hiccup count for {kind:?}"
            );
        }
    }

    #[test]
    fn hiccups_add_up_and_stay_counted_when_a_fatal_error_follows() {
        let watch = OutputWatch::new(PlaybackControl::new());
        let mut on_error = stream_error_callback(Arc::clone(&watch));

        on_error(cpal::Error::new(cpal::ErrorKind::Xrun));
        on_error(cpal::Error::new(cpal::ErrorKind::Xrun));
        on_error(cpal::Error::new(cpal::ErrorKind::DeviceChanged));
        assert!(watch.failure.get().is_none(), "a hiccup ended the clip");

        on_error(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));

        assert_eq!(watch.hiccups(), 3);
    }

    #[test]
    fn a_start_abandoned_before_it_returned_raises_the_stop_flag_and_a_finished_one_does_not() {
        for (finished_starting, stop_raised) in [(false, true), (true, false)] {
            let control = PlaybackControl::new();
            let guard = StopIfAbandoned(Some(Arc::clone(&control)));

            if finished_starting {
                guard.disarm();
            } else {
                drop(guard);
            }

            assert_eq!(
                control.is_stopped(),
                stop_raised,
                "finished starting: {finished_starting}"
            );
        }
    }
}
