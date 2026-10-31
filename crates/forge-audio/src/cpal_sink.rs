use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, I24, Sample, SampleFormat, SizedSample, U24};
use tokio::sync::oneshot;

use crate::convert;
use crate::device::{DeviceId, forget_output_devices};
use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackControl, PlaybackHandle};
use crate::sink::AudioSink;
use forge_speech_types::PcmBuffer;

type PlaybackTask = tokio::task::JoinHandle<Result<(), AudioError>>;

struct OutputWatch {
    control: Arc<PlaybackControl>,
    failure: OnceLock<String>,
    drained: AtomicBool,
}

impl OutputWatch {
    fn new(control: Arc<PlaybackControl>) -> Arc<Self> {
        Arc::new(Self {
            control,
            failure: OnceLock::new(),
            drained: AtomicBool::new(false),
        })
    }

    fn fail(&self, reason: String) {
        if self.failure.set(reason).is_ok() {
            self.control.wake_worker();
        }
    }

    fn mark_drained(&self) {
        if !self.drained.swap(true, Ordering::AcqRel) {
            self.control.wake_worker();
        }
    }

    fn is_drained(&self) -> bool {
        self.drained.load(Ordering::Acquire)
    }
}

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

pub(crate) struct CpalSink {
    device_id: DeviceId,
    target_sample_rate: Option<u32>,
    target_channels: Option<u16>,
}

