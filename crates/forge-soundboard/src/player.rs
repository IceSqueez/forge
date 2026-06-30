use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioEvent, AudioEventSink, PcmBuffer, PlaybackHandle};
use forge_runtime::{SoundPlayer, SoundPlayerError};
use forge_storage::SoundboardClipsRepo;
use forge_types::{ClipId, OutputDevice};

use crate::error::SoundboardError;
use crate::sink_factory::AudioSinkFactory;

/// Upper bound on the linear master gain. +6 dB (the catalog ceiling) is ≈2.0;
/// 4.0 leaves headroom for an interpolated/over-range request without letting a
/// runaway value blow the sample scaling.
const MAX_MASTER_GAIN: f32 = 4.0;

/// Silence drained by the device after a clip's samples are exhausted; the
/// registry entry outlives the clip by this much so a late `stop` still lands.
const PLAYBACK_TAIL_MS: u64 = 200;

type ActiveRegistry = Arc<Mutex<HashMap<ClipId, Vec<(u64, PlaybackHandle)>>>>;

pub struct SoundboardPlayer {
    sink_factory: Arc<dyn AudioSinkFactory>,
    event_sink: Arc<dyn AudioEventSink>,
    clips_repo: Arc<dyn SoundboardClipsRepo>,
    /// Live stop tokens keyed by clip; the `u64` tags one concrete play so
    /// concurrent plays of the same clip register and clean up independently.
    active: ActiveRegistry,
    next_play_id: AtomicU64,
    /// Linear master gain as `f32` bits; there is no shared mixer (per T2), so it
    /// is folded into each clip's samples at play time rather than ramped.
    master_gain_bits: AtomicU32,
}

impl SoundboardPlayer {
    pub fn new(
        sink_factory: Arc<dyn AudioSinkFactory>,
        event_sink: Arc<dyn AudioEventSink>,
        clips_repo: Arc<dyn SoundboardClipsRepo>,
    ) -> Self {
        Self {
            sink_factory,
            event_sink,
            clips_repo,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_play_id: AtomicU64::new(0),
            master_gain_bits: AtomicU32::new(1.0_f32.to_bits()),
        }
    }

    pub fn set_master_volume(&self, gain: f32) {
        let clamped = gain.clamp(0.0, MAX_MASTER_GAIN);
        self.master_gain_bits
            .store(clamped.to_bits(), Ordering::Relaxed);
    }

    pub fn stop(&self, clip_id: ClipId) {
        let handles = {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            guard.remove(&clip_id).unwrap_or_default()
        };
        for (_id, handle) in &handles {
            handle.stop();
        }
    }

