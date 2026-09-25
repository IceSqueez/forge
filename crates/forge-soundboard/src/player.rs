use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioEvent, AudioEventSink, AudioRoute, AudioSink, ControlledPlayback, PcmBuffer,
    PlaybackHandle,
};
use forge_runtime::{SoundPlayer, SoundPlayerError};
use forge_storage::{SettingsRepo, StoredClip};
use forge_types::{ClipId, OutputDevice, Shared};
use tokio::sync::oneshot;

use crate::error::SoundboardError;
use crate::library::{ClipLibrary, ClipSource};
use crate::settings::{SoundboardSettings, SoundboardSettingsHandle};
use crate::sink_factory::AudioSinkFactory;

const MAX_VOLUME: f32 = 1.0;

/// Registry entry outlives clip playback by this much so a late `stop` still lands.
const PLAYBACK_TAIL_MS: u64 = 200;

const LOOP_POLL_MS: u64 = 50;

const LOOP_MIN_CYCLE_MS: u64 = 50;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct PlayLegs {
    stopped: bool,
    handles: Vec<PlaybackHandle>,
}

#[derive(Default)]
struct PlayControl {
    legs: Mutex<PlayLegs>,
}

impl PlayControl {
    fn stop(&self) {
        let handles = {
            let mut legs = lock(&self.legs);
            legs.stopped = true;
            std::mem::take(&mut legs.handles)
        };
        for handle in &handles {
            handle.stop();
        }
    }

    fn is_stopped(&self) -> bool {
        lock(&self.legs).stopped
    }

