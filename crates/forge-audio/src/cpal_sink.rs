use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, I24, Sample, SampleFormat, SizedSample, U24};
use tokio::sync::oneshot;

use crate::convert;
use crate::device::{DeviceId, forget_output_devices};
use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::sink::AudioSink;
use forge_speech_types::PcmBuffer;

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
}

impl CpalSink {
    pub fn new(
        device_id: DeviceId,
        target_sample_rate: Option<u32>,
        target_channels: Option<u16>,
    ) -> Self {
        Self {
            device_id,
            target_sample_rate,
            target_channels,
        }
    }

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
    wait_for_completion(started_stream.duration_ms, &stop, &paused, &failure);
    drop(started_stream.stream);

    if let Some(reason) = failure.get() {
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
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver lives in this scope and the channel holds every sample"
        )]
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

    let feed = PlaybackFeed {
        samples: rx,
        stop: Arc::clone(stop),
        paused: Arc::clone(paused),
    };
    let format = OutputFormat {
        config: stream_config,
        sample_format,
    };
    let stream = build_output_stream(device, format, feed, failure)?;

    stream.play().map_err(|e| AudioError::Host(e.to_string()))?;

    Ok(StartedStream {
        stream,
        duration_ms,
        device_name,
    })
}

struct PlaybackFeed {
    samples: crossbeam_channel::Receiver<i16>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

impl PlaybackFeed {
    fn fill<T: Sample + FromSample<i16>>(&self, data: &mut [T]) {
        if self.stop.load(Ordering::Relaxed) || self.paused.load(Ordering::Relaxed) {
            data.fill(T::EQUILIBRIUM);
            return;
        }
        for slot in data.iter_mut() {
            *slot = self
                .samples
                .try_recv()
                .map(T::from_sample)
                .unwrap_or(T::EQUILIBRIUM);
        }
    }
}

struct OutputFormat {
    config: cpal::StreamConfig,
    sample_format: SampleFormat,
}

fn build_output_stream(
    device: &cpal::Device,
    format: OutputFormat,
    feed: PlaybackFeed,
    failure: &StreamFailure,
) -> Result<cpal::Stream, AudioError> {
    let stream_config = format.config;
    let stream = match format.sample_format {
        SampleFormat::I8 => typed_output_stream::<i8>(device, stream_config, feed, failure),
        SampleFormat::I16 => typed_output_stream::<i16>(device, stream_config, feed, failure),
        SampleFormat::I24 => typed_output_stream::<I24>(device, stream_config, feed, failure),
        SampleFormat::I32 => typed_output_stream::<i32>(device, stream_config, feed, failure),
        SampleFormat::I64 => typed_output_stream::<i64>(device, stream_config, feed, failure),
        SampleFormat::U8 => typed_output_stream::<u8>(device, stream_config, feed, failure),
        SampleFormat::U16 => typed_output_stream::<u16>(device, stream_config, feed, failure),
        SampleFormat::U24 => typed_output_stream::<U24>(device, stream_config, feed, failure),
        SampleFormat::U32 => typed_output_stream::<u32>(device, stream_config, feed, failure),
        SampleFormat::U64 => typed_output_stream::<u64>(device, stream_config, feed, failure),
        SampleFormat::F32 => typed_output_stream::<f32>(device, stream_config, feed, failure),
        SampleFormat::F64 => typed_output_stream::<f64>(device, stream_config, feed, failure),
        other => {
            return Err(AudioError::Host(format!(
                "unsupported sample format {:?}",
                other
            )));
        }
    };

    stream.map_err(|e| AudioError::Host(e.to_string()))
}

fn typed_output_stream<T: SizedSample + FromSample<i16>>(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    feed: PlaybackFeed,
    failure: &StreamFailure,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_output_stream(
        stream_config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| feed.fill(data),
        stream_error_callback(Arc::clone(failure)),
        None,
    )
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the first stream failure is the one reported"
            )]
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
mod tests {
    use super::*;

    const LONG_CLIP_MS: u64 = 600_000;
    const EARLY_EXIT_BUDGET: Duration = Duration::from_secs(30);

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

