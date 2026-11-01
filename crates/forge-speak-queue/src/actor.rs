use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

const PLAYBACK_PROGRESS_TICK: Duration = Duration::from_secs(1);
const PLAYBACK_START_BOUND: Duration = Duration::from_secs(5);

use forge_audio::{AudioError, AudioSink};
use forge_net::TaskOwner;
use forge_speech_types::{EngineId, TtsVoice, VoiceId};
use forge_speech_types::{LanguageCode, PcmBuffer};
use forge_tts_core::{SpeechText, SynthesisRequest, TtsRegistry, engine_display_name};
use forge_tts_pipeline::{DetectionOutcome, LanguageDetector, PipelineResult, spoken_words};
use forge_types::{MessageRef, Shared};

use crate::catalog::{CATALOG_UPDATE_CAPACITY, CatalogUpdate, VoiceCatalogState};
use crate::detector_builds::{DetectorBuild, DetectorBuilds};
use crate::emitter::{
    DetectedLanguage, EnqueuePreview, Outcome, ResolvedPlayback, SpeakEmitter, pipeline_skip,
};
use crate::queues::PendingQueues;
use crate::vocabulary::messages;
use crate::vocabulary::{FailureReason, HoldReason, RejectReason, SkipReason};
use crate::voice::{ResolveResult, VoiceAliasResolver, voice_speaks_language};

use crate::{
    PipelineConfigHandle, Priority, QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakRequest,
    TargetedLegs, UNITY_GAIN, clamp_gain,
};

pub(crate) struct ActorDeps {
    pub(crate) registry: Shared<TtsRegistry>,
    pub(crate) resolver: Shared<VoiceAliasResolver>,
    pub(crate) pipeline: PipelineConfigHandle,
    pub(crate) audio_sink: Arc<dyn AudioSink>,
    pub(crate) event_bus: Arc<dyn forge_events::EventPublisher>,
    pub(crate) disabled_engines: HashSet<EngineId>,
    pub(crate) engine_gains: HashMap<EngineId, f32>,
    pub(crate) tasks: TaskOwner,
}

fn update_registry(registry: &Shared<TtsRegistry>, change: impl FnOnce(&mut TtsRegistry)) {
    let mut next = (*registry.load()).clone();
    change(&mut next);
    registry.store(next);
}

fn update_resolver(
    resolver: &Shared<VoiceAliasResolver>,
    change: impl FnOnce(&mut VoiceAliasResolver),
) {
    let mut next = (*resolver.load()).clone();
    change(&mut next);
    resolver.store(next);
}

struct SynthTaskResult {
    request: SpeakRequest,
    outcome: SynthOutcome,
}

impl SynthTaskResult {
    fn new(request: SpeakRequest, outcome: SynthOutcome) -> Self {
        Self { request, outcome }
    }
}

struct ReadyClip {
    request_id: RequestId,
    request: SpeakRequest,
    pcm: PcmBuffer,
    voice_id: VoiceId,
    engine_id: EngineId,
    language: Option<DetectedLanguage>,
}

struct PlayingClip {
    request_id: RequestId,
    request: SpeakRequest,
    playback: forge_audio::ControlledPlayback,
    elapsed_secs: u32,
}

type PlaybackStart = tokio::task::JoinHandle<Result<forge_audio::ControlledPlayback, Outcome>>;

struct StartingClip {
    request_id: RequestId,
    request: SpeakRequest,
    start: PlaybackStart,
}

enum CurrentPlayback {
    Starting(StartingClip),
    Playing(PlayingClip),
    Held(ReadyClip),
}

fn is_active(current: &Option<CurrentPlayback>) -> bool {
    matches!(
        current,
        Some(CurrentPlayback::Starting(_) | CurrentPlayback::Playing(_))
    )
}

enum ActiveStep {
    Started(Result<Result<forge_audio::ControlledPlayback, Outcome>, tokio::task::JoinError>),
    Finished(Result<(), AudioError>),
}

async fn poll_active(current: &mut Option<CurrentPlayback>) -> ActiveStep {
    match current {
        Some(CurrentPlayback::Starting(clip)) => ActiveStep::Started((&mut clip.start).await),
        Some(CurrentPlayback::Playing(clip)) => ActiveStep::Finished((&mut clip.playback).await),
        _ => std::future::pending().await,
    }
}

fn is_held(current: &Option<CurrentPlayback>) -> bool {
    matches!(current, Some(CurrentPlayback::Held(_)))
}

async fn tick_progress(ticker: &mut Option<tokio::time::Interval>) {
    match ticker {
        Some(t) => {
            t.tick().await;
        }
        None => std::future::pending().await,
    }
}

enum SynthOutcome {
    Speak {
        pcm: PcmBuffer,
        voice_id: VoiceId,
        engine_id: EngineId,
        language: Option<DetectedLanguage>,
    },
    Skipped {
        reason: SkipReason,
        detail: Option<String>,
    },
    Failed {
        reason: FailureReason,
        message: MessageRef,
    },
}

struct SynthTaskDeps {
    resolver: Arc<VoiceAliasResolver>,
    pipeline: PipelineConfigHandle,
    registry: Arc<forge_tts_core::TtsRegistry>,
    voice_catalog: Arc<Vec<TtsVoice>>,
    detector: Option<Arc<LanguageDetector>>,
}

async fn run_synthesis(
    req: SpeakRequest,
    deps: SynthTaskDeps,
    recent_messages: Vec<String>,
) -> SynthTaskResult {
    let configured = deps.pipeline.load();
    let pipeline_cfg = if req.message_emotes.is_empty() {
        configured
    } else {
        Arc::new(configured.with_message_emotes(&req.message_emotes))
    };

    let reward_stripped;
    let text_for_pipeline: &str = if req.is_reward && pipeline_cfg.strip_reward_emotes {
        reward_stripped =
            forge_tts_pipeline::strip_emote_tokens(&req.text, &pipeline_cfg.emote_tokens);
        &reward_stripped
    } else {
        &req.text
    };
    let context = forge_tts_pipeline::PipelineContext {
        viewer_name: &req.viewer_name,
        recent_messages: &recent_messages,
        speech_language: None,
    };
    let processed =
        match forge_tts_pipeline::process_speech(text_for_pipeline, &pipeline_cfg, &context) {
            Ok(processed) => processed,
            Err(reason) => {
                let (reason, detail) = pipeline_skip(&reason);
                return SynthTaskResult::new(req, SynthOutcome::Skipped { reason, detail });
            }
        };

    let detected = detect_language(&deps, &pipeline_cfg, processed.language_sample()).await;
    let text_to_speak = processed.spoken;

    let resolve_result = {
        deps.resolver
            .resolve(&req, detected.as_ref().map(|d| d.code), &deps.voice_catalog)
    };

    let (voice_id, engine_id, pitch, rate) = match resolve_result {
        ResolveResult::Speak {
            voice_id,
            engine_id,
            pitch,
            rate,
        } => (voice_id, engine_id, pitch, rate),
        ResolveResult::Skip { reason } => {
            return SynthTaskResult::new(
                req,
                SynthOutcome::Skipped {
                    reason: reason.into(),
                    detail: None,
                },
            );
        }
    };

    let text_to_speak = match voiced_text(
        text_to_speak,
        text_for_pipeline,
        &pipeline_cfg,
        &context,
        voice_language(&deps.voice_catalog, &voice_id, &engine_id),
    ) {
        PipelineResult::Speak(text) => text,
        PipelineResult::Skip { reason } => {
            let (reason, detail) = pipeline_skip(&reason);
            return SynthTaskResult::new(req, SynthOutcome::Skipped { reason, detail });
        }
    };

    let slot = deps.registry.slot(&engine_id);
    let slot = match slot {
        Some(slot) => slot,
        None => {
            let message = messages::engine_not_registered(&engine_id);
            return SynthTaskResult::new(req, engine_failure(message));
        }
    };

    let engine = match slot.engine().await {
        Ok(e) => e,
        Err(e) => {
            return SynthTaskResult::new(req, engine_failure(messages::tts_failure(&e)));
        }
    };

    let synthesis_req = SynthesisRequest::new(
        SpeechText::plain(text_to_speak),
        voice_id.clone(),
        pitch,
        rate,
    );

    let outcome = match engine.synthesize(synthesis_req).await {
        Ok(pcm) => SynthOutcome::Speak {
            pcm,
            voice_id,
            engine_id,
            language: detected,
        },
        Err(e) => engine_failure(messages::tts_failure(&e)),
    };
    SynthTaskResult::new(req, outcome)
}