    /// Returns `false` after stopping `handles` when a stop already landed on this play.
    fn install(&self, handles: Vec<PlaybackHandle>) -> bool {
        let refused = {
            let mut legs = lock(&self.legs);
            if legs.stopped {
                Some(handles)
            } else {
                legs.handles = handles;
                None
            }
        };
        match refused {
            Some(handles) => {
                for handle in &handles {
                    handle.stop();
                }
                false
            }
            None => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipToggle {
    Started,
    Stopped,
    Ignored,
}

enum ToggleDecision {
    Stop(Vec<ActivePlay>),
    Start(Box<StoredClip>, Reservation),
    Ignore,
}

struct ActivePlay {
    play_id: u64,
    control: Arc<PlayControl>,
    label: String,
}

type ActiveRegistry = Arc<Mutex<HashMap<ClipId, Vec<ActivePlay>>>>;

fn retire(active: &ActiveRegistry, clip_id: ClipId, play_id: u64) -> bool {
    let mut guard = lock(active);
    let Some(plays) = guard.get_mut(&clip_id) else {
        return false;
    };
    let before = plays.len();
    plays.retain(|play| play.play_id != play_id);
    let removed = plays.len() < before;
    if plays.is_empty() {
        guard.remove(&clip_id);
    }
    removed
}

struct Reservation {
    clip_id: ClipId,
    play_id: u64,
    label: String,
    control: Arc<PlayControl>,
}

struct PreparedClip {
    sinks: Vec<Arc<dyn AudioSink>>,
    buffer: PcmBuffer,
    duration_ms: u64,
    device_label: String,
}

struct StartedLegs {
    handles: Vec<PlaybackHandle>,
    main: ControlledPlayback,
}

#[derive(Clone)]
struct PlayReporter {
    active: ActiveRegistry,
    event_sink: Arc<dyn AudioEventSink>,
}

impl PlayReporter {
    fn fail(&self, play: &Reservation, error: String) {
        if !retire(&self.active, play.clip_id, play.play_id) {
            return;
        }
        play.control.stop();
        self.event_sink.emit(AudioEvent::PlaybackFailed {
            clip_id: Some(play.clip_id),
            clip_label: Some(play.label.clone()),
            error,
        });
    }

    fn finish(&self, play: &Reservation) {
        if retire(&self.active, play.clip_id, play.play_id) {
            self.event_sink.emit(AudioEvent::PlaybackFinished {
                clip_id: Some(play.clip_id),
                clip_label: Some(play.label.clone()),
            });
        }
    }
}

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
    active: ActiveRegistry,
    next_play_id: AtomicU64,
    settings: SoundboardSettingsHandle,
    settings_store: Shared<Option<Arc<dyn SettingsRepo>>>,
    clip_route: Shared<ClipRoute>,
}

impl SoundboardPlayer {
    pub fn with_settings(
        sink_factory: Arc<dyn AudioSinkFactory>,
        event_sink: Arc<dyn AudioEventSink>,
        library: Arc<ClipLibrary>,
        settings: SoundboardSettingsHandle,
    ) -> Self {
        Self {
            sink_factory,
            event_sink,
            library,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_play_id: AtomicU64::new(0),
            settings,
            settings_store: Shared::new(None),
            clip_route: Shared::new(ClipRoute::default()),
        }
    }

    /// A later install reaches the next clip and leaves a playing one untouched.
    pub fn install_route(&self, route: ClipRoute) {
        self.clip_route.store(route);
    }

    /// Without a store, a volume set by a sub-action lasts only until restart.
    pub fn install_settings_store(&self, store: Arc<dyn SettingsRepo>) {
        self.settings_store.store(Some(store));
    }

    pub fn library(&self) -> &Arc<ClipLibrary> {
        &self.library
    }

    pub fn settings_handle(&self) -> SoundboardSettingsHandle {
        self.settings.clone()
    }

    pub fn update_settings(
        &self,
        change: impl FnOnce(&mut SoundboardSettings),
    ) -> Arc<SoundboardSettings> {
        self.settings.update(|settings| {
            change(settings);
            settings.master_volume = settings.master_volume.clamp(0.0, MAX_VOLUME);
        })
    }

    pub fn set_master_volume(&self, gain: f32) -> f32 {
        self.update_settings(|settings| settings.master_volume = gain)
            .master_volume
    }

    pub fn stop(&self, clip_id: ClipId) {
        let plays = lock(&self.active).remove(&clip_id).unwrap_or_default();
        self.halt(clip_id, &plays);
    }

    fn halt(&self, clip_id: ClipId, plays: &[ActivePlay]) {
        if plays.is_empty() {
            return;
        }
        let clip_label = plays.first().map(|play| play.label.clone());
        for play in plays {
            play.control.stop();
        }
        self.event_sink.emit(AudioEvent::PlaybackFinished {
            clip_id: Some(clip_id),
            clip_label,
        });
    }

    pub async fn toggle(
        &self,
        clip_id: ClipId,
        override_device: Option<OutputDevice>,
    ) -> Result<ClipToggle, SoundboardError> {
        let settings = self.settings.load();
        let lookup = if settings.enabled {
            Some(self.library.get(clip_id).await)
        } else {
            None
        };

        let decision = {
            let mut guard = lock(&self.active);
            match guard.remove(&clip_id).filter(|plays| !plays.is_empty()) {
                Some(plays) => ToggleDecision::Stop(plays),
                None => match lookup {
                    None => ToggleDecision::Ignore,
                    Some(lookup) => {
                        let clip = lookup?
                            .ok_or_else(|| SoundboardError::ClipNotFound(clip_id.to_string()))?;
                        let play = self.reserve_in(&mut guard, clip_id, clip.name.clone());
                        ToggleDecision::Start(Box::new(clip), play)
                    }
                },
            }
        };

        match decision {
            ToggleDecision::Stop(plays) => {
                self.halt(clip_id, &plays);
                Ok(ClipToggle::Stopped)
            }
            ToggleDecision::Start(clip, play) => {
                self.start(*clip, play, override_device, &settings).await?;
                Ok(ClipToggle::Started)
            }
            ToggleDecision::Ignore => {
                tracing::debug!(clip_id = %clip_id, "soundboard disabled by settings; toggle request ignored");
                Ok(ClipToggle::Ignored)
            }
        }
    }

    pub fn stop_all(&self) {
        let drained = std::mem::take(&mut *lock(&self.active));
        for (clip_id, plays) in &drained {
            let clip_label = plays.first().map(|play| play.label.clone());
            for play in plays {
                play.control.stop();
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

        let play = self.reserve(clip_id, clip.name.clone());
        self.start(clip, play, override_device, &settings).await
    }

    async fn start(
        &self,
        clip: StoredClip,
        play: Reservation,
        override_device: Option<OutputDevice>,
        settings: &SoundboardSettings,
    ) -> Result<(), SoundboardError> {
        let clip_id = clip.id;
        let reporter = self.reporter();

        let prepared = match self.prepare(&clip, override_device, settings).await {
            Ok(prepared) => prepared,
            Err(e) => return reported_failure(&reporter, &play, e),
        };
        if play.control.is_stopped() {
            return Ok(());
        }

        let legs = match play_target(&prepared.sinks, prepared.buffer.clone()).await {
            Ok(legs) => legs,
            Err(e) => return reported_failure(&reporter, &play, SoundboardError::Audio(e)),
        };
        if !play.control.install(legs.handles) {
            return Ok(());
        }

        self.event_sink.emit(AudioEvent::PlaybackStarted {
            clip_id: Some(clip_id),
            clip_label: Some(clip.name.clone()),
            device: prepared.device_label,
            duration_secs: Some(prepared.duration_ms as f64 / 1000.0),
            looped: clip.loop_playback,
        });
        if play.control.is_stopped() {
            self.event_sink.emit(AudioEvent::PlaybackFinished {
                clip_id: Some(clip_id),
                clip_label: Some(clip.name.clone()),
            });
            return Ok(());
        }

        if clip.loop_playback {
            tokio::spawn(run_loop(LoopPlay {
                play,
                reporter,
                sinks: prepared.sinks,
                buffer: prepared.buffer,
                cycle_ms: prepared.duration_ms.max(LOOP_MIN_CYCLE_MS),
                main: legs.main,
            }));
        } else {
            tokio::spawn(watch_single(
                play,
                reporter,
                legs.main,
                prepared.duration_ms,
            ));
        }
        Ok(())
    }

    fn reporter(&self) -> PlayReporter {
        PlayReporter {
            active: Arc::clone(&self.active),
            event_sink: Arc::clone(&self.event_sink),
        }
    }

    fn reserve(&self, clip_id: ClipId, label: String) -> Reservation {
        self.reserve_in(&mut lock(&self.active), clip_id, label)
    }

    fn reserve_in(
        &self,
        active: &mut HashMap<ClipId, Vec<ActivePlay>>,
        clip_id: ClipId,
        label: String,
    ) -> Reservation {
        let play_id = self.next_play_id.fetch_add(1, Ordering::Relaxed);
        let control = Arc::new(PlayControl::default());
        active.entry(clip_id).or_default().push(ActivePlay {
            play_id,
            control: Arc::clone(&control),
            label: label.clone(),
        });
        Reservation {
            clip_id,
            play_id,
            label,
            control,
        }
    }

    async fn prepare(
        &self,
        clip: &StoredClip,
        override_device: Option<OutputDevice>,
        settings: &SoundboardSettings,
    ) -> Result<PreparedClip, SoundboardError> {
        let path = match self.library.source_of(clip).await {
            ClipSource::Managed(path) => path,
            ClipSource::Legacy(path) => {
                self.library.adopt_in_background(clip.id, path.clone());
                path
            }
            ClipSource::Missing => return Err(SoundboardError::SourceMissing(clip.name.clone())),
        };

        let device = resolve_device(&clip.output_device, override_device, settings);
        let device_label = device_label(&device);
        let sinks = self.build_sinks(&device, settings.also_headphones).await?;

        let mut buffer = tokio::task::spawn_blocking(move || forge_audio::decode_file(&path))
            .await
            .map_err(|e| SoundboardError::JoinError(e.to_string()))??;
        buffer.apply_gain(
            clip.volume.clamp(0.0, MAX_VOLUME) * settings.master_volume.clamp(0.0, MAX_VOLUME),
        );

        let sample_rate = buffer.sample_rate.max(1) as u64;
        let channels = buffer.channels.max(1) as u64;
        let frames = (buffer.samples.len() as u64) / channels;
        let duration_ms = frames.saturating_mul(1000) / sample_rate;

        Ok(PreparedClip {
            sinks,
            buffer,
            duration_ms,
            device_label,
        })
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
}

fn reported_failure(
    reporter: &PlayReporter,
    play: &Reservation,
    error: SoundboardError,
) -> Result<(), SoundboardError> {
    if play.control.is_stopped() {
        return Ok(());
    }
    let reason = match &error {
        SoundboardError::Audio(audio) => audio.to_string(),
        other => other.to_string(),
    };
    reporter.fail(play, reason);
    Err(error)
}

/// The leg is awaited on its own task: dropping an unsettled completion stops the clip.
fn watch_main_leg(main: ControlledPlayback) -> oneshot::Receiver<AudioError> {
    let (failure_tx, failure_rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Err(e) = main.await {
            let _ = failure_tx.send(e);
        }
    });
    failure_rx
}

async fn main_leg_failure(failure: oneshot::Receiver<AudioError>) -> AudioError {
    match failure.await {
        Ok(error) => error,
        Err(_) => std::future::pending().await,
    }
}

async fn watch_single(
    play: Reservation,
    reporter: PlayReporter,
    main: ControlledPlayback,
    duration_ms: u64,
) {
    tokio::select! {
        error = main_leg_failure(watch_main_leg(main)) => reporter.fail(&play, error.to_string()),
        () = tokio::time::sleep(Duration::from_millis(duration_ms + PLAYBACK_TAIL_MS)) => {
            reporter.finish(&play);
        }
    }
}

struct LoopPlay {
    play: Reservation,
    reporter: PlayReporter,
    sinks: Vec<Arc<dyn AudioSink>>,
    buffer: PcmBuffer,
    cycle_ms: u64,
    main: ControlledPlayback,
}

async fn run_loop(looped: LoopPlay) {
    let LoopPlay {
        play,
        reporter,
        sinks,
        buffer,
        cycle_ms,
        main,
    } = looped;
    let mut failure = watch_main_leg(main);
    loop {
        tokio::select! {
            error = main_leg_failure(failure) => {
                reporter.fail(&play, error.to_string());
                return;
            }
            () = wait_cycle(&play.control, cycle_ms) => {}
        }
        if play.control.is_stopped() {
            break;
        }
        match play_target(&sinks, buffer.clone()).await {
            Ok(legs) => {
                if !play.control.install(legs.handles) {
                    break;
                }
                failure = watch_main_leg(legs.main);
            }
            Err(e) => {
                reporter.fail(&play, e.to_string());
                return;
            }
        }
    }
    retire(&reporter.active, play.clip_id, play.play_id);
}

async fn wait_cycle(control: &PlayControl, cycle_ms: u64) {
    let mut waited = 0u64;
    while waited < cycle_ms {
        if control.is_stopped() {
            return;
        }
        let step = LOOP_POLL_MS.min(cycle_ms - waited);
        tokio::time::sleep(Duration::from_millis(step)).await;
        waited += step;
    }
}

async fn play_target(
    sinks: &[Arc<dyn AudioSink>],
    buffer: PcmBuffer,
) -> Result<StartedLegs, AudioError> {
    let starts = sinks.iter().map(|sink| {
        let sink = Arc::clone(sink);
        let buffer = buffer.clone();
        async move { sink.play_controlled(buffer).await }
    });
    let mut outcomes = futures::future::join_all(starts).await.into_iter();
    let Some(main) = outcomes.next() else {
        return Err(AudioError::NoRoute);
    };

    let mut secondary = Vec::new();
    for (idx, outcome) in outcomes.enumerate() {
        match outcome {
            Ok(leg) => {
                secondary.push(leg.handle());
                tokio::spawn(watch_secondary(idx + 1, leg));
            }
            Err(e) => {
                tracing::warn!(sink_index = idx + 1, error = %e, "secondary output sink failed");
            }
        }
    }

    match main {
        Ok(main) => {
            let mut handles = vec![main.handle()];
            handles.extend(secondary);
            Ok(StartedLegs { handles, main })
        }
        Err(e) => {
            for handle in &secondary {
                handle.stop();
            }
            Err(e)
        }
    }
}

async fn watch_secondary(sink_index: usize, leg: ControlledPlayback) {
    if let Err(e) = leg.await {
        tracing::warn!(sink_index, error = %e, "secondary output sink stopped during playback");
    }
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
        let volume = SoundboardPlayer::set_master_volume(self, gain);
        let Some(store) = self.settings_store.load().as_ref().clone() else {
            return Ok(());
        };
        forge_storage::set_soundboard_master_volume(store.as_ref(), volume)
            .await
            .map_err(|e| SoundPlayerError::Play(e.to_string()))
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
        // Why: the ceiling is unity (the clip's own recorded loudness) - no boost
        // above it (closes QA finding S6).
        let samples = vec![100, -100, 4_000, -4_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 1.0, Some(10.0)).await;
        assert_proportional(&baseline, &out, 1.0);
    }

    #[tokio::test]
    async fn clip_volume_clamps_gain_above_ceiling() {
        let samples = vec![100, -100, 4_000, -4_000];
        let baseline = capture_played_samples(&samples, 1.0, None).await;
        let out = capture_played_samples(&samples, 10.0, None).await;
        assert_proportional(&baseline, &out, 1.0);
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

    const EVENT_DEADLINE: Duration = Duration::from_secs(2);
    const EVENT_POLL: Duration = Duration::from_millis(5);
    const ONE_SECOND_OF_FRAMES: usize = 22_050;
    const SHORT_CLIP_FRAMES: usize = 100;
    const UNPLUGGED: &str = "usb headset unplugged";

    type SharedEvents = Arc<Mutex<Vec<AudioEvent>>>;

    async fn wait_for_event(events: &SharedEvents, wanted: impl Fn(&AudioEvent) -> bool) {
        let deadline = tokio::time::Instant::now() + EVENT_DEADLINE;
        while tokio::time::Instant::now() < deadline {
            if events.lock().unwrap().iter().any(&wanted) {
                return;
            }
            tokio::time::sleep(EVENT_POLL).await;
        }
        panic!(
            "expected event never arrived; saw {:?}",
            events.lock().unwrap()
        );
    }

    fn started_count(events: &SharedEvents) -> usize {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, AudioEvent::PlaybackStarted { .. }))
            .count()
    }

    struct SingleSinkFactory(Arc<dyn AudioSink>);

    #[async_trait]
    impl AudioSinkFactory for SingleSinkFactory {
        async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            Ok(Arc::clone(&self.0))
        }
    }

    struct RefusingDevice;

    #[async_trait]
    impl AudioSink for RefusingDevice {
        async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
            Err(AudioError::Host(
                "device 'usb-headset' not found".to_owned(),
            ))
        }
    }