    const SCALE_TOLERANCE: f64 = 0.01;

    type LevelRenderer = fn(&[i16]) -> Vec<f64>;

    fn feed_with(samples: &[i16], stopped: bool, paused: bool) -> PlaybackFeed {
        let (tx, rx) = crossbeam_channel::unbounded();
        for &sample in samples {
            tx.send(sample).unwrap();
        }
        PlaybackFeed {
            samples: rx,
            stop: Arc::new(AtomicBool::new(stopped)),
            paused: Arc::new(AtomicBool::new(paused)),
        }
    }

    fn rendered_levels<T: Sample + FromSample<i16>>(samples: &[i16]) -> Vec<f64>
    where
        f64: FromSample<T>,
    {
        let feed = feed_with(samples, false, false);
        let mut out = vec![T::EQUILIBRIUM; samples.len()];
        feed.fill(&mut out);
        out.into_iter().map(f64::from_sample).collect()
    }

    fn underrun_tail_is<T: Sample + FromSample<i16>>(queued: &[i16], expected_silence: T) -> bool {
        let feed = feed_with(queued, false, false);
        let mut out = vec![T::from_sample(i16::MAX); queued.len() + 4];
        feed.fill(&mut out);
        out[queued.len()..]
            .iter()
            .all(|slot| *slot == expected_silence)
    }

    #[test]
    fn every_output_format_receives_the_pcm_scaled_to_its_full_range() {
        let pcm = [i16::MAX, i16::MIN, 0, 16_384, -16_384];
        let expected = [1.0, -1.0, 0.0, 0.5, -0.5];
        let formats: [(&str, LevelRenderer); 12] = [
            ("i8", rendered_levels::<i8>),
            ("i16", rendered_levels::<i16>),
            ("i24", rendered_levels::<I24>),
            ("i32", rendered_levels::<i32>),
            ("i64", rendered_levels::<i64>),
            ("u8", rendered_levels::<u8>),
            ("u16", rendered_levels::<u16>),
            ("u24", rendered_levels::<U24>),
            ("u32", rendered_levels::<u32>),
            ("u64", rendered_levels::<u64>),
            ("f32", rendered_levels::<f32>),
            ("f64", rendered_levels::<f64>),
        ];
        for (name, render) in formats {
            let levels = render(&pcm);
            for ((level, want), source) in levels.iter().zip(expected).zip(pcm) {
                assert!(
                    (level - want).abs() < SCALE_TOLERANCE,
                    "{name}: pcm {source} played at level {level}, expected about {want}"
                );
            }
        }
    }

    #[test]
    fn an_underrun_pads_the_buffer_with_each_formats_silence() {
        let queued = [i16::MAX, i16::MIN];
        let cases = [
            ("i16", underrun_tail_is::<i16>(&queued, 0)),
            ("i32", underrun_tail_is::<i32>(&queued, 0)),
            ("u8", underrun_tail_is::<u8>(&queued, 128)),
            ("u16", underrun_tail_is::<u16>(&queued, 32_768)),
            ("f32", underrun_tail_is::<f32>(&queued, 0.0)),
            ("empty u16", underrun_tail_is::<u16>(&[], 32_768)),
            ("empty u8", underrun_tail_is::<u8>(&[], 128)),
        ];
        for (name, silent) in cases {
            assert!(
                silent,
                "{name}: underrun did not write the format's silence"
            );
        }
    }

    #[test]
    fn a_paused_or_stopped_feed_writes_silence_without_draining_the_queue() {
        let queued = [i16::MAX, i16::MAX, i16::MAX];
        for (stopped, paused) in [(true, false), (false, true), (true, true)] {
            let feed = feed_with(&queued, stopped, paused);
            let mut out = vec![u16::MAX; 8];

            feed.fill(&mut out);

            assert!(
                out.iter().all(|slot| *slot == 32_768),
                "stopped={stopped} paused={paused}: wrote audio instead of silence"
            );
            assert_eq!(
                feed.samples.len(),
                queued.len(),
                "stopped={stopped} paused={paused}: queued audio was consumed"
            );
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