fn voice_language(
    catalog: &[TtsVoice],
    voice_id: &VoiceId,
    engine_id: &EngineId,
) -> Option<LanguageCode> {
    catalog
        .iter()
        .find(|voice| &voice.id == voice_id && &voice.engine_id == engine_id)
        .and_then(|voice| LanguageCode::from_locale(&voice.locale))
}

fn voiced_text(
    neutral: String,
    input: &str,
    config: &forge_tts_pipeline::PipelineConfig,
    context: &forge_tts_pipeline::PipelineContext<'_>,
    language: Option<LanguageCode>,
) -> PipelineResult {
    if spoken_words(language) == spoken_words(None) {
        return PipelineResult::Speak(neutral);
    }
    let voiced = forge_tts_pipeline::PipelineContext {
        speech_language: language,
        ..*context
    };
    forge_tts_pipeline::process(input, config, &voiced)
}

fn engine_failure(message: MessageRef) -> SynthOutcome {
    SynthOutcome::Failed {
        reason: FailureReason::EngineError,
        message,
    }
}

async fn run_synthesis_bounded(
    req: SpeakRequest,
    deps: SynthTaskDeps,
    recent_messages: Vec<String>,
    timeout: Duration,
) -> SynthTaskResult {
    let fallback = req.clone();
    match tokio::time::timeout(timeout, run_synthesis(req, deps, recent_messages)).await {
        Ok(result) => result,
        Err(_) => SynthTaskResult::new(
            fallback,
            SynthOutcome::Failed {
                reason: FailureReason::EngineTimeout,
                message: messages::synthesis_timed_out(
                    u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
                ),
            },
        ),
    }
}

async fn detect_language(
    deps: &SynthTaskDeps,
    pipeline_cfg: &forge_tts_pipeline::PipelineConfig,
    sample: &str,
) -> Option<DetectedLanguage> {
    if !pipeline_cfg.output.language_aware_voice {
        return None;
    }
    let detector = deps.detector.clone()?;
    let sample = sample.to_owned();

    let outcome = match tokio::task::spawn_blocking(move || detector.detect(&sample)).await {
        Ok(outcome) => outcome,
        Err(e) => {
            tracing::warn!(error = %e, "language detection failed");
            return None;
        }
    };
    let DetectionOutcome::Detected {
        language,
        confidence,
    } = outcome
    else {
        return None;
    };

    deps.voice_catalog
        .iter()
        .any(|voice| voice_speaks_language(voice, language))
        .then_some(DetectedLanguage {
            code: language,
            confidence,
        })
}

pub(crate) struct ActorHandles {
    pub(crate) depth: Arc<AtomicUsize>,
    pub(crate) voices: Shared<Vec<TtsVoice>>,
    pub(crate) disabled_engines: Shared<HashSet<EngineId>>,
    pub(crate) master_volume_bits: Arc<AtomicU32>,
    pub(crate) engine_gains: Shared<HashMap<EngineId, f32>>,
    pub(crate) targeted_legs: TargetedLegs,
}

struct ActorInbox {
    commands: tokio::sync::mpsc::Receiver<SpeakCommand>,
    synth_results: tokio::sync::mpsc::Receiver<SynthTaskResult>,
    catalog_updates: tokio::sync::mpsc::Receiver<CatalogUpdate>,
    detector_builds: tokio::sync::mpsc::Receiver<DetectorBuild>,
}

struct QueueActor {
    config: QueueConfig,
    deps: ActorDeps,
    handles: ActorHandles,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    queues: PendingQueues,
    paused: bool,
    voicegate_active: bool,
    active_request_id: Option<RequestId>,
    last_successful: Option<SpeakRequest>,
    current_playback: Option<CurrentPlayback>,
    progress_ticker: Option<tokio::time::Interval>,
    recent_messages: VecDeque<String>,
    synthesis_task: Option<(RequestId, tokio::task::AbortHandle)>,
    synth_tx: tokio::sync::mpsc::Sender<SynthTaskResult>,
    catalog_tx: tokio::sync::mpsc::Sender<CatalogUpdate>,
    catalog: VoiceCatalogState,
    task_deps: SynthTaskDeps,
    language_aware: bool,
    detector_builds: DetectorBuilds,
}

pub(crate) async fn run_actor(
    config: QueueConfig,
    deps: ActorDeps,
    handles: ActorHandles,
    commands: tokio::sync::mpsc::Receiver<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    detector_ready: tokio::sync::watch::Sender<bool>,
) {
    let (mut actor, mut inbox) =
        QueueActor::new(config, deps, handles, commands, event_tx, detector_ready);
    actor.run(&mut inbox).await;
}

impl QueueActor {
    fn new(
        config: QueueConfig,
        deps: ActorDeps,
        handles: ActorHandles,
        commands: tokio::sync::mpsc::Receiver<SpeakCommand>,
        event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
        detector_ready: tokio::sync::watch::Sender<bool>,
    ) -> (Self, ActorInbox) {
        let (synth_tx, synth_results) = tokio::sync::mpsc::channel::<SynthTaskResult>(8);
        let (catalog_tx, catalog_updates) =
            tokio::sync::mpsc::channel::<CatalogUpdate>(CATALOG_UPDATE_CAPACITY);
        let (detector_tx, detector_builds_rx) = tokio::sync::mpsc::channel::<DetectorBuild>(1);
        let detector_builds = DetectorBuilds::new(detector_tx, detector_ready, deps.tasks.clone());

        update_registry(&deps.registry, |registry| {
            for engine_id in &deps.disabled_engines {
                registry.invalidate(engine_id);
            }
        });
        handles
            .disabled_engines
            .store(deps.disabled_engines.clone());
        handles.engine_gains.store(deps.engine_gains.clone());
        let mut catalog = VoiceCatalogState::owned_by(deps.tasks.clone());
        let initial_catalog =
            catalog.rebuild(&deps.registry.load(), &deps.disabled_engines, &catalog_tx);
        handles.voices.store_arc(initial_catalog.clone());
        let task_deps = SynthTaskDeps {
            resolver: deps.resolver.load(),
            pipeline: deps.pipeline.clone(),
            registry: deps.registry.load(),
            voice_catalog: initial_catalog,
            detector: None,
        };
        let language_aware = deps.pipeline.load().output.language_aware_voice;

        let actor = Self {
            config,
            deps,
            handles,
            event_tx,
            queues: PendingQueues::default(),
            paused: false,
            voicegate_active: false,
            active_request_id: None,
            last_successful: None,
            current_playback: None,
            progress_ticker: None,
            recent_messages: VecDeque::new(),
            synthesis_task: None,
            synth_tx,
            catalog_tx,
            catalog,
            task_deps,
            language_aware,
            detector_builds,
        };
        let inbox = ActorInbox {
            commands,
            synth_results,
            catalog_updates,
            detector_builds: detector_builds_rx,
        };
        (actor, inbox)
    }