    pub fn stop_all(&self) {
        let drained = {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *guard)
        };
        for handles in drained.values() {
            for (_id, handle) in handles {
                handle.stop();
            }
        }
    }

    pub async fn play(
        &self,
        clip_id: ClipId,
        override_device: Option<OutputDevice>,
    ) -> Result<(), SoundboardError> {
        let clip = self
            .clips_repo
            .get(clip_id)
            .await
            .map_err(|e| SoundboardError::Storage(e.to_string()))?
            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;

        let device = override_device.unwrap_or_else(|| clip.output_device.clone());
        let device_label = device_label(&device);

        let sink = match self.sink_factory.build(&device).await {
            Ok(s) => s,
            Err(e) => {
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    error: e.to_string(),
                });
                return Err(SoundboardError::Audio(e));
            }
        };

        let path = clip.file_path.clone();
        let buffer = match tokio::task::spawn_blocking(move || forge_audio::decode_file(&path))
            .await
            .map_err(|e| SoundboardError::JoinError(e.to_string()))
            .and_then(|r| r.map_err(SoundboardError::Audio))
        {
            Ok(b) => b,
            Err(e) => {
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    error: e.to_string(),
                });
                return Err(e);
            }
        };

        let gain = clip.volume * f32::from_bits(self.master_gain_bits.load(Ordering::Relaxed));
        let scaled: Vec<i16> = buffer
            .samples
            .iter()
            .map(|&s| {
                let v = s as f32 * gain;
                v.clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect();

        let sample_rate = buffer.sample_rate.max(1) as u64;
        let channels = buffer.channels.max(1) as u64;
        let frames = (scaled.len() as u64) / channels;
        let duration_ms = frames.saturating_mul(1000) / sample_rate;
        let buffer = PcmBuffer::new(scaled, buffer.sample_rate, buffer.channels);

        self.event_sink.emit(AudioEvent::PlaybackStarted {
            clip_id: Some(clip_id),
            device: device_label,
        });

        match sink.play_stoppable(buffer).await {
            Ok(handle) => {
                self.register(clip_id, handle, duration_ms);
                self.event_sink.emit(AudioEvent::PlaybackFinished {
                    clip_id: Some(clip_id),
                });
                Ok(())
            }
            Err(e) => {
                let error = e.to_string();
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    error,
                });
                Err(SoundboardError::Audio(e))
            }
        }
    }

    /// Stores the stop token and schedules its removal once the clip's own
    /// duration (plus tail) has elapsed, so the registry self-drains even when no
    /// explicit stop arrives. An earlier `stop`/`stop_all` removes it first; the
    /// scheduled cleanup then finds nothing and is inert.
    fn register(&self, clip_id: ClipId, handle: PlaybackHandle, duration_ms: u64) {
        let play_id = self.next_play_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            guard.entry(clip_id).or_default().push((play_id, handle));
        }

        let active = Arc::clone(&self.active);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(duration_ms + PLAYBACK_TAIL_MS)).await;
            let mut guard = active.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(plays) = guard.get_mut(&clip_id) {
                plays.retain(|(id, _)| *id != play_id);
                if plays.is_empty() {
                    guard.remove(&clip_id);
                }
            }
        });
    }
}