    struct DeliveryGate {
        at_delivery: usize,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    struct FakeOverlayPage {
        deliveries: std::sync::atomic::AtomicUsize,
        gate: Option<DeliveryGate>,
        commands: tokio::sync::mpsc::UnboundedSender<(String, forge_audio::RemoteCommand)>,
    }

    #[async_trait]
    impl forge_audio::RemoteAudioDestination for FakeOverlayPage {
        async fn deliver(
            &self,
            _destination: &forge_audio::RemoteDestinationId,
            _clip: forge_audio::RemoteClip,
        ) -> Result<forge_audio::RemoteDelivery, AudioError> {
            let n = self.deliveries.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(gate) = self.gate.as_ref().filter(|gate| gate.at_delivery == n) {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            Ok(forge_audio::RemoteDelivery {
                clip_id: forge_audio::RemoteClipId::new(format!("clip-{n}")),
                live_players: 1,
            })
        }

        async fn control(
            &self,
            _destination: &forge_audio::RemoteDestinationId,
            clip_id: &forge_audio::RemoteClipId,
            command: forge_audio::RemoteCommand,
        ) -> Result<(), AudioError> {
            let _ = self.commands.send((clip_id.expose().to_owned(), command));
            Ok(())
        }

        async fn verdict(
            &self,
            _clip_id: &forge_audio::RemoteClipId,
        ) -> Result<forge_audio::RemoteVerdict, AudioError> {
            std::future::pending().await
        }
    }

