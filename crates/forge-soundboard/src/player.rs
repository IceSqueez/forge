use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioEvent, AudioEventSink, AudioSink, PcmBuffer, PlaybackHandle};
use forge_runtime::{SoundPlayer, SoundPlayerError};
use forge_storage::SoundboardClipsRepo;
use forge_types::{ClipId, OutputDevice};

use crate::error::SoundboardError;
use crate::settings::{SoundboardSettings, SoundboardSettingsHandle};
use crate::sink_factory::AudioSinkFactory;

/// Upper bound on the linear master gain. +6 dB (the catalog ceiling) is ≈2.0;
/// 4.0 leaves headroom for an interpolated/over-range request without letting a
/// runaway value blow the sample scaling.
const MAX_MASTER_GAIN: f32 = 4.0;

/// Silence drained by the device after a clip's samples are exhausted; the
/// registry entry outlives the clip by this much so a late `stop` still lands.
const PLAYBACK_TAIL_MS: u64 = 200;

const LOOP_POLL_MS: u64 = 50;

const LOOP_MIN_CYCLE_MS: u64 = 50;

#[derive(Clone)]
enum StopToken {
    Handle(PlaybackHandle),
    Loop {
        should_stop: Arc<AtomicBool>,
        current: Arc<Mutex<PlaybackHandle>>,
    },
}

impl StopToken {
    fn stop(&self) {
        match self {
            StopToken::Handle(handle) => handle.stop(),
            StopToken::Loop {
                should_stop,
                current,
            } => {
                should_stop.store(true, Ordering::Relaxed);
                let handle = current.lock().unwrap_or_else(PoisonError::into_inner);
                handle.stop();
            }
        }
    }
}

type ActiveRegistry = Arc<Mutex<HashMap<ClipId, Vec<(u64, StopToken)>>>>;

pub struct SoundboardPlayer {
    sink_factory: Arc<dyn AudioSinkFactory>,
    event_sink: Arc<dyn AudioEventSink>,
    clips_repo: Arc<dyn SoundboardClipsRepo>,
    /// Live stop tokens keyed by clip; the `u64` tags one concrete play so
    /// concurrent plays of the same clip register and clean up independently.
    active: ActiveRegistry,
    next_play_id: AtomicU64,
    master_gain_bits: AtomicU32,
    settings: SoundboardSettingsHandle,
}

impl SoundboardPlayer {
    pub fn with_settings(
        sink_factory: Arc<dyn AudioSinkFactory>,
        event_sink: Arc<dyn AudioEventSink>,
        clips_repo: Arc<dyn SoundboardClipsRepo>,
        settings: SoundboardSettingsHandle,
    ) -> Self {
        let master_volume = settings.load().master_volume.clamp(0.0, MAX_MASTER_GAIN);
        Self {
            sink_factory,
            event_sink,
            clips_repo,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_play_id: AtomicU64::new(0),
            master_gain_bits: AtomicU32::new(master_volume.to_bits()),
            settings,
        }
    }

    pub fn settings_handle(&self) -> SoundboardSettingsHandle {
        self.settings.clone()
    }

    pub fn update_settings(&self, settings: SoundboardSettings) {
        self.set_master_volume(settings.master_volume);
        self.settings.swap(settings);
    }

    pub fn set_master_volume(&self, gain: f32) {
        let clamped = gain.clamp(0.0, MAX_MASTER_GAIN);
        self.master_gain_bits
            .store(clamped.to_bits(), Ordering::Relaxed);
    }

    pub fn stop(&self, clip_id: ClipId) {
        let tokens = {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            guard.remove(&clip_id).unwrap_or_default()
        };
        if tokens.is_empty() {
            return;
        }
        for (_id, token) in &tokens {
            token.stop();
        }
        self.event_sink.emit(AudioEvent::PlaybackFinished {
            clip_id: Some(clip_id),
        });
    }