#[async_trait]
impl SoundPlayer for SoundboardPlayer {
    async fn play(
        &self,
        clip_id: ClipId,
        output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError> {
        SoundboardPlayer::play(self, clip_id, output_device_override)
            .await
            .map_err(|e| SoundPlayerError::Play(e.to_string()))
    }

    async fn stop(&self, clip_id: ClipId) -> Result<(), SoundPlayerError> {
        SoundboardPlayer::stop(self, clip_id);
        Ok(())
    }

    async fn stop_all(&self) -> Result<(), SoundPlayerError> {
        SoundboardPlayer::stop_all(self);
        Ok(())
    }

    async fn set_master_volume(&self, gain: f32) -> Result<(), SoundPlayerError> {
        SoundboardPlayer::set_master_volume(self, gain);
        Ok(())
    }
}

fn device_label(device: &OutputDevice) -> String {
    match device {
        OutputDevice::Default => "default".to_string(),
        OutputDevice::ByName { name } => name.clone(),
        OutputDevice::ById { id } => id.clone(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use time::OffsetDateTime;

    use async_trait::async_trait;
    use forge_audio::{AudioError, AudioEvent, AudioEventSink, AudioSink, PcmBuffer};
    use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
    use forge_types::{ClipId, OutputDevice};

    use super::*;
    use crate::sink_factory::AudioSinkFactory;

    struct CountingSink {
        count: Arc<Mutex<usize>>,
        last_buf: Arc<Mutex<Option<PcmBuffer>>>,
    }

    #[async_trait]
    impl AudioSink for CountingSink {
        async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
            *self.count.lock().unwrap() += 1;
            *self.last_buf.lock().unwrap() = Some(buffer);
            Ok(())
        }
    }

    type SharedCount = Arc<Mutex<usize>>;
    type SharedBuf = Arc<Mutex<Option<PcmBuffer>>>;

    struct CountingFactory {
        count: SharedCount,
        last_buf: SharedBuf,
    }

    impl CountingFactory {
        fn new() -> (Self, SharedCount, SharedBuf) {
            let count = Arc::new(Mutex::new(0usize));
            let last_buf = Arc::new(Mutex::new(None));
            (
                Self {
                    count: Arc::clone(&count),
                    last_buf: Arc::clone(&last_buf),
                },
                count,
                last_buf,
            )
        }
    }

    #[async_trait]
    impl AudioSinkFactory for CountingFactory {
        async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            Ok(Arc::new(CountingSink {
                count: Arc::clone(&self.count),
                last_buf: Arc::clone(&self.last_buf),
            }))
        }
    }

    struct RecordingEventSink {
        events: Arc<Mutex<Vec<AudioEvent>>>,
    }

    impl RecordingEventSink {
        fn new() -> (Self, Arc<Mutex<Vec<AudioEvent>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: Arc::clone(&events),
                },
                events,
            )
        }
    }

    impl AudioEventSink for RecordingEventSink {
        fn emit(&self, event: AudioEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    struct MockClipsRepo {
        clip: Option<StoredClip>,
    }

    #[async_trait]
    impl SoundboardClipsRepo for MockClipsRepo {
        async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
            Ok(vec![])
        }

        async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
            Ok(self.clip.as_ref().filter(|c| c.id == id).cloned())
        }

        async fn save(&self, _clip: &StoredClip) -> Result<(), StorageError> {
            Ok(())
        }

        async fn delete(&self, _id: ClipId) -> Result<bool, StorageError> {
            Ok(false)
        }
    }

    fn write_wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let bits_per_sample: u16 = 16;
        let block_align = channels * bits_per_sample / 8;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_len = (samples.len() * 2) as u32;
        let file_len = 36 + data_len;

        let mut buf = Vec::with_capacity(44 + samples.len() * 2);
        buf.write_all(b"RIFF").unwrap();
        buf.write_all(&file_len.to_le_bytes()).unwrap();
        buf.write_all(b"WAVE").unwrap();
        buf.write_all(b"fmt ").unwrap();
        buf.write_all(&16u32.to_le_bytes()).unwrap();
        buf.write_all(&1u16.to_le_bytes()).unwrap();
        buf.write_all(&channels.to_le_bytes()).unwrap();
        buf.write_all(&sample_rate.to_le_bytes()).unwrap();
        buf.write_all(&byte_rate.to_le_bytes()).unwrap();
        buf.write_all(&block_align.to_le_bytes()).unwrap();
        buf.write_all(&bits_per_sample.to_le_bytes()).unwrap();
        buf.write_all(b"data").unwrap();
        buf.write_all(&data_len.to_le_bytes()).unwrap();
        for &s in samples {
            buf.write_all(&s.to_le_bytes()).unwrap();
        }
        buf
    }

    fn make_wav_tempfile(
        sample_rate: u32,
        channels: u16,
        n_frames: usize,
    ) -> tempfile::NamedTempFile {
        let samples: Vec<i16> = (0..n_frames * channels as usize)
            .map(|i| ((i % 256) as i16) * 100)
            .collect();
        let wav_bytes = write_wav(sample_rate, channels, &samples);
        let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(tmp.path(), &wav_bytes).unwrap();
        tmp
    }

    fn make_stored_clip(id: ClipId, path: PathBuf) -> StoredClip {
        StoredClip {
            id,
            name: "test clip".to_string(),
            file_path: path,
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: None,
            created_at: OffsetDateTime::now_utc(),
        }
    }

    /// Writes the exact `samples` as a mono 16-bit PCM wav. 16-bit PCM is
    /// lossless, so `forge_audio::decode_file` hands them back verbatim — letting
    /// the scaling assertions compare against known inputs rather than magnitudes.
    fn wav_with_samples(samples: &[i16]) -> tempfile::NamedTempFile {
        let wav_bytes = write_wav(22_050, 1, samples);
        let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(tmp.path(), &wav_bytes).unwrap();
        tmp
    }

    /// Plays a clip built from `samples` with the given per-clip volume and an
    /// optional master gain, returning the samples the sink actually received.
    /// The capture happens via `CountingSink::play` (the `play_stoppable` default
    /// forwards to it), so it observes the post-scaling buffer.
    async fn capture_played_samples(
        samples: &[i16],
        clip_volume: f32,
        master_gain: Option<f32>,
    ) -> Vec<i16> {
        let clip_id = ClipId::new();
        let tmp = wav_with_samples(samples);
        let mut clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        clip.volume = clip_volume;

        let (factory, _count, last_buf) = CountingFactory::new();
        let (event_sink, _events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: Some(clip) };
        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        if let Some(gain) = master_gain {
            player.set_master_volume(gain);
        }
        player.play(clip_id, None).await.unwrap();

        let buf = last_buf.lock().unwrap();
        buf.as_ref().unwrap().samples.clone()
    }

    /// Asserts `actual[i] ≈ factor × baseline[i]` (within one quantization step,
    /// clamped to i16 range). Comparing against a unity-gain baseline rather than
    /// raw input absorbs `decode_file`'s lossy i16→f32→i16 round-trip while still
    /// pinning the multiplicative scaling factor.
    fn assert_proportional(baseline: &[i16], actual: &[i16], factor: f32) {
        assert_eq!(
            baseline.len(),
            actual.len(),
            "baseline and actual sample counts differ"
        );
        for (&b, &a) in baseline.iter().zip(actual) {
            let expected = (b as f32 * factor).clamp(i16::MIN as f32, i16::MAX as f32);
            assert!(
                (a as f32 - expected).abs() <= 1.0,
                "sample {a} is not ~{factor}× baseline {b} (expected {expected})"
            );
        }
    }

    #[tokio::test]
    async fn play_emits_started_and_finished_and_calls_sink() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 2205);
        let clip = make_stored_clip(clip_id, tmp.path().to_path_buf());

        let (factory, play_count, _last_buf) = CountingFactory::new();
        let (event_sink, events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: Some(clip) };

        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        player.play(clip_id, None).await.unwrap();

        assert_eq!(*play_count.lock().unwrap(), 1, "sink must be called once");

        let recorded = events.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert!(matches!(
            recorded[0],
            AudioEvent::PlaybackStarted {
                clip_id: Some(_),
                ..
            }
        ));
        assert!(matches!(
            recorded[1],
            AudioEvent::PlaybackFinished { clip_id: Some(_) }
        ));
    }

    #[tokio::test]
    async fn play_applies_clip_volume_to_samples() {
        let samples = vec![0, 200, -200, 12_000, -12_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 0.5, None).await;
        assert_proportional(&baseline, &out, 0.5);
    }

    /// Master gain defaults to unity: a play before any `set_master_volume`
    /// produces the SAME buffer as an explicit master of 1.0. Guards against a
    /// non-1.0 default silently attenuating every clip. (Compares two plays so
    /// the decode round-trip cancels — exact equality is valid here.)
    #[tokio::test]
    async fn master_volume_defaults_to_unity() {
        let samples = vec![0, 100, -100, 12_000, -12_000];
        let default_master = capture_played_samples(&samples, 1.0, None).await;
        let explicit_unity = capture_played_samples(&samples, 1.0, Some(1.0)).await;
        assert_eq!(default_master, explicit_unity);
    }

    /// Master gain scales the buffer by that linear factor relative to unity.
    #[tokio::test]
    async fn master_volume_scales_samples_by_linear_gain() {
        let samples = vec![0, 200, -200, 12_000, -12_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 1.0, Some(0.5)).await;
        assert_proportional(&baseline, &out, 0.5);
    }

    /// The application point is `clip.volume * master_gain` (multiplicative, not
    /// additive, not either-factor-alone): clip 0.5 × master 0.5 = 0.25× the
    /// unity baseline. Pins where master gain folds into the per-clip samples.
    #[tokio::test]
    async fn master_gain_multiplies_with_clip_volume() {
        let samples = vec![0, 400, -400, 8_000, -8_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 0.5, Some(0.5)).await;
        assert_proportional(&baseline, &out, 0.25);
    }

    /// Master gain is clamped to the [0, 4] ceiling: a request of 10.0 must apply
    /// 4.0. Samples chosen so ×4 stays in range while ×10 would clamp to i16::MAX,
    /// making the assertion distinguish the clamp ceiling from the raw request.
    #[tokio::test]
    async fn master_volume_clamps_gain_above_ceiling() {
        let samples = vec![100, -100, 4_000, -4_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 1.0, Some(10.0)).await;
        assert_proportional(&baseline, &out, 4.0);
    }

    /// A negative master gain clamps to 0.0 — silence, never sign inversion.
    #[tokio::test]
    async fn master_volume_clamps_negative_gain_to_silence() {
        let samples = vec![12_000, -12_000, 5_000];
        let out = capture_played_samples(&samples, 1.0, Some(-1.0)).await;
        assert!(
            out.iter().all(|&s| s == 0),
            "negative master gain must produce silence, got {out:?}"
        );
    }

    /// Stopping an unregistered clip and stop_all on an empty registry succeed
    /// without effect (documented contract) and without panicking — guards the
    /// `remove(..).unwrap_or_default()` / `PoisonError::into_inner` paths against
    /// an accidental `.unwrap()`.
    #[tokio::test]
    async fn stop_on_idle_player_succeeds_without_effect() {
        let (factory, _count, _buf) = CountingFactory::new();
        let (event_sink, _events) = RecordingEventSink::new();
        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(MockClipsRepo { clip: None }),
        );

        assert!(SoundPlayer::stop(&player, ClipId::new()).await.is_ok());
        assert!(SoundPlayer::stop_all(&player).await.is_ok());
    }

    #[tokio::test]
    async fn play_returns_clip_not_found_when_repo_returns_none() {
        let clip_id = ClipId::new();
        let (factory, _count, _buf) = CountingFactory::new();
        let (event_sink, events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: None };

        let player = SoundboardPlayer::new(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        let err = player.play(clip_id, None).await.unwrap_err();
        assert!(
            matches!(err, SoundboardError::ClipNotFound(_)),
            "expected ClipNotFound, got {:?}",
            err
        );
        assert!(
            events.lock().unwrap().is_empty(),
            "no events must be emitted on ClipNotFound"
        );
    }

    #[tokio::test]
    async fn play_overrides_device_when_specified() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let mut clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        clip.output_device = OutputDevice::Default;

        let received_device: Arc<Mutex<Option<OutputDevice>>> = Arc::new(Mutex::new(None));
        let received_device_clone = Arc::clone(&received_device);

        struct DeviceCapturingFactory {
            captured: Arc<Mutex<Option<OutputDevice>>>,
        }

        #[async_trait]
        impl AudioSinkFactory for DeviceCapturingFactory {
            async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
                *self.captured.lock().unwrap() = Some(device.clone());
                Ok(Arc::new(forge_audio::NullSink))
            }
        }

        let (event_sink, _events) = RecordingEventSink::new();
        let clips_repo = MockClipsRepo { clip: Some(clip) };
        let player = SoundboardPlayer::new(
            Arc::new(DeviceCapturingFactory {
                captured: received_device_clone,
            }),
            Arc::new(event_sink),
            Arc::new(clips_repo),
        );

        let override_dev = OutputDevice::ByName {
            name: "Speakers".to_string(),
        };
        player
            .play(clip_id, Some(override_dev.clone()))
            .await
            .unwrap();

        let captured = received_device.lock().unwrap();
        assert_eq!(*captured, Some(override_dev));
    }
}