    type PageCommands = tokio::sync::mpsc::UnboundedReceiver<(String, forge_audio::RemoteCommand)>;

    fn overlay_page(gate: Option<DeliveryGate>) -> (Arc<dyn AudioSink>, PageCommands) {
        let (commands, received) = tokio::sync::mpsc::unbounded_channel();
        let page = FakeOverlayPage {
            deliveries: std::sync::atomic::AtomicUsize::new(0),
            gate,
            commands,
        };
        let sink = forge_audio::RemoteSink::new(
            Arc::new(page),
            forge_audio::RemoteDestinationId::new("soundboard"),
        );
        (Arc::new(sink), received)
    }

    async fn wait_for_stop_of(commands: &mut PageCommands, clip: &str) {
        let arrived = tokio::time::timeout(EVENT_DEADLINE, async {
            while let Some((id, command)) = commands.recv().await {
                if id == clip && command == forge_audio::RemoteCommand::Stop {
                    return true;
                }
            }
            false
        })
        .await;
        assert_eq!(arrived, Ok(true), "{clip} was never told to stop");
    }

    fn player_over(
        factory: Arc<dyn AudioSinkFactory>,
        clip: StoredClip,
    ) -> (SoundboardPlayer, SharedEvents) {
        let (event_sink, events) = RecordingEventSink::new();
        let player = SoundboardPlayer::with_settings(
            factory,
            Arc::new(event_sink),
            unadopted_library(Some(clip)),
            SoundboardSettingsHandle::default(),
        );
        (player, events)
    }