impl CpalSink {
    pub(crate) fn new(
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
        control: Arc<PlaybackControl>,
    ) -> Result<Option<PlaybackTask>, AudioError> {
        let (started_tx, started_rx) = oneshot::channel();
        let request = PlaybackRequest {
            device_id: self.device_id.0.clone(),
            buffer,
            target_sr: self.target_sample_rate,
            target_ch: self.target_channels,
            control,
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
        match self.start_playback(buffer, PlaybackControl::new()).await? {
            Some(task) => task
                .await
                .map_err(|e| AudioError::JoinFailed(e.to_string()))?,
            None => Ok(()),
        }
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let control = PlaybackControl::new();
        self.start_playback(buffer, Arc::clone(&control)).await?;
        Ok(PlaybackHandle::from_control(control))
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let control = PlaybackControl::new();
        let handle = PlaybackHandle::from_control(Arc::clone(&control));
        let start_guard = StopIfAbandoned(Some(Arc::clone(&control)));
        let started = self.start_playback(buffer, control).await?;
        start_guard.disarm();
        match started {
            Some(task) => Ok(ControlledPlayback::from_handle(handle, task)),
            None => Ok(ControlledPlayback::resolved(Ok(()))),
        }
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
    target_sr: Option<u32>,
    target_ch: Option<u16>,
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
        target_sr,
        target_ch,
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
            .and_then(|device| try_start_stream(&device, &buffer, target_sr, target_ch, &watch));
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
    target_sr: Option<u32>,
    target_ch: Option<u16>,
    watch: &Arc<OutputWatch>,
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

    let channels = usize::from(dst_ch.max(1));
    let tail_frames = u64::from(dst_sr) * u64::from(OUTPUT_TAIL_MS) / MS_PER_SECOND;
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

struct PlaybackFeed {
    samples: Vec<i16>,
    cursor: usize,
    channels: usize,
    tail_left: usize,
    watch: Arc<OutputWatch>,
}

impl PlaybackFeed {
    fn fill<T: Sample + FromSample<i16>>(&mut self, data: &mut [T]) {
        if self.watch.control.is_stopped() || self.watch.control.is_paused() {
            data.fill(T::EQUILIBRIUM);
            return;
        }
        let queued = self.samples.len() - self.cursor;
        let available = data.len().min(queued);
        let whole_frames = available - available % self.channels;
        let block = &self.samples[self.cursor..self.cursor + whole_frames];
        for (slot, &sample) in data.iter_mut().zip(block) {
            *slot = T::from_sample(sample);
        }
        data[whole_frames..].fill(T::EQUILIBRIUM);
        self.cursor += whole_frames;
        if self.cursor == self.samples.len() {
            self.tail_left = self.tail_left.saturating_sub(data.len() - whole_frames);
            if self.tail_left == 0 {
                self.watch.mark_drained();
            }
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

const OUTPUT_TAIL_MS: u32 = 150;
const MS_PER_SECOND: u64 = 1_000;
const STALL_GRACE_MS: u64 = 2_000;

fn wait_for_completion(watch: &OutputWatch, stall_limit: Duration) {
    let control = &watch.control;
    let mut active = Duration::ZERO;
    loop {
        if control.is_stopped() || watch.failure.get().is_some() || watch.is_drained() {
            return;
        }
        if control.is_paused() {
            std::thread::park();
            continue;
        }
        let left = stall_limit.saturating_sub(active);
        if left.is_zero() {
            return;
        }
        let parked_at = Instant::now();
        std::thread::park_timeout(left);
        if !control.is_paused() {
            active += parked_at.elapsed();
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

fn stream_error_callback(watch: Arc<OutputWatch>) -> impl FnMut(cpal::Error) + Send + 'static {
    move |err: cpal::Error| {
        if ends_playback(err.kind()) {
            tracing::error!(error = %err, "cpal output stream failed");
            watch.fail(err.to_string());
        } else {
            tracing::debug!(error = %err, "cpal output stream hiccup");
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

    const LONG_STALL: Duration = Duration::from_secs(600);

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
        }
    }

    const SCALE_TOLERANCE: f64 = 0.01;
    const TAIL_SAMPLES: usize = 8;

    type LevelRenderer = fn(&[i16]) -> Vec<f64>;

    fn feed_with(samples: &[i16], stopped: bool, paused: bool) -> PlaybackFeed {
        let control = PlaybackControl::new();
        if stopped {
            control.stop();
        }
        if paused {
            control.pause();
        }
        PlaybackFeed {
            samples: samples.to_vec(),
            cursor: 0,
            channels: 1,
            tail_left: TAIL_SAMPLES,
            watch: OutputWatch::new(control),
        }
    }

    fn rendered_levels<T: Sample + FromSample<i16>>(samples: &[i16]) -> Vec<f64>
    where
        f64: FromSample<T>,
    {
        let mut feed = feed_with(samples, false, false);
        let mut out = vec![T::EQUILIBRIUM; samples.len()];
        feed.fill(&mut out);
        out.into_iter().map(f64::from_sample).collect()
    }

    fn underrun_tail_is<T: Sample + FromSample<i16>>(queued: &[i16], expected_silence: T) -> bool {
        let mut feed = feed_with(queued, false, false);
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
            let mut feed = feed_with(&queued, stopped, paused);
            let mut out = vec![u16::MAX; 8];

            feed.fill(&mut out);

            assert!(
                out.iter().all(|slot| *slot == 32_768),
                "stopped={stopped} paused={paused}: wrote audio instead of silence"
            );
            assert_eq!(
                feed.cursor, 0,
                "stopped={stopped} paused={paused}: queued audio was consumed"
            );
        }
    }

    fn idle_watch() -> Arc<OutputWatch> {
        OutputWatch::new(PlaybackControl::new())
    }

    #[test]
    fn the_playback_wait_ends_as_soon_as_the_stream_reports_a_failure() {
        let watch = idle_watch();
        watch.fail("device unplugged".to_owned());

        wait_for_completion(&watch, LONG_STALL);
    }

    #[test]
    fn the_playback_wait_ends_once_the_feed_has_rendered_its_silent_tail() {
        let watch = idle_watch();
        let mut feed = PlaybackFeed {
            samples: vec![1; 4],
            cursor: 0,
            channels: 1,
            tail_left: TAIL_SAMPLES,
            watch: Arc::clone(&watch),
        };
        let mut block = [0_i16; 4];

        feed.fill(&mut block);
        assert!(!watch.is_drained(), "the clip body is not the tail");
        feed.fill(&mut block);
        assert!(!watch.is_drained(), "half the tail is not the whole tail");
        feed.fill(&mut block);

        assert!(watch.is_drained());
        wait_for_completion(&watch, LONG_STALL);
    }

    #[test]
    fn the_playback_wait_gives_up_on_a_stream_that_never_asks_for_audio() {
        let watch = idle_watch();

        wait_for_completion(&watch, Duration::ZERO);

        assert!(!watch.is_drained());
    }

    #[test]
    fn the_playback_wait_ends_when_stop_is_requested_with_audio_still_queued() {
        let watch = idle_watch();
        watch.control.stop();

        wait_for_completion(&watch, LONG_STALL);

        assert!(!watch.is_drained());
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