    fn emitter(&self) -> SpeakEmitter<'_> {
        SpeakEmitter::new(&self.event_tx, self.deps.event_bus.as_ref())
    }

    async fn run(&mut self, inbox: &mut ActorInbox) {
        loop {
            self.prepare_cycle();
            tokio::select! {
                command = inbox.commands.recv() => {
                    match command {
                        None => break,
                        Some(command) => {
                            if self.handle_command(command).is_break() {
                                break;
                            }
                        }
                    }
                }
                result = inbox.synth_results.recv() => {
                    if let Some(result) = result {
                        self.handle_synth_result(result);
                    }
                }
                update = inbox.catalog_updates.recv() => {
                    if let Some(merged) = update.and_then(|update| self.catalog.apply(update)) {
                        self.publish_catalog(merged);
                    }
                }
                build = inbox.detector_builds.recv() => {
                    if let Some(build) = build {
                        self.adopt_detector(build);
                    }
                }
                () = tick_progress(&mut self.progress_ticker), if self.progress_ticker.is_some() => {
                    self.on_progress_tick();
                }
                step = poll_active(&mut self.current_playback), if is_active(&self.current_playback) => {
                    self.on_active_step(step).await;
                }
            }
        }
        self.abort_synthesis();
    }

    fn prepare_cycle(&mut self) {
        self.handles
            .depth
            .store(self.queues.len(), Ordering::Relaxed);
        self.reap_stale_synthesis();
        self.follow_language_setting();
        self.resume_held_clip();
        self.start_next_synthesis();
    }

    fn abort_synthesis(&mut self) {
        if let Some((_, task)) = self.synthesis_task.take() {
            task.abort();
        }
    }

    fn reap_stale_synthesis(&mut self) {
        if let Some((request_id, _)) = &self.synthesis_task
            && self.active_request_id.as_ref() != Some(request_id)
        {
            self.abort_synthesis();
        }
    }

    fn follow_language_setting(&mut self) {
        let language_aware_now = self.deps.pipeline.load().output.language_aware_voice;
        if language_aware_now == self.language_aware {
            return;
        }
        self.language_aware = language_aware_now;
        if self.language_aware {
            self.detector_builds.rebuild(&self.task_deps.voice_catalog);
        } else {
            self.detector_builds.invalidate();
            self.task_deps.detector = None;
        }
    }

    fn resume_held_clip(&mut self) {
        if !self.paused
            && is_held(&self.current_playback)
            && let Some(CurrentPlayback::Held(clip)) = self.current_playback.take()
        {
            self.start_clip(clip);
        }
    }

    fn start_next_synthesis(&mut self) {
        let ready = self.active_request_id.is_none()
            && !self.paused
            && !self.voicegate_active
            && self.queues.next().is_some_and(|request| {
                let resolver = self.deps.resolver.load();
                self.catalog.serves(
                    request,
                    resolver.active_alias_engine(&request.viewer_id, &request.viewer_name),
                )
            });
        if !ready {
            return;
        }
        let Some(request) = self.queues.pop_next() else {
            return;
        };
        let queue_len = self.queues.len();
        let emit = self.emitter();
        emit.feed(self.queues.changed_event());
        emit.started(&request, None, queue_len);
        self.active_request_id = Some(request.request_id.clone());
        let tx = self.synth_tx.clone();
        let task_deps = SynthTaskDeps {
            resolver: self.deps.resolver.load(),
            pipeline: self.task_deps.pipeline.clone(),
            registry: self.deps.registry.load(),
            voice_catalog: self.task_deps.voice_catalog.clone(),
            detector: self.task_deps.detector.clone(),
        };
        let window = self.config.repeat_window.get();
        while self.recent_messages.len() > window {
            self.recent_messages.pop_front();
        }
        let recent_snapshot: Vec<String> = self.recent_messages.iter().cloned().collect();
        while self.recent_messages.len() >= window {
            self.recent_messages.pop_front();
        }
        self.recent_messages.push_back(request.text.clone());
        let timeout = self.config.timeout_per_item;
        let request_id = request.request_id.clone();
        let task = self.deps.tasks.spawn_until_stopped(async move {
            let result = run_synthesis_bounded(request, task_deps, recent_snapshot, timeout).await;
            if tx.send(result).await.is_err() {
                tracing::debug!("speak queue stopped before the synthesis result arrived");
            }
        });
        self.synthesis_task = Some((request_id, task));
    }

    fn adopt_detector(&mut self, build: DetectorBuild) {
        if self.detector_builds.is_current(build.generation) {
            self.task_deps.detector = build.detector.filter(|_| self.language_aware);
            self.detector_builds
                .settle(self.task_deps.detector.is_some());
        }
    }

    fn on_progress_tick(&mut self) {
        if self.paused {
            return;
        }
        if let Some(CurrentPlayback::Playing(clip)) = self.current_playback.as_mut() {
            clip.elapsed_secs += 1;
            let event = SpeakEvent::Progress {
                request_id: clip.request_id.clone(),
                elapsed_secs: clip.elapsed_secs,
            };
            self.emitter().feed(event);
        }
    }

    async fn on_active_step(&mut self, step: ActiveStep) {
        match (step, self.current_playback.take()) {
            (ActiveStep::Started(started), Some(CurrentPlayback::Starting(clip))) => {
                self.on_playback_started(started, clip).await;
            }
            (ActiveStep::Finished(result), Some(CurrentPlayback::Playing(clip))) => {
                self.progress_ticker = None;
                self.finish_playback(result, clip);
            }
            (_, unchanged) => self.current_playback = unchanged,
        }
    }

    async fn on_playback_started(
        &mut self,
        started: Result<Result<forge_audio::ControlledPlayback, Outcome>, tokio::task::JoinError>,
        clip: StartingClip,
    ) {
        let started =
            started.unwrap_or_else(|e| Err(device_failure(&AudioError::JoinFailed(e.to_string()))));
        match started {
            Ok(playback) => {
                if self.paused {
                    playback.pause();
                }
                let mut ticker = tokio::time::interval(PLAYBACK_PROGRESS_TICK);
                ticker.tick().await;
                self.progress_ticker = Some(ticker);
                self.current_playback = Some(CurrentPlayback::Playing(PlayingClip {
                    request_id: clip.request_id,
                    request: clip.request,
                    playback,
                    elapsed_secs: 0,
                }));
            }
            Err(failure) => {
                self.active_request_id = None;
                tracing::warn!(request_id = %clip.request_id.0, ?failure, "audio playback failed to start");
                self.emitter().outcome(&clip.request, failure);
            }
        }
    }

    fn finish_playback(&mut self, result: Result<(), AudioError>, clip: PlayingClip) {
        self.active_request_id = None;
        match result {
            Ok(()) => {
                self.emitter().outcome(&clip.request, Outcome::Finished);
                self.last_successful = Some(clip.request);
            }
            Err(e) => {
                tracing::warn!(request_id = %clip.request_id.0, error = %e, "audio playback failed");
                self.emitter().outcome(&clip.request, device_failure(&e));
            }
        }
    }

    fn handle_synth_result(&mut self, result: SynthTaskResult) {
        if self.active_request_id.as_ref() != Some(&result.request.request_id) {
            return;
        }
        match result.outcome {
            SynthOutcome::Skipped { reason, detail } => {
                self.active_request_id = None;
                tracing::debug!(request_id = %result.request.request_id.0, ?reason, "speak skipped");
                self.emitter()
                    .outcome(&result.request, Outcome::Skipped { reason, detail });
            }
            SynthOutcome::Failed { reason, message } => {
                self.active_request_id = None;
                tracing::warn!(request_id = %result.request.request_id.0, code = %message.code(), detail = message.detail().map(|detail| detail.as_str()), "speak failed");
                self.emitter()
                    .outcome(&result.request, Outcome::Failed { reason, message });
            }
            SynthOutcome::Speak {
                mut pcm,
                voice_id,
                engine_id,
                language,
            } => {
                if let Some(cap_secs) = self.deps.pipeline.load().output.max_duration_secs {
                    pcm.truncate_to_secs(cap_secs);
                }
                let clip = ReadyClip {
                    request_id: result.request.request_id.clone(),
                    request: result.request,
                    pcm,
                    voice_id,
                    engine_id,
                    language,
                };
                if self.paused {
                    self.current_playback = Some(CurrentPlayback::Held(clip));
                } else {
                    self.start_clip(clip);
                }
            }
        }
    }

    fn start_clip(&mut self, clip: ReadyClip) {
        let ReadyClip {
            request_id,
            request,
            mut pcm,
            voice_id,
            engine_id,
            language,
        } = clip;
        let duration_secs = u32::try_from(pcm.duration_ms() / 1000).unwrap_or(u32::MAX);
        let emit = SpeakEmitter::new(&self.event_tx, self.deps.event_bus.as_ref());
        emit.started(
            &request,
            Some(ResolvedPlayback {
                voice_id: &voice_id,
                engine_id: &engine_id,
                duration_secs,
                language: language.as_ref(),
            }),
            self.queues.len(),
        );
        let engine_gain = self
            .handles
            .engine_gains
            .load()
            .get(&engine_id)
            .copied()
            .unwrap_or(UNITY_GAIN);
        pcm.apply_gain(self.config.master_volume * engine_gain);
        match sink_for(&request, &self.deps, &self.handles.targeted_legs) {
            Ok(sink) => {
                let start = tokio::spawn(async move {
                    match tokio::time::timeout(PLAYBACK_START_BOUND, sink.play_controlled(pcm))
                        .await
                    {
                        Ok(started) => started.map_err(|e| device_failure(&e)),
                        Err(_) => Err(Outcome::Failed {
                            reason: FailureReason::DeviceError,
                            message: messages::audio_start_timed_out(
                                PLAYBACK_START_BOUND.as_secs(),
                            ),
                        }),
                    }
                });
                self.current_playback = Some(CurrentPlayback::Starting(StartingClip {
                    request_id,
                    request,
                    start,
                }));
            }
            Err(e) => {
                self.active_request_id = None;
                tracing::warn!(request_id = %request_id.0, error = %e, "audio playback failed to start");
                emit.outcome(&request, device_failure(&e));
            }
        }
    }

    fn stop_active(&mut self, reason: SkipReason) {
        let Some(request_id) = self.active_request_id.take() else {
            return;
        };
        let stopped_request = match self.current_playback.take() {
            Some(CurrentPlayback::Playing(current)) => {
                current.playback.stop();
                Some(current.request)
            }
            Some(CurrentPlayback::Starting(clip)) => {
                stop_when_started(&self.deps.tasks, clip.start);
                Some(clip.request)
            }
            Some(CurrentPlayback::Held(held)) => Some(held.request),
            None => None,
        };
        self.progress_ticker = None;
        let emit = self.emitter();
        match stopped_request {
            Some(request) => emit.outcome(
                &request,
                Outcome::Skipped {
                    reason,
                    detail: None,
                },
            ),
            None => emit.skipped_in_flight(request_id, reason),
        }
    }

    fn handle_command(&mut self, command: SpeakCommand) -> ControlFlow<()> {
        match command {
            SpeakCommand::Shutdown => return ControlFlow::Break(()),
            SpeakCommand::Enqueue(request) => self.admit_request(request),
            SpeakCommand::Skip => self.stop_active(SkipReason::UserSkip),
            SpeakCommand::PlayNow(request_id) => {
                if self.queues.promote(&request_id) {
                    self.stop_active(SkipReason::Promoted);
                    self.emitter().feed(self.queues.changed_event());
                }
            }
            SpeakCommand::RemoveQueued(request_id) => self.remove_queued(&request_id),
            SpeakCommand::Cancel(request_id) => {
                if self.active_request_id.as_ref() == Some(&request_id) {
                    self.stop_active(SkipReason::Cancelled);
                } else {
                    self.remove_queued(&request_id);
                }
            }
            SpeakCommand::Reorder { request_id, before } => {
                self.reorder(&request_id, before);
            }
            SpeakCommand::Clear => {
                self.stop_active(SkipReason::Cleared);
                self.clear_pending(false);
            }
            SpeakCommand::ClearPending => self.clear_pending(true),
            SpeakCommand::SetAlias(alias) => {
                update_resolver(&self.deps.resolver, |resolver| resolver.set_alias(alias));
            }
            SpeakCommand::SwitchAlias {
                viewer_id,
                engine_id,
                voice_id,
            } => {
                update_resolver(&self.deps.resolver, |resolver| {
                    resolver.switch_alias(&viewer_id, engine_id, voice_id);
                });
            }
            SpeakCommand::SetStrategy(strategy) => {
                update_resolver(&self.deps.resolver, |resolver| {
                    resolver.set_strategy(strategy);
                });
            }
            SpeakCommand::RemoveAlias(id) => {
                update_resolver(&self.deps.resolver, |resolver| resolver.remove_alias(&id));
            }
            SpeakCommand::SetVolume(volume) => {
                self.config.master_volume = clamp_gain(volume);
                self.handles
                    .master_volume_bits
                    .store(self.config.master_volume.to_bits(), Ordering::Relaxed);
            }
            SpeakCommand::SetRepeatWindow(window) => self.config.repeat_window = window,
            SpeakCommand::SetEngineParams(engine_id, defaults, gain) => {
                update_resolver(&self.deps.resolver, |resolver| {
                    resolver.set_engine_default(engine_id.clone(), defaults);
                });
                let mut next = (*self.handles.engine_gains.load()).clone();
                next.insert(engine_id, clamp_gain(gain));
                self.handles.engine_gains.store(next);
            }
            SpeakCommand::RefreshVoiceCatalog => {
                update_registry(&self.deps.registry, TtsRegistry::invalidate_all);
                let disabled = self.handles.disabled_engines.load();
                self.rebuild_catalog(&disabled);
            }
            SpeakCommand::RegisterEngine(registration) => {
                update_registry(&self.deps.registry, |registry| {
                    registry.register(registration.0);
                });
                let disabled = self.handles.disabled_engines.load();
                self.rebuild_catalog(&disabled);
            }
            SpeakCommand::SetEngineEnabled(engine_id, enabled) => {
                self.set_engine_enabled(&engine_id, enabled);
            }
            SpeakCommand::Pause => {
                self.paused = true;
                if let Some(CurrentPlayback::Playing(clip)) = self.current_playback.as_ref() {
                    clip.playback.pause();
                }
                self.emitter().paused(HoldReason::User);
            }
            SpeakCommand::Resume => {
                self.paused = false;
                if let Some(CurrentPlayback::Playing(clip)) = self.current_playback.as_ref() {
                    clip.playback.resume();
                }
                self.emitter().resumed(HoldReason::User);
            }
            SpeakCommand::Replay => {
                if let Some(last) = &self.last_successful {
                    let mut replay = last.clone();
                    replay.request_id = RequestId::new();
                    replay.priority = Priority::High;
                    self.admit_request(replay);
                }
            }
            SpeakCommand::VoiceGateActivated => {
                self.voicegate_active = true;
                self.emitter().paused(HoldReason::VoiceGate);
            }
            SpeakCommand::VoiceGateDeactivated => {
                self.voicegate_active = false;
                self.emitter().resumed(HoldReason::VoiceGate);
            }
        }
        ControlFlow::Continue(())
    }

    fn set_engine_enabled(&mut self, engine_id: &EngineId, enabled: bool) {
        let mut next = (*self.handles.disabled_engines.load()).clone();
        let changed = if enabled {
            next.remove(engine_id)
        } else {
            next.insert(engine_id.clone())
        };
        if changed {
            update_registry(&self.deps.registry, |registry| {
                registry.invalidate(engine_id);
            });
        }
        let rebuilt = self
            .catalog
            .rebuild(&self.deps.registry.load(), &next, &self.catalog_tx);
        self.handles.disabled_engines.store(next);
        self.publish_catalog(rebuilt);
    }

    fn rebuild_catalog(&mut self, disabled: &HashSet<EngineId>) {
        let rebuilt = self
            .catalog
            .rebuild(&self.deps.registry.load(), disabled, &self.catalog_tx);
        self.publish_catalog(rebuilt);
    }

    fn publish_catalog(&mut self, merged: Arc<Vec<TtsVoice>>) {
        self.handles.voices.store_arc(merged.clone());
        if self.language_aware {
            self.detector_builds.rebuild(&merged);
        }
        self.task_deps.voice_catalog = merged;
        self.emitter().feed(SpeakEvent::VoiceCatalogChanged);
    }

    fn admit_request(&mut self, request: SpeakRequest) {
        let total = self.queues.len();
        let rejection = if total >= self.config.max_queue_len {
            tracing::debug!(request_id = %request.request_id.0, "queue full, rejecting");
            Some((RejectReason::QueueFull, self.config.max_queue_len))
        } else if self.queues.user_count(&request.viewer_id) >= self.config.per_user_limit {
            tracing::debug!(viewer_id = %request.viewer_id, "per-user limit reached");
            Some((RejectReason::PerUserLimit, self.config.per_user_limit))
        } else {
            None
        };
        let emit = SpeakEmitter::new(&self.event_tx, self.deps.event_bus.as_ref());
        if let Some((reason, limit)) = rejection {
            emit.outcome(
                &request,
                Outcome::Rejected {
                    reason,
                    limit,
                    queue_len: total,
                },
            );
            return;
        }
        let preview = enqueue_preview(&request, &self.deps, &self.task_deps.voice_catalog);
        emit.enqueued(&request, total + 1, preview);
        self.queues.push(request);
        emit.feed(self.queues.changed_event());
    }

    fn remove_queued(&mut self, request_id: &RequestId) {
        let Some(request) = self.queues.remove(request_id) else {
            return;
        };
        let emit = self.emitter();
        emit.outcome(&request, Outcome::Removed);
        emit.feed(self.queues.changed_event());
    }

    fn reorder(&mut self, request_id: &RequestId, before: Option<RequestId>) {
        let Some(moved) = self.queues.reorder(request_id, before.as_ref()) else {
            return;
        };
        let emit = self.emitter();
        emit.reordered(&moved, before);
        emit.feed(self.queues.changed_event());
    }

    fn clear_pending(&mut self, keep_current: bool) {
        let drained = self.queues.drain_all();
        let emit = self.emitter();
        for request in drained {
            emit.outcome(&request, Outcome::Removed);
        }
        emit.cleared(keep_current);
        emit.feed(self.queues.changed_event());
    }
}