    #[tokio::test]
    async fn a_device_that_refuses_to_open_fails_the_play_and_reports_it_for_the_clip() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
        let (player, events) = player_over(
            Arc::new(SingleSinkFactory(Arc::new(RefusingDevice))),
            make_stored_clip(clip_id, tmp.path().to_path_buf()),
        );

        let played = player.play(clip_id, None).await;

        assert!(
            matches!(played, Err(SoundboardError::Audio(AudioError::Host(_)))),
            "a refused device open was not returned to the caller: {played:?}"
        );
        let recorded = events.lock().unwrap();
        assert!(
            matches!(
                recorded.as_slice(),
                [AudioEvent::PlaybackFailed { clip_id: Some(id), clip_label: Some(label), .. }]
                    if *id == clip_id && label == "test clip"
            ),
            "expected exactly one PlaybackFailed naming the clip and no PlaybackStarted, got {recorded:?}"
        );
    }

    #[tokio::test]
    async fn a_main_device_that_refuses_to_open_stops_the_overlay_leg_that_already_started() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
        let (overlay, mut commands) = overlay_page(None);
        let (player, _events) = player_over(
            Arc::new(SingleSinkFactory(Arc::new(RefusingDevice))),
            make_stored_clip(clip_id, tmp.path().to_path_buf()),
        );
        player.install_route(ClipRoute::new(AudioRoute::Both, Some(overlay)));