    pub fn stop_all(&self) {
        let drained = {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *guard)
        };
        for (clip_id, tokens) in &drained {
            for (_id, token) in tokens {
                token.stop();
            }
            self.event_sink.emit(AudioEvent::PlaybackFinished {
                clip_id: Some(*clip_id),
            });
        }
    }

    pub async fn ensure_clip_duration(
        &self,
        clip_id: ClipId,
    ) -> Result<Option<f32>, SoundboardError> {
        let mut clip = self
            .clips_repo
            .get(clip_id)
            .await
            .map_err(|e| SoundboardError::Storage(e.to_string()))?
            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;

        if clip.duration_secs.is_some() {
            return Ok(clip.duration_secs);
        }

        let path = clip.file_path.clone();
        let probed =
            tokio::task::spawn_blocking(move || crate::duration::probe_clip_duration_secs(&path))
                .await
                .map_err(|e| SoundboardError::JoinError(e.to_string()))??;

        clip.duration_secs = Some(probed);
        self.clips_repo
            .save(&clip)
            .await
            .map_err(|e| SoundboardError::Storage(e.to_string()))?;

        Ok(Some(probed))
    }

    pub async fn play(
        &self,
        clip_id: ClipId,
        override_device: Option<OutputDevice>,
    ) -> Result<(), SoundboardError> {
        let settings = self.settings.load();
        if !settings.enabled {
            tracing::debug!(clip_id = %clip_id, "soundboard disabled by settings; play request ignored");
            return Ok(());
        }

        let clip = self
            .clips_repo
            .get(clip_id)
            .await
            .map_err(|e| SoundboardError::Storage(e.to_string()))?
            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;

        let device = resolve_device(&clip.output_device, override_device, &settings);
        let device_label = device_label(&device);

        let sinks = match self.build_sinks(&device, settings.also_headphones).await {
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
        let mut buffer = buffer;
        buffer.apply_gain(gain);

        let sample_rate = buffer.sample_rate.max(1) as u64;
        let channels = buffer.channels.max(1) as u64;
        let frames = (buffer.samples.len() as u64) / channels;
        let duration_ms = frames.saturating_mul(1000) / sample_rate;

        self.event_sink.emit(AudioEvent::PlaybackStarted {
            clip_id: Some(clip_id),
            device: device_label,
            duration_secs: Some(duration_ms as f64 / 1000.0),
            looped: clip.loop_playback,
        });

        if clip.loop_playback {
            self.play_looping(clip_id, sinks, buffer, duration_ms);
            return Ok(());
        }

        match play_target(&sinks, buffer).await {
            Ok(handle) => {
                self.register(clip_id, handle, duration_ms);
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

    async fn build_sinks(
        &self,
        device: &OutputDevice,
        also_headphones: bool,
    ) -> Result<Vec<Arc<dyn AudioSink>>, AudioError> {
        let targets = forge_audio::fan_out_targets(device, also_headphones);
        let mut sinks = Vec::with_capacity(targets.len());
        for (idx, target) in targets.iter().enumerate() {
            match self.sink_factory.build(target).await {
                Ok(sink) => sinks.push(sink),
                Err(e) if idx == 0 => return Err(e),
                Err(e) => {
                    tracing::warn!(error = %e, "also_headphones fan-out sink build failed, playing to primary device only");
                }
            }
        }

        Ok(sinks)
    }

    fn register(&self, clip_id: ClipId, handle: PlaybackHandle, duration_ms: u64) {
        let play_id = self.next_play_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            guard
                .entry(clip_id)
                .or_default()
                .push((play_id, StopToken::Handle(handle)));
        }

        let active = Arc::clone(&self.active);
        let event_sink = Arc::clone(&self.event_sink);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(duration_ms + PLAYBACK_TAIL_MS)).await;
            let completed_naturally = {
                let mut guard = active.lock().unwrap_or_else(PoisonError::into_inner);
                match guard.get_mut(&clip_id) {
                    Some(plays) => {
                        let before = plays.len();
                        plays.retain(|(id, _)| *id != play_id);
                        let removed = plays.len() < before;
                        if plays.is_empty() {
                            guard.remove(&clip_id);
                        }
                        removed
                    }
                    None => false,
                }
            };
            if completed_naturally {
                event_sink.emit(AudioEvent::PlaybackFinished {
                    clip_id: Some(clip_id),
                });
            }
        });
    }

    fn play_looping(
        &self,
        clip_id: ClipId,
        sinks: Vec<Arc<dyn AudioSink>>,
        buffer: PcmBuffer,
        duration_ms: u64,
    ) {
        let should_stop = Arc::new(AtomicBool::new(false));
        let current = Arc::new(Mutex::new(PlaybackHandle::default()));
        let play_id = self.next_play_id.fetch_add(1, Ordering::Relaxed);

        {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            guard.entry(clip_id).or_default().push((
                play_id,
                StopToken::Loop {
                    should_stop: Arc::clone(&should_stop),
                    current: Arc::clone(&current),
                },
            ));
        }

        let active = Arc::clone(&self.active);
        let event_sink = Arc::clone(&self.event_sink);
        let cycle_ms = duration_ms.max(LOOP_MIN_CYCLE_MS);

        tokio::spawn(async move {
            loop {
                if should_stop.load(Ordering::Relaxed) {
                    break;
                }

                match play_target(&sinks, buffer.clone()).await {
                    Ok(handle) => {
                        let mut guard = current.lock().unwrap_or_else(PoisonError::into_inner);
                        *guard = handle;
                    }
                    Err(e) => {
                        event_sink.emit(AudioEvent::PlaybackFailed {
                            clip_id: Some(clip_id),
                            error: e.to_string(),
                        });
                        break;
                    }
                }

                let mut waited = 0u64;
                while waited < cycle_ms {
                    if should_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let step = LOOP_POLL_MS.min(cycle_ms - waited);
                    tokio::time::sleep(Duration::from_millis(step)).await;
                    waited += step;
                }
            }

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

async fn play_target(
    sinks: &[Arc<dyn AudioSink>],
    buffer: PcmBuffer,
) -> Result<PlaybackHandle, AudioError> {
    let (handle, mut outcomes) = forge_audio::fan_out_stoppable(buffer, sinks).await;
    let main = outcomes.remove(0);
    for (idx, outcome) in outcomes.into_iter().enumerate() {
        if let Err(e) = outcome {
            tracing::warn!(sink_index = idx + 1, error = %e, "secondary output sink failed");
        }
    }
    main?;
    Ok(handle)
}

fn resolve_device(
    clip_device: &OutputDevice,
    override_device: Option<OutputDevice>,
    settings: &SoundboardSettings,
) -> OutputDevice {
    if let Some(dev) = override_device {
        return dev;
    }
    if !matches!(clip_device, OutputDevice::Default) {
        return clip_device.clone();
    }
    settings.output_device()
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
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    /// Writes the exact `samples` as a mono 16-bit PCM wav. 16-bit PCM is
    /// lossless, so `forge_audio::decode_file` hands them back verbatim - letting
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
        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
            SoundboardSettingsHandle::default(),
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

        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
            SoundboardSettingsHandle::default(),
        );

        player.play(clip_id, None).await.unwrap();

        assert_eq!(*play_count.lock().unwrap(), 1, "sink must be called once");

        // PlaybackFinished now fires once the clip's own decoded duration has
        // actually elapsed (natural-completion cleanup task), not synchronously
        // after `play` returns - poll with a bound instead of asserting inline.
        let deadline = std::time::Instant::now() + Duration::from_millis(2_000);
        while events.lock().unwrap().len() < 2 && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

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
    /// the decode round-trip cancels - exact equality is valid here.)
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

    /// A negative master gain clamps to 0.0 - silence, never sign inversion.
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
    /// without effect (documented contract) and without panicking - guards the
    /// `remove(..).unwrap_or_default()` / `PoisonError::into_inner` paths against
    /// an accidental `.unwrap()`.
    #[tokio::test]
    async fn stop_on_idle_player_succeeds_without_effect() {
        let (factory, _count, _buf) = CountingFactory::new();
        let (event_sink, _events) = RecordingEventSink::new();
        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(MockClipsRepo { clip: None }),
            SoundboardSettingsHandle::default(),
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

        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            Arc::new(clips_repo),
            SoundboardSettingsHandle::default(),
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
        let player = SoundboardPlayer::with_settings(
            Arc::new(DeviceCapturingFactory {
                captured: received_device_clone,
            }),
            Arc::new(event_sink),
            Arc::new(clips_repo),
            SoundboardSettingsHandle::default(),
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