fn sink_for(
    request: &SpeakRequest,
    deps: &ActorDeps,
    targeted_legs: &TargetedLegs,
) -> Result<Arc<dyn AudioSink>, AudioError> {
    let Some(target) = &request.target else {
        return Ok(Arc::clone(&deps.audio_sink));
    };
    targeted_legs
        .load()
        .as_ref()
        .as_ref()
        .map(|legs| legs.sink_for(target))
        .ok_or(AudioError::NoRoute)
}

fn device_failure(error: &AudioError) -> Outcome {
    Outcome::Failed {
        reason: FailureReason::DeviceError,
        message: messages::audio_failure(error),
    }
}

fn stop_when_started(tasks: &TaskOwner, start: PlaybackStart) {
    tasks.spawn(async move {
        if let Ok(Ok(playback)) = start.await {
            playback.stop();
        }
    });
}

fn enqueue_preview(req: &SpeakRequest, deps: &ActorDeps, catalog: &[TtsVoice]) -> EnqueuePreview {
    let spoken_secs = ((req.text.chars().count() as u32) / 15).max(1);
    let estimated_secs = match deps.pipeline.load().output.max_duration_secs {
        Some(cap_secs) => spoken_secs.min(cap_secs),
        None => spoken_secs,
    };
    let resolver = deps.resolver.load();
    let preview = match resolver.resolve(req, None, catalog) {
        ResolveResult::Speak {
            voice_id,
            engine_id,
            ..
        } => {
            let name = catalog
                .iter()
                .find(|v| v.id == voice_id)
                .map(|v| v.name.clone())
                .unwrap_or_else(|| voice_id.0.clone());
            format!("{} \u{b7} {}", engine_display_name(&engine_id.0), name)
        }
        ResolveResult::Skip { .. } => String::new(),
    };
    EnqueuePreview {
        voice_preview: preview,
        estimated_secs,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use forge_audio::{AudioError, AudioSink};
    use forge_events::Event;
    use forge_events::test_support::NullPublisher;
    use forge_speech_types::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults};

    use super::*;

    use forge_events::test_support::CapturingPublisher;

    fn neutral_pipeline() -> forge_tts_pipeline::PipelineConfig {
        forge_tts_pipeline::PipelineConfig {
            emote_tokens: forge_tts_pipeline::EmoteTokenSet::default(),
            skip_rules: forge_tts_pipeline::SkipRulesConfig::default(),
            replacement_rules: Vec::new(),
            word_blocklist: Vec::new(),
            blocklist_mode: forge_tts_pipeline::BlocklistMode::Censor,
            output: forge_tts_pipeline::OutputConfig {
                read_display_name_first: false,
                emote_treatment: forge_tts_pipeline::EmoteTreatment::Keep,
                strip_emoji: false,
                sanitize_punctuation: false,
                max_duration_secs: None,
                language_aware_voice: false,
            },
            strip_reward_emotes: false,
        }
    }

    struct SilentSink;
    #[async_trait]
    impl AudioSink for SilentSink {
        async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
            Ok(())
        }
    }

    fn minimal_deps() -> ActorDeps {
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        ActorDeps {
            registry: Shared::new(forge_tts_core::TtsRegistry::new()),
            resolver: Shared::new(resolver),
            pipeline: crate::PipelineConfigHandle::new(neutral_pipeline()),
            audio_sink: Arc::new(SilentSink),
            event_bus: Arc::new(NullPublisher),
            disabled_engines: HashSet::new(),
            engine_gains: HashMap::new(),
            tasks: TaskOwner::new(),
        }
    }

    fn request(viewer: &str, text: &str, priority: Priority) -> SpeakRequest {
        SpeakRequest {
            request_id: RequestId::new(),
            viewer_id: viewer.into(),
            viewer_name: viewer.into(),
            text: text.into(),
            priority,
            engine_override: None,
            voice_override: None,
            source_event_id: Some(forge_types::EventId::new()),
            is_reward: false,
            target: None,
            message_emotes: Default::default(),
        }
    }

    fn recording_deps() -> (ActorDeps, Arc<CapturingPublisher>) {
        let events = CapturingPublisher::shared();
        let deps = ActorDeps {
            event_bus: events.clone(),
            ..minimal_deps()
        };
        (deps, events)
    }

    struct Harness {
        actor: QueueActor,
        events: Arc<CapturingPublisher>,
        rx: tokio::sync::broadcast::Receiver<SpeakEvent>,
    }

    impl Harness {
        fn new() -> Self {
            let (deps, events) = recording_deps();
            let (tx, rx) = tokio::sync::broadcast::channel::<SpeakEvent>(32);
            let config = QueueConfig::default();
            let handles = ActorHandles {
                depth: Arc::new(AtomicUsize::new(0)),
                voices: Shared::new(Vec::new()),
                disabled_engines: Shared::new(HashSet::new()),
                master_volume_bits: Arc::new(AtomicU32::new(config.master_volume.to_bits())),
                engine_gains: Shared::new(HashMap::new()),
                targeted_legs: TargetedLegs::new(None),
            };
            let (_command_tx, commands) = tokio::sync::mpsc::channel(1);
            let (ready_tx, _ready_rx) = tokio::sync::watch::channel(false);
            let (actor, _inbox) = QueueActor::new(config, deps, handles, commands, tx, ready_tx);
            Self { actor, events, rx }
        }

        fn run(&mut self, cmd: SpeakCommand) {
            assert!(self.actor.handle_command(cmd).is_continue());
        }

        fn bus(&self, kind: &str) -> Event {
            self.events
                .events()
                .iter()
                .find(|e| e.kind == kind)
                .cloned()
                .unwrap_or_else(|| panic!("no bus event of kind {kind}"))
        }

        fn published(&self, kind: &str) -> bool {
            self.events.has_kind(kind)
        }

        fn drain_events(&mut self) -> Vec<SpeakEvent> {
            std::iter::from_fn(|| self.rx.try_recv().ok()).collect()
        }
    }

    fn stage(h: &mut Harness, items: &[(&str, Priority)]) -> Vec<RequestId> {
        items
            .iter()
            .map(|(text, priority)| {
                let req = request(text, text, *priority);
                let id = req.request_id.clone();
                h.run(SpeakCommand::Enqueue(req));
                id
            })
            .collect()
    }

    fn texts(queue: &VecDeque<SpeakRequest>) -> Vec<&str> {
        queue.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn reorder_recomputes_the_anchor_position_after_removing_the_moved_item() {
        let mut h = Harness::new();
        let ids = stage(
            &mut h,
            &[
                ("a", Priority::Normal),
                ("b", Priority::Normal),
                ("c", Priority::Normal),
            ],
        );

        h.run(SpeakCommand::Reorder {
            request_id: ids[0].clone(),
            before: Some(ids[2].clone()),
        });

        assert_eq!(texts(&h.actor.queues.normal), vec!["b", "a", "c"]);
    }

    #[test]
    fn reorder_without_an_anchor_moves_the_item_to_the_tail() {
        let mut h = Harness::new();
        let ids = stage(
            &mut h,
            &[
                ("a", Priority::Normal),
                ("b", Priority::Normal),
                ("c", Priority::Normal),
            ],
        );

        h.run(SpeakCommand::Reorder {
            request_id: ids[0].clone(),
            before: None,
        });

        assert_eq!(texts(&h.actor.queues.normal), vec!["b", "c", "a"]);
    }

    #[test]
    fn reorder_onto_a_high_anchor_moves_the_item_into_the_high_queue() {
        let mut h = Harness::new();
        let ids = stage(
            &mut h,
            &[
                ("h1", Priority::High),
                ("h2", Priority::High),
                ("n1", Priority::Normal),
                ("n2", Priority::Normal),
            ],
        );

        h.run(SpeakCommand::Reorder {
            request_id: ids[3].clone(),
            before: Some(ids[1].clone()),
        });

        assert_eq!(texts(&h.actor.queues.high), vec!["h1", "n2", "h2"]);
        assert_eq!(texts(&h.actor.queues.normal), vec!["n1"]);
    }

    #[test]
    fn reorder_no_op_inputs_leave_both_queues_and_the_bus_untouched() {
        for case in ["absent request id", "self anchor", "absent anchor"] {
            let mut h = Harness::new();
            let ids = stage(
                &mut h,
                &[
                    ("a", Priority::Normal),
                    ("b", Priority::Normal),
                    ("c", Priority::Normal),
                ],
            );
            let absent = RequestId::new();
            let (request_id, before) = match case {
                "absent request id" => (absent, Some(ids[0].clone())),
                "self anchor" => (ids[1].clone(), Some(ids[1].clone())),
                _ => (ids[1].clone(), Some(absent)),
            };
            let _ = h.drain_events();

            h.run(SpeakCommand::Reorder { request_id, before });

            assert_eq!(texts(&h.actor.queues.normal), vec!["a", "b", "c"], "{case}");
            assert!(h.actor.queues.high.is_empty(), "{case}");
            assert!(
                !h.drain_events()
                    .iter()
                    .any(|e| matches!(e, SpeakEvent::QueueChanged { .. })),
                "{case}: a no-op must not report a queue change",
            );
            assert!(
                !h.published("speak.reordered"),
                "{case}: a no-op must not publish a reorder",
            );
        }
    }

    #[test]
    fn reorder_bus_event_identifies_the_moved_item_and_its_anchor() {
        let mut h = Harness::new();
        let anchor = request("anchor", "stay put", Priority::Normal);
        let anchor_id = anchor.request_id.clone();
        let mut moved = request("nova", "move me", Priority::Normal);
        let src = forge_types::EventId::new();
        moved.source_event_id = Some(src);
        let moved_id = moved.request_id.clone();
        h.run(SpeakCommand::Enqueue(anchor));
        h.run(SpeakCommand::Enqueue(moved));

        h.run(SpeakCommand::Reorder {
            request_id: moved_id.clone(),
            before: Some(anchor_id.clone()),
        });

        let ev = h.bus("speak.reordered");
        assert_eq!(ev.payload["request_id"].as_str(), Some(moved_id.0.as_str()));
        assert_eq!(ev.payload["before"].as_str(), Some(anchor_id.0.as_str()));
        assert_eq!(ev.payload["viewer_name"].as_str(), Some("nova"));
        assert_eq!(ev.payload["text"].as_str(), Some("move me"));
        assert_eq!(ev.caused_by, Some(src));
    }

    #[test]
    fn set_volume_clamps_into_unit_range() {
        for (input, expected) in [
            (1.5_f32, 1.0_f32),
            (-0.2, 0.0),
            (0.5, 0.5),
            (0.0, 0.0),
            (1.0, 1.0),
        ] {
            let mut h = Harness::new();
            h.run(SpeakCommand::SetVolume(input));
            assert_eq!(
                h.actor.config.master_volume, expected,
                "SetVolume({input}) must set master_volume to {expected}",
            );
        }
    }

    #[test]
    fn enqueue_emits_event_carrying_request_payload() {
        let mut h = Harness::new();
        h.run(SpeakCommand::Enqueue(request(
            "nova",
            "hi chat",
            Priority::High,
        )));
        let payload = h
            .drain_events()
            .into_iter()
            .find_map(|ev| match ev {
                SpeakEvent::Enqueued {
                    viewer_name,
                    text,
                    is_high_priority,
                    ..
                } => Some((viewer_name, text, is_high_priority)),
                _ => None,
            })
            .expect("an Enqueued event must be emitted");
        assert_eq!(payload, ("nova".to_owned(), "hi chat".to_owned(), true));
    }

    struct CapturingEngine {
        id: EngineId,
        seen: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl forge_tts_core::TtsEngine for CapturingEngine {
        fn engine_id(&self) -> &EngineId {
            &self.id
        }
        async fn list_voices(&self) -> Result<Vec<TtsVoice>, forge_tts_core::TtsError> {
            Ok(vec![])
        }
        async fn synthesize(
            &self,
            req: SynthesisRequest,
        ) -> Result<PcmBuffer, forge_tts_core::TtsError> {
            self.seen.lock().unwrap().push(req.text.into_string());
            Ok(PcmBuffer::new(vec![0i16; 4], 22_050, 1))
        }
    }

    struct CapturingFactory {
        id: EngineId,
        seen: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl forge_tts_core::TtsEngineFactory for CapturingFactory {
        fn engine_id(&self) -> EngineId {
            self.id.clone()
        }

        fn class(&self) -> forge_tts_core::EngineClass {
            forge_tts_core::EngineClass::Local
        }

        fn create(&self) -> Result<Box<dyn forge_tts_core::TtsEngine>, forge_tts_core::TtsError> {
            Ok(Box::new(CapturingEngine {
                id: self.id.clone(),
                seen: self.seen.clone(),
            }))
        }
    }

    async fn spoken_by_capturing_engine(
        cfg: forge_tts_pipeline::PipelineConfig,
        text: &str,
        is_reward: bool,
        message_emotes: &[&str],
    ) -> Option<String> {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine_id = EngineId("cap".into());
        let voice = TtsVoice {
            id: VoiceId("cap-1".into()),
            name: "cap-1".into(),
            locale: "en-US".into(),
            gender: forge_speech_types::VoiceGender::Neutral,
            engine_id: engine_id.clone(),
            is_neural: false,
            sample_rate_hint: 22_050,
        };
        let mut registry = forge_tts_core::TtsRegistry::new();
        registry.register(Arc::new(CapturingFactory {
            id: engine_id,
            seen: seen.clone(),
        }));
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let deps = SynthTaskDeps {
            resolver: Arc::new(resolver),
            pipeline: crate::PipelineConfigHandle::new(cfg),
            registry: Arc::new(registry),
            voice_catalog: Arc::new(vec![voice.clone()]),
            detector: None,
        };
        let mut req = request("nova", text, Priority::Normal);
        req.voice_override = Some(voice.id.clone());
        req.is_reward = is_reward;
        req.message_emotes = message_emotes
            .iter()
            .map(|code| (*code).to_owned())
            .collect();

        let result = run_synthesis(req, deps, Vec::new()).await;
        let spoken = seen.lock().unwrap().clone();
        match result.outcome {
            SynthOutcome::Speak { .. } => {
                assert_eq!(spoken.len(), 1, "one synthesis per spoken request");
                spoken.into_iter().next()
            }
            SynthOutcome::Skipped { .. } => {
                assert!(spoken.is_empty(), "a skipped request reached the engine");
                None
            }
            SynthOutcome::Failed { .. } => panic!("synthesis failed for {text:?}"),
        }
    }

    fn lul_config() -> forge_tts_pipeline::PipelineConfig {
        let mut emote_tokens = forge_tts_pipeline::EmoteTokenSet::default();
        emote_tokens.tokens.insert("LUL".into());
        forge_tts_pipeline::PipelineConfig {
            emote_tokens,
            ..neutral_pipeline()
        }
    }

    #[test]
    fn enqueue_estimate_is_capped_by_the_maximum_duration() {
        const SECS_OF_SPEECH: usize = 20;
        let text = "a".repeat(SECS_OF_SPEECH * 15);
        for (text, cap, expected) in [
            (text.as_str(), None, 20),
            (text.as_str(), Some(5), 5),
            (text.as_str(), Some(19), 19),
            (text.as_str(), Some(20), 20),
            (text.as_str(), Some(21), 20),
            (text.as_str(), Some(600), 20),
            ("hi", Some(5), 1),
            ("hi", None, 1),
        ] {
            let deps = minimal_deps();
            let mut cfg = neutral_pipeline();
            cfg.output.max_duration_secs = cap;
            deps.pipeline.swap(cfg);
            let req = request("nova", text, Priority::Normal);

            let estimated_secs = enqueue_preview(&req, &deps, &[]).estimated_secs;

            assert_eq!(estimated_secs, expected, "len {} cap {cap:?}", text.len());
        }
    }

    #[tokio::test]
    async fn reward_emote_strip_fires_only_when_reward_and_toggle_both_set() {
        for (is_reward, strip_reward_emotes, expected) in [
            (true, true, "hi"),
            (false, true, "hi LUL"),
            (true, false, "hi LUL"),
            (false, false, "hi LUL"),
        ] {
            let cfg = forge_tts_pipeline::PipelineConfig {
                strip_reward_emotes,
                ..lul_config()
            };

            let spoken = spoken_by_capturing_engine(cfg, "hi LUL", is_reward, &[]).await;

            assert_eq!(
                spoken.as_deref(),
                Some(expected),
                "is_reward={is_reward} strip_reward_emotes={strip_reward_emotes}",
            );
        }
    }

    #[tokio::test]
    async fn strip_emoji_setting_removes_emoji_from_chat_and_reward_messages_only_when_on() {
        for (strip_emoji, is_reward, text, expected) in [
            (true, false, "nice\u{1F389}", "nice"),
            (true, true, "LUL nice\u{1F389}", "nice"),
            (false, false, "nice\u{1F389}", "nice\u{1F389}"),
            (false, true, "nice\u{1F389}", "nice\u{1F389}"),
        ] {
            let settings = forge_storage::TtsPipelineSettings {
                strip_emoji,
                ..Default::default()
            };
            let (mut cfg, _) = crate::build_config(&[], &settings).dropping_invalid();
            cfg.emote_tokens.tokens.insert("LUL".into());

            let spoken = spoken_by_capturing_engine(cfg, text, is_reward, &[]).await;

            assert_eq!(
                spoken.as_deref(),
                Some(expected),
                "strip_emoji={strip_emoji} is_reward={is_reward}"
            );
        }
    }

    #[tokio::test]
    async fn a_requests_own_emote_codes_are_stripped_like_configured_ones() {
        let strip_twitch = || {
            let mut cfg = neutral_pipeline();
            cfg.output.emote_treatment = forge_tts_pipeline::EmoteTreatment::StripTwitch;
            cfg
        };
        let skip_emote_only = || {
            let mut cfg = neutral_pipeline();
            cfg.skip_rules.emote_only = true;
            cfg
        };
        let strip_reward = || forge_tts_pipeline::PipelineConfig {
            strip_reward_emotes: true,
            ..neutral_pipeline()
        };
        for (label, cfg, text, is_reward, codes, expected) in [
            (
                "strip on",
                strip_twitch(),
                "LUL that was funny",
                false,
                &["LUL"][..],
                Some("that was funny"),
            ),
            (
                "emote-only skip",
                skip_emote_only(),
                "LUL LUL",
                false,
                &["LUL"][..],
                None,
            ),
            (
                "emote-only rule without codes",
                skip_emote_only(),
                "LUL LUL",
                false,
                &[][..],
                Some("LUL LUL"),
            ),
            (
                "strip off",
                neutral_pipeline(),
                "LUL that was funny",
                false,
                &["LUL"][..],
                Some("LUL that was funny"),
            ),
            (
                "no codes on the request",
                strip_twitch(),
                "LUL that was funny",
                false,
                &[][..],
                Some("LUL that was funny"),
            ),
            (
                "reward strip",
                strip_reward(),
                "LUL hi",
                true,
                &["LUL"][..],
                Some("hi"),
            ),
        ] {
            let spoken = spoken_by_capturing_engine(cfg, text, is_reward, codes).await;

            assert_eq!(spoken.as_deref(), expected, "{label}");
        }
    }

    #[test]
    fn rejected_bus_event_carries_token_limit_and_current_queue_len() {
        for (config, first_viewer, second_viewer, token) in [
            (
                QueueConfig {
                    max_queue_len: 1,
                    per_user_limit: 50,
                    ..QueueConfig::default()
                },
                "a",
                "b",
                "queue_full",
            ),
            (
                QueueConfig {
                    max_queue_len: 50,
                    per_user_limit: 1,
                    ..QueueConfig::default()
                },
                "same",
                "same",
                "per_user_limit",
            ),
        ] {
            let mut h = Harness::new();
            h.actor.config = config;
            h.run(SpeakCommand::Enqueue(request(
                first_viewer,
                "accepted",
                Priority::Normal,
            )));
            let mut rejected = request(second_viewer, "over the cap", Priority::Normal);
            let src = forge_types::EventId::new();
            rejected.source_event_id = Some(src);
            h.run(SpeakCommand::Enqueue(rejected));

            let ev = h.bus("speak.rejected");
            assert_eq!(ev.payload["reason"].as_str(), Some(token));
            assert_eq!(ev.payload["limit"].as_u64(), Some(1));
            assert_eq!(ev.payload["queue_len"].as_u64(), Some(1));
            assert_eq!(ev.payload["viewer_name"].as_str(), Some(second_viewer));
            assert_eq!(ev.payload["text"].as_str(), Some("over the cap"));
            assert_eq!(ev.caused_by, Some(src));
        }
    }

    #[test]
    fn pause_and_resume_bus_reasons_are_symmetric_per_origin() {
        for (cmd, kind, reason) in [
            (SpeakCommand::Pause, "speak.paused", "user"),
            (SpeakCommand::Resume, "speak.resumed", "user"),
            (
                SpeakCommand::VoiceGateActivated,
                "speak.paused",
                "voicegate",
            ),
            (
                SpeakCommand::VoiceGateDeactivated,
                "speak.resumed",
                "voicegate",
            ),
        ] {
            let mut h = Harness::new();
            h.run(cmd);
            let ev = h.bus(kind);
            assert_eq!(ev.payload["reason"].as_str(), Some(reason));
            assert!(ev.caused_by.is_none());
        }
    }

    #[test]
    fn enqueued_bus_event_links_caused_by_only_when_request_carries_source() {
        let src = forge_types::EventId::new();
        for (source, expected) in [(Some(src), Some(src)), (None, None)] {
            let mut h = Harness::new();
            let mut req = request("nova", "hi chat", Priority::Normal);
            req.source_event_id = source;
            h.run(SpeakCommand::Enqueue(req));
            let ev = h.bus("speak.enqueued");
            assert_eq!(ev.caused_by, expected);
        }
    }

    #[test]
    fn replay_publishes_enqueued_bus_event_carrying_original_source() {
        let mut h = Harness::new();
        let src = forge_types::EventId::new();
        let mut last = request("nova", "again please", Priority::Normal);
        last.source_event_id = Some(src);
        h.actor.last_successful = Some(last);

        h.run(SpeakCommand::Replay);

        let ev = h.bus("speak.enqueued");
        assert_eq!(ev.payload["viewer_name"].as_str(), Some("nova"));
        assert_eq!(ev.payload["text"].as_str(), Some("again please"));
        assert_eq!(ev.caused_by, Some(src));
    }

    #[test]
    fn pipeline_skip_carries_rule_name_only_for_matched_rule() {
        use forge_tts_pipeline::SkipReason as PipelineSkip;
        for (pipeline, expected) in [
            (
                PipelineSkip::MatchedSkipRule(forge_tts_pipeline::SkipRule::ContainsUrl),
                (SkipReason::SkipRuleMatched, Some("contains_url".to_owned())),
            ),
            (
                PipelineSkip::BlockedByWordFilter,
                (SkipReason::BlockedByWordFilter, None),
            ),
            (
                PipelineSkip::EmptyAfterProcessing,
                (SkipReason::EmptyAfterProcessing, None),
            ),
        ] {
            assert_eq!(pipeline_skip(&pipeline), expected);
        }
    }

    fn bus_trail(
        h: &Harness,
    ) -> Vec<(
        String,
        Option<serde_json::Value>,
        Option<forge_types::EventId>,
    )> {
        h.events
            .events()
            .iter()
            .map(|e| (e.kind.clone(), e.payload.get("text").cloned(), e.caused_by))
            .collect()
    }

    fn source_of(h: &Harness, text: &str) -> Option<forge_types::EventId> {
        h.actor
            .queues
            .high
            .iter()
            .chain(h.actor.queues.normal.iter())
            .find(|r| r.text == text)
            .and_then(|r| r.source_event_id)
    }

    fn stage_mixed_pending(h: &mut Harness) -> [Option<forge_types::EventId>; 3] {
        stage(
            h,
            &[
                ("a", Priority::Normal),
                ("b", Priority::High),
                ("c", Priority::Normal),
            ],
        );
        let sources = [source_of(h, "b"), source_of(h, "a"), source_of(h, "c")];
        drop(h.events.take());
        sources
    }

    fn removed(
        text: &str,
        source: Option<forge_types::EventId>,
    ) -> (
        String,
        Option<serde_json::Value>,
        Option<forge_types::EventId>,
    ) {
        (
            "speak.removed".to_owned(),
            Some(serde_json::json!(text)),
            source,
        )
    }

    #[test]
    fn clear_pending_reports_each_pending_message_removed_in_play_order_then_cleared() {
        let mut h = Harness::new();
        let [b, a, c] = stage_mixed_pending(&mut h);

        h.run(SpeakCommand::ClearPending);

        assert_eq!(
            bus_trail(&h),
            vec![
                removed("b", b),
                removed("a", a),
                removed("c", c),
                ("speak.cleared".to_owned(), None, None),
            ]
        );
        assert_eq!(
            h.bus("speak.cleared").payload,
            serde_json::json!({ "keep_current": true })
        );
    }

    #[test]
    fn clear_pending_keeps_the_in_flight_message() {
        let mut h = Harness::new();
        let current = RequestId::new();
        h.actor.active_request_id = Some(current.clone());
        stage_mixed_pending(&mut h);

        h.run(SpeakCommand::ClearPending);

        assert_eq!(h.actor.active_request_id, Some(current));
        assert!(!h.published("speak.skipped"));
    }

    #[test]
    fn clear_stops_the_in_flight_message_then_reports_each_pending_removed_then_cleared() {
        let mut h = Harness::new();
        h.actor.active_request_id = Some(RequestId::new());
        let [b, a, c] = stage_mixed_pending(&mut h);

        h.run(SpeakCommand::Clear);

        assert_eq!(
            bus_trail(&h),
            vec![
                (
                    "speak.skipped".to_owned(),
                    Some(serde_json::Value::Null),
                    None
                ),
                removed("b", b),
                removed("a", a),
                removed("c", c),
                ("speak.cleared".to_owned(), None, None),
            ]
        );
        assert_eq!(
            h.bus("speak.cleared").payload,
            serde_json::json!({ "keep_current": false })
        );
        assert!(h.actor.active_request_id.is_none());
    }

    #[test]
    fn clear_with_nothing_queued_publishes_only_cleared() {
        for (cmd, keep_current) in [
            (SpeakCommand::Clear, false),
            (SpeakCommand::ClearPending, true),
        ] {
            let mut h = Harness::new();
            h.run(cmd);
            assert_eq!(
                bus_trail(&h),
                vec![("speak.cleared".to_owned(), None, None)]
            );
            assert_eq!(
                h.bus("speak.cleared").payload,
                serde_json::json!({ "keep_current": keep_current })
            );
        }
    }

    #[test]
    fn stopping_a_message_still_synthesizing_publishes_skipped_without_viewer_or_text() {
        let mut h = Harness::new();
        let in_flight = RequestId::new();
        h.actor.active_request_id = Some(in_flight.clone());

        h.run(SpeakCommand::Skip);

        let ev = h.bus("speak.skipped");
        assert_eq!(
            ev.payload,
            serde_json::json!({
                "request_id": in_flight.0,
                "reason": "user_skip",
                "detail": null,
                "viewer_name": null,
                "text": null,
            })
        );
        assert!(ev.caused_by.is_none());
    }

    fn prefixing_pipeline() -> forge_tts_pipeline::PipelineConfig {
        let mut config = neutral_pipeline();
        config.output.read_display_name_first = true;
        config
    }

    #[test]
    fn voiced_text_keeps_the_neutral_text_for_english_unknown_and_missing_voice_languages() {
        let config = prefixing_pipeline();
        let context = forge_tts_pipeline::PipelineContext {
            viewer_name: "nova",
            recent_messages: &[],
            speech_language: None,
        };
        for language in [
            LanguageCode::from_locale("en-US"),
            LanguageCode::from_locale("de-DE"),
            None,
        ] {
            let result = voiced_text("NEUTRAL".into(), "hi", &config, &context, language);
            assert_eq!(
                result,
                PipelineResult::Speak("NEUTRAL".into()),
                "{language:?}"
            );
        }
    }

    #[test]
    fn voiced_text_reprocesses_the_input_in_a_ukrainian_voice_language() {
        let config = prefixing_pipeline();
        let context = forge_tts_pipeline::PipelineContext {
            viewer_name: "nova",
            recent_messages: &[],
            speech_language: None,
        };
        let result = voiced_text(
            "NEUTRAL".into(),
            "hi",
            &config,
            &context,
            LanguageCode::from_locale("uk-UA"),
        );
        assert_eq!(result, PipelineResult::Speak("nova каже: hi".into()));
    }

    #[test]
    fn voiced_text_reports_a_skip_that_only_the_voice_language_pass_raises() {
        let mut config = neutral_pipeline();
        config.output.read_display_name_first = false;
        let context = forge_tts_pipeline::PipelineContext {
            viewer_name: "nova",
            recent_messages: &[],
            speech_language: None,
        };
        let result = voiced_text(
            "NEUTRAL".into(),
            "   ",
            &config,
            &context,
            LanguageCode::from_locale("uk"),
        );
        assert!(matches!(result, PipelineResult::Skip { .. }), "{result:?}");
    }

    #[tokio::test]
    async fn a_detector_build_is_current_until_a_newer_build_or_invalidation_supersedes_it() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<DetectorBuild>(4);
        let mut builds =
            DetectorBuilds::new(tx, tokio::sync::watch::Sender::new(false), TaskOwner::new());

        builds.rebuild(&[]);
        let first = rx.recv().await.expect("first build result");
        assert!(builds.is_current(first.generation));

        builds.rebuild(&[]);
        assert!(!builds.is_current(first.generation));
        let second = rx.recv().await.expect("second build result");
        assert!(builds.is_current(second.generation));
        assert_ne!(first.generation, second.generation);

        builds.invalidate();
        assert!(!builds.is_current(second.generation));
    }

    #[tokio::test]
    async fn a_build_finished_after_a_newer_rebuild_was_started_is_reported_stale() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<DetectorBuild>(4);
        let mut builds =
            DetectorBuilds::new(tx, tokio::sync::watch::Sender::new(false), TaskOwner::new());

        builds.rebuild(&[]);
        builds.rebuild(&[]);

        let mut generations = Vec::new();
        for _ in 0..2 {
            generations.push(rx.recv().await.expect("build result").generation);
        }
        let current: Vec<bool> = generations.iter().map(|g| builds.is_current(*g)).collect();
        assert_eq!(current.iter().filter(|c| **c).count(), 1);
        assert!(builds.is_current(builds.generation));
    }
}