        let played = player.play(clip_id, None).await;

        assert!(played.is_err(), "the main leg failure must fail the play");
        wait_for_stop_of(&mut commands, "clip-1").await;
    }

    struct UnpluggableDevice {
        opened: Arc<std::sync::atomic::AtomicUsize>,
        unplug: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl AudioSink for UnpluggableDevice {
        async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
            Ok(())
        }

        async fn play_controlled(
            &self,
            _buffer: PcmBuffer,
        ) -> Result<ControlledPlayback, AudioError> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            let unplug = Arc::clone(&self.unplug);
            Ok(ControlledPlayback::from_future(async move {
                unplug.notified().await;
                Err(AudioError::DeviceLost(UNPLUGGED.to_owned()))
            }))
        }
    }

    #[tokio::test]
    async fn a_device_lost_mid_play_ends_a_looped_clip_with_a_failure() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, ONE_SECOND_OF_FRAMES);
        let mut clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        clip.loop_playback = true;
        let unplug = Arc::new(tokio::sync::Notify::new());
        let device = UnpluggableDevice {
            opened: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            unplug: Arc::clone(&unplug),
        };
        let (player, events) = player_over(Arc::new(SingleSinkFactory(Arc::new(device))), clip);

        player.play(clip_id, None).await.unwrap();
        assert_eq!(started_count(&events), 1);
        unplug.notify_one();

        wait_for_event(&events, |event| {
            matches!(
                event,
                AudioEvent::PlaybackFailed { clip_id: Some(id), clip_label: Some(label), error }
                    if *id == clip_id && label == "test clip" && error.contains(UNPLUGGED)
            )
        })
        .await;
    }

    struct GatedFactory {
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        opened: Arc<std::sync::atomic::AtomicUsize>,
    }

    struct OpenCountingDevice(Arc<std::sync::atomic::AtomicUsize>);

    #[async_trait]
    impl AudioSink for OpenCountingDevice {
        async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait]
    impl AudioSinkFactory for GatedFactory {
        async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(Arc::new(OpenCountingDevice(Arc::clone(&self.opened))))
        }
    }

    #[tokio::test]
    async fn a_stop_while_the_clip_is_still_being_prepared_keeps_it_from_playing() {
        for stop_all in [false, true] {
            let clip_id = ClipId::new();
            let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let factory = GatedFactory {
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
                opened: Arc::clone(&opened),
            };
            let (player, events) = player_over(
                Arc::new(factory),
                make_stored_clip(clip_id, tmp.path().to_path_buf()),
            );
            let player = Arc::new(player);

            let playing = tokio::spawn({
                let player = Arc::clone(&player);
                async move { player.play(clip_id, None).await }
            });
            tokio::time::timeout(EVENT_DEADLINE, entered.notified())
                .await
                .unwrap();
            if stop_all {
                player.stop_all();
            } else {
                player.stop(clip_id);
            }
            release.notify_one();
            let played = playing.await.unwrap();

            assert!(
                played.is_ok(),
                "a stopped play is not a failure: {played:?}"
            );
            assert_eq!(
                (opened.load(Ordering::SeqCst), started_count(&events)),
                (0, 0),
                "stop_all={stop_all}: a clip stopped before it started still reached the device"
            );
        }
    }

    #[tokio::test]
    async fn a_stop_at_a_loop_cycle_boundary_stops_the_pass_that_was_starting() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
        let mut clip = make_stored_clip(clip_id, tmp.path().to_path_buf());
        clip.loop_playback = true;
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let (overlay, mut commands) = overlay_page(Some(DeliveryGate {
            at_delivery: 2,
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        }));
        let (player, _events) = player_over(Arc::new(CountingFactory::new().0), clip);
        player.install_route(ClipRoute::new(AudioRoute::Overlay, Some(overlay)));

        player.play(clip_id, None).await.unwrap();
        tokio::time::timeout(EVENT_DEADLINE, entered.notified())
            .await
            .unwrap();
        player.stop(clip_id);
        release.notify_one();

        wait_for_stop_of(&mut commands, "clip-2").await;
    }

    #[derive(Default)]
    struct MemorySettings(Mutex<HashMap<String, String>>);

    #[async_trait]
    impl SettingsRepo for MemorySettings {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }

        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(key).is_some())
        }

        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    const SUB_ACTION_VOLUME: f32 = 0.25;

    fn idle_player() -> SoundboardPlayer {
        SoundboardPlayer::with_settings(
            Arc::new(CountingFactory::new().0),
            Arc::new(RecordingEventSink::new().0),
            unadopted_library(None),
            SoundboardSettingsHandle::default(),
        )
    }

    #[tokio::test]
    async fn the_set_master_volume_sub_action_persists_through_the_installed_settings_store() {
        let store = Arc::new(MemorySettings::default());
        let player = idle_player();
        player.install_settings_store(Arc::clone(&store) as Arc<dyn SettingsRepo>);

        SoundPlayer::set_master_volume(&player, SUB_ACTION_VOLUME)
            .await
            .unwrap();

        let persisted = forge_storage::soundboard_master_volume(store.as_ref())
            .await
            .unwrap();
        assert!(
            (persisted - SUB_ACTION_VOLUME).abs() < f32::EPSILON,
            "the sub-action volume did not reach the store: {persisted}"
        );
    }

    #[tokio::test]
    async fn a_master_volume_above_ceiling_persists_the_same_clamped_gain_the_live_setting_holds() {
        let store = Arc::new(MemorySettings::default());
        let player = idle_player();
        player.install_settings_store(Arc::clone(&store) as Arc<dyn SettingsRepo>);

        // A +6 dB step now overshoots the 0 dB ceiling; it must clamp to unity
        // (1.0), and restart must not revert to a stale, unclamped value.
        SoundPlayer::set_master_volume(&player, 10.0).await.unwrap();

        let persisted = forge_storage::soundboard_master_volume(store.as_ref())
            .await
            .unwrap();
        let live = player.settings_handle().load().master_volume;
        assert_eq!(
            (persisted, live),
            (1.0, 1.0),
            "persisted and live master volume must both clamp to unity"
        );
    }

    #[tokio::test]
    async fn a_sub_action_volume_survives_a_later_edit_of_another_soundboard_setting() {
        let player = idle_player();

        SoundPlayer::set_master_volume(&player, SUB_ACTION_VOLUME)
            .await
            .unwrap();
        player.update_settings(|settings| settings.also_headphones = true);

        let volume = player.settings_handle().load().master_volume;
        assert!(
            (volume - SUB_ACTION_VOLUME).abs() < f32::EPSILON,
            "editing another setting undid the sub-action volume: {volume}"
        );
    }
    struct GatedToggle {
        player: Arc<SoundboardPlayer>,
        clip_id: ClipId,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        opened: Arc<std::sync::atomic::AtomicUsize>,
        _source: tempfile::NamedTempFile,
    }

    impl GatedToggle {
        fn new() -> Self {
            let clip_id = ClipId::new();
            let source = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let factory = GatedFactory {
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
                opened: Arc::clone(&opened),
            };
            let (player, _events) = player_over(
                Arc::new(factory),
                make_stored_clip(clip_id, source.path().to_path_buf()),
            );
            Self {
                player: Arc::new(player),
                clip_id,
                entered,
                release,
                opened,
                _source: source,
            }
        }

        async fn start_held_at_the_device(
            &self,
        ) -> tokio::task::JoinHandle<Result<ClipToggle, SoundboardError>> {
            let starting = tokio::spawn({
                let player = Arc::clone(&self.player);
                let clip_id = self.clip_id;
                async move { player.toggle(clip_id, None).await }
            });
            tokio::time::timeout(EVENT_DEADLINE, self.entered.notified())
                .await
                .unwrap();
            starting
        }

        async fn finish(
            &self,
            starting: tokio::task::JoinHandle<Result<ClipToggle, SoundboardError>>,
        ) -> ClipToggle {
            self.release.notify_one();
            tokio::time::timeout(EVENT_DEADLINE, starting)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        }
    }

    #[tokio::test]
    async fn a_toggle_of_an_idle_clip_starts_it() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
        let (factory, played, _) = CountingFactory::new();
        let (player, _events) = player_over(
            Arc::new(factory),
            make_stored_clip(clip_id, tmp.path().to_path_buf()),
        );

        let toggled = player.toggle(clip_id, None).await.unwrap();

        assert_eq!((toggled, *played.lock().unwrap()), (ClipToggle::Started, 1));
    }

    #[tokio::test]
    async fn a_second_toggle_while_the_clip_is_starting_stops_it_before_it_reaches_the_device() {
        let rig = GatedToggle::new();
        let starting = rig.start_held_at_the_device().await;

        let second = rig.player.toggle(rig.clip_id, None).await.unwrap();
        let first = rig.finish(starting).await;

        assert_eq!(
            (first, second, rig.opened.load(Ordering::SeqCst)),
            (ClipToggle::Started, ClipToggle::Stopped, 0)
        );
    }

    #[tokio::test]
    async fn a_toggle_after_the_clip_finished_starts_it_again() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
        let (factory, played, _) = CountingFactory::new();
        let (player, events) = player_over(
            Arc::new(factory),
            make_stored_clip(clip_id, tmp.path().to_path_buf()),
        );
        player.toggle(clip_id, None).await.unwrap();
        wait_for_event(&events, |event| {
            matches!(event, AudioEvent::PlaybackFinished { .. })
        })
        .await;

        let again = player.toggle(clip_id, None).await.unwrap();

        assert_eq!((again, *played.lock().unwrap()), (ClipToggle::Started, 2));
    }

    #[tokio::test]
    async fn a_toggle_with_the_soundboard_disabled_leaves_an_idle_clip_silent() {
        let clip_id = ClipId::new();
        let tmp = make_wav_tempfile(22_050, 1, SHORT_CLIP_FRAMES);
        let (factory, played, _) = CountingFactory::new();
        let (player, _events) = player_over(
            Arc::new(factory),
            make_stored_clip(clip_id, tmp.path().to_path_buf()),
        );
        player.update_settings(|settings| settings.enabled = false);

        let toggled = player.toggle(clip_id, None).await.unwrap();

        assert_eq!((toggled, *played.lock().unwrap()), (ClipToggle::Ignored, 0));
    }

    #[tokio::test]
    async fn a_toggle_with_the_soundboard_disabled_still_stops_a_clip_already_starting() {
        let rig = GatedToggle::new();
        let starting = rig.start_held_at_the_device().await;
        rig.player
            .update_settings(|settings| settings.enabled = false);

        let second = rig.player.toggle(rig.clip_id, None).await.unwrap();
        rig.finish(starting).await;

        assert_eq!(
            (second, rig.opened.load(Ordering::SeqCst)),
            (ClipToggle::Stopped, 0)
        );
    }

    #[tokio::test]
    async fn a_toggle_of_a_clip_the_library_does_not_hold_reports_it_not_found() {
        let (player, _events) = player_over(
            Arc::new(CountingFactory::new().0),
            make_stored_clip(ClipId::new(), PathBuf::from("/clips/other.wav")),
        );

        let outcome = player.toggle(ClipId::new(), None).await;

        assert!(
            matches!(outcome, Err(SoundboardError::ClipNotFound(_))),
            "got {outcome:?}"
        );
    }
}
