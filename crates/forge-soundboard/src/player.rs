use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioEvent, AudioEventSink, AudioRoute, AudioSink, PcmBuffer, PlaybackHandle,
};
use forge_runtime::{SoundPlayer, SoundPlayerError};
use forge_types::{ClipId, OutputDevice, Shared};

use crate::error::SoundboardError;
use crate::library::{ClipLibrary, ClipSource};
use crate::settings::{SoundboardSettings, SoundboardSettingsHandle};
use crate::sink_factory::AudioSinkFactory;

/// Linear gain; the +6 dB catalog ceiling is ~2.0, this leaves headroom above it.
const MAX_MASTER_GAIN: f32 = 4.0;

/// Registry entry outlives clip playback by this much so a late `stop` still lands.
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

type ActiveRegistry = Arc<Mutex<HashMap<ClipId, Vec<(u64, StopToken, String)>>>>;

#[derive(Clone, Default)]
pub struct ClipRoute {
    route: AudioRoute,
    overlay: Option<Arc<dyn AudioSink>>,
}

impl ClipRoute {
    pub fn new(route: AudioRoute, overlay: Option<Arc<dyn AudioSink>>) -> Self {
        Self { route, overlay }
    }

    fn overlay_leg(&self) -> Option<&Arc<dyn AudioSink>> {
        self.overlay.as_ref().filter(|_| self.route.plays_overlay())
    }
}

pub struct SoundboardPlayer {
    sink_factory: Arc<dyn AudioSinkFactory>,
    event_sink: Arc<dyn AudioEventSink>,
    library: Arc<ClipLibrary>,
    /// The `u64` tags one concrete play so concurrent plays of the same clip differ.
    active: ActiveRegistry,
    next_play_id: AtomicU64,
    master_gain_bits: AtomicU32,
    settings: SoundboardSettingsHandle,
    clip_route: Shared<ClipRoute>,
}

impl SoundboardPlayer {
    pub fn with_settings(
        sink_factory: Arc<dyn AudioSinkFactory>,
        event_sink: Arc<dyn AudioEventSink>,
        library: Arc<ClipLibrary>,
        settings: SoundboardSettingsHandle,
    ) -> Self {
        let master_volume = settings.load().master_volume.clamp(0.0, MAX_MASTER_GAIN);
        Self {
            sink_factory,
            event_sink,
            library,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_play_id: AtomicU64::new(0),
            master_gain_bits: AtomicU32::new(master_volume.to_bits()),
            settings,
            clip_route: Shared::new(ClipRoute::default()),
        }
    }

    /// Installed once at boot; a later install reaches the next clip and leaves a playing one untouched.
    pub fn install_route(&self, route: ClipRoute) {
        self.clip_route.store(route);
    }

    pub fn library(&self) -> &Arc<ClipLibrary> {
        &self.library
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
        let clip_label = tokens.first().map(|(_, _, label)| label.clone());
        for (_id, token, _label) in &tokens {
            token.stop();
        }
        self.event_sink.emit(AudioEvent::PlaybackFinished {
            clip_id: Some(clip_id),
            clip_label,
        });
    }

    pub fn stop_all(&self) {
        let drained = {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *guard)
        };
        for (clip_id, tokens) in &drained {
            let clip_label = tokens.first().map(|(_, _, label)| label.clone());
            for (_id, token, _label) in tokens {
                token.stop();
            }
            self.event_sink.emit(AudioEvent::PlaybackFinished {
                clip_id: Some(*clip_id),
                clip_label,
            });
        }
    }

    pub async fn ensure_clip_duration(
        &self,
        clip_id: ClipId,
    ) -> Result<Option<f32>, SoundboardError> {
        let mut clip = self
            .library
            .get(clip_id)
            .await?
            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;

        if clip.duration_secs.is_some() {
            return Ok(clip.duration_secs);
        }

        let Some(path) = self
            .library
            .source_of(&clip)
            .await
            .path()
            .map(Path::to_path_buf)
        else {
            return Err(SoundboardError::SourceMissing(clip.name.clone()));
        };
        let probed =
            tokio::task::spawn_blocking(move || crate::duration::probe_clip_duration_secs(&path))
                .await
                .map_err(|e| SoundboardError::JoinError(e.to_string()))??;

        clip.duration_secs = Some(probed);
        self.library.save_clip(&clip).await?;

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
            .library
            .get(clip_id)
            .await?
            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;

        let path = match self.library.source_of(&clip).await {
            ClipSource::Managed(path) => path,
            ClipSource::Legacy(path) => {
                self.library.adopt_in_background(clip.id, path.clone());
                path
            }
            ClipSource::Missing => {
                let failure = SoundboardError::SourceMissing(clip.name.clone());
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    clip_label: Some(clip.name.clone()),
                    error: failure.to_string(),
                });
                return Err(failure);
            }
        };

        let device = resolve_device(&clip.output_device, override_device, &settings);
        let device_label = device_label(&device);

        let sinks = match self.build_sinks(&device, settings.also_headphones).await {
            Ok(s) => s,
            Err(e) => {
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    clip_label: Some(clip.name.clone()),
                    error: e.to_string(),
                });
                return Err(SoundboardError::Audio(e));
            }
        };

        let buffer = match tokio::task::spawn_blocking(move || forge_audio::decode_file(&path))
            .await
            .map_err(|e| SoundboardError::JoinError(e.to_string()))
            .and_then(|r| r.map_err(SoundboardError::Audio))
        {
            Ok(b) => b,
            Err(e) => {
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    clip_label: Some(clip.name.clone()),
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
            clip_label: Some(clip.name.clone()),
            device: device_label,
            duration_secs: Some(duration_ms as f64 / 1000.0),
            looped: clip.loop_playback,
        });

        if clip.loop_playback {
            self.play_looping(clip_id, sinks, buffer, duration_ms, clip.name.clone());
            return Ok(());
        }

        match play_target(&sinks, buffer).await {
            Ok(handle) => {
                self.register(clip_id, handle, duration_ms, clip.name.clone());
                Ok(())
            }
            Err(e) => {
                let error = e.to_string();
                self.event_sink.emit(AudioEvent::PlaybackFailed {
                    clip_id: Some(clip_id),
                    clip_label: Some(clip.name.clone()),
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
        let clip_route = self.clip_route.load();
        let overlay = clip_route.overlay_leg();
        let mut sinks: Vec<Arc<dyn AudioSink>> = Vec::new();

        if clip_route.route.plays_local() {
            for (idx, target) in forge_audio::fan_out_targets(device, also_headphones)
                .iter()
                .enumerate()
            {
                match self.sink_factory.build(target).await {
                    Ok(sink) => sinks.push(sink),
                    Err(e) if idx == 0 && overlay.is_none() => return Err(e),
                    Err(e) => {
                        tracing::warn!(error = %e, sink_index = idx, "output sink build failed, playing the remaining routes only");
                    }
                }
            }
        }
        if let Some(overlay) = overlay {
            sinks.push(Arc::clone(overlay));
        }
        if sinks.is_empty() {
            return Err(AudioError::NoRoute);
        }

        Ok(sinks)
    }

    fn register(&self, clip_id: ClipId, handle: PlaybackHandle, duration_ms: u64, label: String) {
        let play_id = self.next_play_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut guard = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            guard.entry(clip_id).or_default().push((
                play_id,
                StopToken::Handle(handle),
                label.clone(),
            ));
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
                        plays.retain(|(id, _, _)| *id != play_id);
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
                    clip_label: Some(label),
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
        label: String,
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
                label.clone(),
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
                            clip_label: Some(label.clone()),
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
                plays.retain(|(id, _, _)| *id != play_id);
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
    let (handle, outcomes) = forge_audio::fan_out_stoppable(buffer, sinks).await;
    let mut outcomes = outcomes.into_iter();
    let Some(main) = outcomes.next() else {
        return Err(AudioError::NoRoute);
    };
    for (idx, outcome) in outcomes.enumerate() {
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
    clippy::panic,
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
    use forge_storage::{
        MediaBlob, MediaBlobId, MediaReferrer, MediaReferrerKind, MediaRepo, SoundboardClipsRepo,
        StorageError, StoredClip,
    };
    use forge_types::{ClipId, OutputDevice};

    use super::*;
    use crate::sink_factory::AudioSinkFactory;

    struct NoManagedMedia {
        imports: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl MediaRepo for NoManagedMedia {
        async fn store(&self, _label: &str, _bytes: Vec<u8>) -> Result<MediaBlob, StorageError> {
            Err(StorageError::NotReady)
        }

        async fn import_file(&self, _source: &Path) -> Result<MediaBlob, StorageError> {
            self.imports.fetch_add(1, Ordering::Relaxed);
            Err(StorageError::NotReady)
        }

        async fn get(&self, _id: &MediaBlobId) -> Result<Option<MediaBlob>, StorageError> {
            Ok(None)
        }

        async fn list(&self) -> Result<Vec<MediaBlob>, StorageError> {
            Ok(Vec::new())
        }

        async fn total_bytes(&self) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn resolve(&self, id: &MediaBlobId) -> Result<PathBuf, StorageError> {
            Err(StorageError::NotFound {
                key: id.as_str().to_owned(),
            })
        }

        async fn read(&self, id: &MediaBlobId) -> Result<Vec<u8>, StorageError> {
            Err(StorageError::NotFound {
                key: id.as_str().to_owned(),
            })
        }

        async fn delete(&self, _id: &MediaBlobId) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn retain(
            &self,
            _referrer: &MediaReferrer,
            _id: &MediaBlobId,
        ) -> Result<(), StorageError> {
            Ok(())
        }

        async fn release(&self, _referrer: &MediaReferrer) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn release_all(
            &self,
            _kind: MediaReferrerKind,
            _referrer_id: &str,
        ) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn referrers(&self, _id: &MediaBlobId) -> Result<Vec<MediaReferrer>, StorageError> {
            Ok(Vec::new())
        }

        async fn blob_of(
            &self,
            _referrer: &MediaReferrer,
        ) -> Result<Option<MediaBlobId>, StorageError> {
            Ok(None)
        }
    }

    fn unadopted_library(clip: Option<StoredClip>) -> Arc<ClipLibrary> {
        counted_unadopted_library(clip).0
    }

    fn counted_unadopted_library(
        clip: Option<StoredClip>,
    ) -> (Arc<ClipLibrary>, Arc<std::sync::atomic::AtomicUsize>) {
        let imports = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let library = Arc::new(ClipLibrary::new(
            Arc::new(MockClipsRepo { clip }),
            Arc::new(NoManagedMedia {
                imports: Arc::clone(&imports),
            }),
        ));
        (library, imports)
    }

    async fn settle_background_work() {
        for _ in 0..BACKGROUND_SETTLE_POLLS {
            tokio::task::yield_now().await;
        }
    }

    const BACKGROUND_SETTLE_POLLS: usize = 8;

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

    fn wav_with_samples(samples: &[i16]) -> tempfile::NamedTempFile {
        let wav_bytes = write_wav(22_050, 1, samples);
        let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(tmp.path(), &wav_bytes).unwrap();
        tmp
    }

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
        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            SoundboardSettingsHandle::default(),
        );

        if let Some(gain) = master_gain {
            player.set_master_volume(gain);
        }
        player.play(clip_id, None).await.unwrap();

        let buf = last_buf.lock().unwrap();
        buf.as_ref().unwrap().samples.clone()
    }

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

        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            SoundboardSettingsHandle::default(),
        );

        player.play(clip_id, None).await.unwrap();

        assert_eq!(*play_count.lock().unwrap(), 1, "sink must be called once");

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
            AudioEvent::PlaybackFinished {
                clip_id: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn play_applies_clip_volume_to_samples() {
        let samples = vec![0, 200, -200, 12_000, -12_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 0.5, None).await;
        assert_proportional(&baseline, &out, 0.5);
    }

    #[tokio::test]
    async fn master_volume_defaults_to_unity() {
        let samples = vec![0, 100, -100, 12_000, -12_000];
        let default_master = capture_played_samples(&samples, 1.0, None).await;
        let explicit_unity = capture_played_samples(&samples, 1.0, Some(1.0)).await;
        assert_eq!(default_master, explicit_unity);
    }

    #[tokio::test]
    async fn master_volume_scales_samples_by_linear_gain() {
        let samples = vec![0, 200, -200, 12_000, -12_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 1.0, Some(0.5)).await;
        assert_proportional(&baseline, &out, 0.5);
    }

    #[tokio::test]
    async fn master_gain_multiplies_with_clip_volume() {
        let samples = vec![0, 400, -400, 8_000, -8_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 0.5, Some(0.5)).await;
        assert_proportional(&baseline, &out, 0.25);
    }

    #[tokio::test]
    async fn master_volume_clamps_gain_above_ceiling() {
        let samples = vec![100, -100, 4_000, -4_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 1.0, Some(10.0)).await;
        assert_proportional(&baseline, &out, 4.0);
    }

    #[tokio::test]
    async fn master_volume_clamps_negative_gain_to_silence() {
        let samples = vec![12_000, -12_000, 5_000];
        let out = capture_played_samples(&samples, 1.0, Some(-1.0)).await;
        assert!(
            out.iter().all(|&s| s == 0),
            "negative master gain must produce silence, got {out:?}"
        );
    }

    #[tokio::test]
    async fn stop_on_idle_player_succeeds_without_effect() {
        let (factory, _count, _buf) = CountingFactory::new();
        let (event_sink, _events) = RecordingEventSink::new();
        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            unadopted_library(None),
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

        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            unadopted_library(None),
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
        let player = SoundboardPlayer::with_settings(
            Arc::new(DeviceCapturingFactory {
                captured: received_device_clone,
            }),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
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

    #[tokio::test]
    async fn play_reports_a_missing_source_when_neither_copy_of_the_clip_resolves() {
        let clip_id = ClipId::new();
        let clip = make_stored_clip(clip_id, PathBuf::from("/forge-qa/never-written.wav"));
        let (event_sink, _events) = RecordingEventSink::new();
        let (factory, play_count, _last_buf) = CountingFactory::new();

        let player = SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            SoundboardSettingsHandle::default(),
        );

        let error = player.play(clip_id, None).await.unwrap_err();

        assert!(
            matches!(error, SoundboardError::SourceMissing(ref name) if name == "test clip"),
            "a clip with no readable source did not name itself: {error:?}"
        );
        assert_eq!(*play_count.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn play_names_the_clip_in_the_failure_it_emits_for_a_missing_source() {
        let clip_id = ClipId::new();
        let clip = make_stored_clip(clip_id, PathBuf::from("/forge-qa/never-written.wav"));
        let (event_sink, events) = RecordingEventSink::new();

        let player = SoundboardPlayer::with_settings(
            Arc::new(CountingFactory::new().0),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            SoundboardSettingsHandle::default(),
        );

        let _ = player.play(clip_id, None).await;

        let recorded = events.lock().unwrap();
        let [AudioEvent::PlaybackFailed { error, .. }] = recorded.as_slice() else {
            panic!("expected a single PlaybackFailed, got {recorded:?}");
        };
        assert!(
            error.contains("test clip"),
            "the emitted failure does not name the clip: {error}"
        );
    }

    #[tokio::test]
    async fn play_schedules_the_import_of_a_clip_that_is_not_in_the_managed_library_yet() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        let (event_sink, _events) = RecordingEventSink::new();
        let (library, imports) = counted_unadopted_library(Some(clip));

        let player = SoundboardPlayer::with_settings(
            Arc::new(CountingFactory::new().0),
            Arc::new(event_sink),
            library,
            SoundboardSettingsHandle::default(),
        );

        player.play(clip_id, None).await.unwrap();
        settle_background_work().await;

        assert_eq!(imports.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn play_reports_playback_failed_when_no_output_device_can_be_built() {
        struct NoDeviceFactory;

        #[async_trait]
        impl AudioSinkFactory for NoDeviceFactory {
            async fn build(
                &self,
                _device: &OutputDevice,
            ) -> Result<Arc<dyn AudioSink>, AudioError> {
                Err(AudioError::NoDefaultDevice)
            }
        }

        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        let (event_sink, events) = RecordingEventSink::new();

        let player = SoundboardPlayer::with_settings(
            Arc::new(NoDeviceFactory),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            SoundboardSettingsHandle::default(),
        );

        let error = player.play(clip_id, None).await.unwrap_err();

        assert!(
            matches!(error, SoundboardError::Audio(AudioError::NoDefaultDevice)),
            "a sink that cannot be built did not surface as an audio failure: {error:?}"
        );
        let recorded = events.lock().unwrap();
        assert!(
            matches!(
                recorded.as_slice(),
                [AudioEvent::PlaybackFailed { clip_id: Some(id), .. }] if *id == clip_id
            ),
            "expected a single PlaybackFailed for the clip, got {recorded:?}"
        );
    }

    const PRIMARY_DEVICE_ID: &str = "speakers";
    const OVERLAY_LEG: &str = "overlay";

    #[derive(Clone)]
    struct LegLog(Arc<Mutex<Vec<String>>>);

    impl LegLog {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn fed(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    struct LoggingSink {
        label: String,
        log: LegLog,
    }

    #[async_trait]
    impl AudioSink for LoggingSink {
        async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
            self.log.0.lock().unwrap().push(self.label.clone());
            Ok(())
        }
    }

    struct LoggingFactory {
        log: LegLog,
        built: Arc<Mutex<Vec<OutputDevice>>>,
        failing_builds: Vec<usize>,
    }

    impl LoggingFactory {
        fn new(log: &LegLog, failing_builds: &[usize]) -> (Self, Arc<Mutex<Vec<OutputDevice>>>) {
            let built = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    log: log.clone(),
                    built: Arc::clone(&built),
                    failing_builds: failing_builds.to_vec(),
                },
                built,
            )
        }
    }

    #[async_trait]
    impl AudioSinkFactory for LoggingFactory {
        async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            let index = {
                let mut built = self.built.lock().unwrap();
                built.push(device.clone());
                built.len() - 1
            };
            if self.failing_builds.contains(&index) {
                return Err(AudioError::NoDefaultDevice);
            }
            Ok(Arc::new(LoggingSink {
                label: format!("device:{}", device_label(device)),
                log: self.log.clone(),
            }))
        }
    }

    fn overlay_sink(log: &LegLog) -> Arc<dyn AudioSink> {
        Arc::new(LoggingSink {
            label: OVERLAY_LEG.to_string(),
            log: log.clone(),
        })
    }

    fn built_labels(built: &Arc<Mutex<Vec<OutputDevice>>>) -> Vec<String> {
        built.lock().unwrap().iter().map(device_label).collect()
    }

    fn labels(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// A named primary plus `also_headphones` is the only shape that fans out to two device legs.
    fn fan_out_settings() -> SoundboardSettingsHandle {
        SoundboardSettingsHandle::new(SoundboardSettings {
            output_device_id: Some(PRIMARY_DEVICE_ID.to_string()),
            also_headphones: true,
            ..SoundboardSettings::default()
        })
    }

    fn fan_out_player(factory: LoggingFactory, clip: StoredClip) -> SoundboardPlayer {
        let (event_sink, _events) = RecordingEventSink::new();
        SoundboardPlayer::with_settings(
            Arc::new(factory),
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            fan_out_settings(),
        )
    }

    #[tokio::test]
    async fn the_installed_route_selects_which_legs_receive_the_clip() {
        for (route, expected_devices, expected_legs) in [
            (
                AudioRoute::Local,
                &["speakers", "default"][..],
                &["device:speakers", "device:default"][..],
            ),
            (AudioRoute::Overlay, &[][..], &["overlay"][..]),
            (
                AudioRoute::Both,
                &["speakers", "default"][..],
                &["device:speakers", "device:default", "overlay"][..],
            ),
        ] {
            let clip_id = ClipId::new();
            let tmp = make_wav_tempfile(22_050, 1, 100);
            let log = LegLog::new();
            let (factory, built) = LoggingFactory::new(&log, &[]);
            let player =
                fan_out_player(factory, make_stored_clip(clip_id, tmp.path().to_path_buf()));
            player.install_route(ClipRoute::new(route, Some(overlay_sink(&log))));

            player.play(clip_id, None).await.unwrap();

            assert_eq!(
                built_labels(&built),
                labels(expected_devices),
                "route {route} asked the sink factory for the wrong devices"
            );
            assert_eq!(
                log.fed(),
                labels(expected_legs),
                "route {route} fed the wrong legs, in the wrong order"
            );
        }
    }

    #[tokio::test]
    async fn a_failing_primary_device_degrades_to_the_installed_overlay_leg() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let log = LegLog::new();
        let (factory, _built) = LoggingFactory::new(&log, &[0, 1]);
        let player = fan_out_player(factory, make_stored_clip(clip_id, tmp.path().to_path_buf()));
        player.install_route(ClipRoute::new(AudioRoute::Both, Some(overlay_sink(&log))));

        let played = player.play(clip_id, None).await;

        assert!(
            played.is_ok(),
            "an overlay leg must keep the clip playing when no device sink builds: {played:?}"
        );
        assert_eq!(log.fed(), labels(&["overlay"]));
    }

    #[tokio::test]
    async fn a_failing_secondary_fan_out_sink_still_plays_on_the_primary_device() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let log = LegLog::new();
        let (factory, _built) = LoggingFactory::new(&log, &[1]);
        let player = fan_out_player(factory, make_stored_clip(clip_id, tmp.path().to_path_buf()));

        let played = player.play(clip_id, None).await;

        assert!(
            played.is_ok(),
            "a secondary fan-out failure must not fail the clip: {played:?}"
        );
        assert_eq!(log.fed(), labels(&["device:speakers"]));
    }

    #[tokio::test]
    async fn a_route_installed_after_a_clip_started_reaches_only_the_next_clip() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let log = LegLog::new();
        let (factory, _built) = LoggingFactory::new(&log, &[]);
        let player = fan_out_player(factory, make_stored_clip(clip_id, tmp.path().to_path_buf()));

        player.play(clip_id, None).await.unwrap();
        assert_eq!(
            log.fed(),
            labels(&["device:speakers", "device:default"]),
            "a clip started before the install must stay on the device legs"
        );

        player.install_route(ClipRoute::new(AudioRoute::Both, Some(overlay_sink(&log))));
        player.play(clip_id, None).await.unwrap();

        assert_eq!(
            log.fed(),
            labels(&[
                "device:speakers",
                "device:default",
                "device:speakers",
                "device:default",
                "overlay",
            ]),
            "the overlay leg must reach the next clip, and only that one"
        );
    }

    #[tokio::test]
    async fn an_overlay_route_with_no_installed_leg_fails_the_clip_with_no_route() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, 100);
        let log = LegLog::new();
        let (factory, _built) = LoggingFactory::new(&log, &[]);
        let player = fan_out_player(factory, make_stored_clip(clip_id, tmp.path().to_path_buf()));
        player.install_route(ClipRoute::new(AudioRoute::Overlay, None));

        let error = player.play(clip_id, None).await.unwrap_err();

        assert!(
            matches!(error, SoundboardError::Audio(AudioError::NoRoute)),
            "a misconfigured overlay route did not surface as NoRoute: {error:?}"
        );
    }
}
