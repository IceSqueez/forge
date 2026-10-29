use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

const PLAYBACK_PROGRESS_TICK: Duration = Duration::from_secs(1);

use forge_audio::{AudioError, AudioSink};
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice, VoiceId};
use forge_tts_core::{SynthesisRequest, TtsError, engine_display_name};
use forge_tts_pipeline::{DetectionOutcome, LanguageDetector, PipelineResult};
use forge_types::Shared;

use crate::catalog::{CATALOG_UPDATE_CAPACITY, CatalogUpdate, VoiceCatalogState};
use crate::emitter::{
    DetectedLanguage, EnqueuePreview, Outcome, ResolvedPlayback, SpeakEmitter, pipeline_skip,
};
use crate::vocabulary::{FailureReason, HoldReason, RejectReason, SkipReason};
use crate::voice::{ResolveResult, VoiceAliasResolver, candidate_languages, voice_speaks_language};

use crate::{
    PipelineConfigHandle, Priority, QueueConfig, QueueDeps, QueuedOrderEntry, RequestId,
    SpeakCommand, SpeakEvent, SpeakRequest, TargetedLegs,
};

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

enum CurrentPlayback {
    Playing(PlayingClip),
    Held(ReadyClip),
}

fn is_playing(current: &Option<CurrentPlayback>) -> bool {
    matches!(current, Some(CurrentPlayback::Playing(_)))
}

fn is_held(current: &Option<CurrentPlayback>) -> bool {
    matches!(current, Some(CurrentPlayback::Held(_)))
}

async fn poll_current(
    current: &mut Option<CurrentPlayback>,
) -> Result<(), forge_audio::AudioError> {
    match current {
        Some(CurrentPlayback::Playing(c)) => (&mut c.playback).await,
        _ => std::future::pending().await,
    }
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
        error: String,
    },
}

struct SynthTaskDeps {
    resolver: Arc<std::sync::RwLock<VoiceAliasResolver>>,
    pipeline: PipelineConfigHandle,
    registry: Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>,
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
    };
    let pipeline_result = forge_tts_pipeline::process(text_for_pipeline, &pipeline_cfg, &context);
    let text_to_speak = match pipeline_result {
        PipelineResult::Speak(t) => t,
        PipelineResult::Skip { reason } => {
            let (reason, detail) = pipeline_skip(&reason);
            return SynthTaskResult::new(req, SynthOutcome::Skipped { reason, detail });
        }
    };

    let detected = detect_language(
        &deps,
        &pipeline_cfg,
        &context,
        text_for_pipeline,
        &text_to_speak,
    )
    .await;

    let resolve_result = {
        let guard = deps.resolver.read().unwrap_or_else(|e| e.into_inner());
        guard.resolve(&req, detected.as_ref().map(|d| d.code), &deps.voice_catalog)
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

    let slot = deps
        .registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .slot(&engine_id);
    let slot = match slot {
        Some(slot) => slot,
        None => {
            let error = format!("engine {:?} not registered", engine_id);
            return SynthTaskResult::new(req, engine_failure(error));
        }
    };

    let engine = match slot.engine().await {
        Ok(e) => e,
        Err(e) => {
            return SynthTaskResult::new(req, engine_failure(e.to_string()));
        }
    };

    let synthesis_req = SynthesisRequest {
        text: text_to_speak,
        voice_id: voice_id.clone(),
        pitch_semitones: pitch,
        rate_multiplier: rate,
        ssml: false,
    };

    let outcome = match engine.synthesize(synthesis_req).await {
        Ok(pcm) => SynthOutcome::Speak {
            pcm,
            voice_id,
            engine_id,
            language: detected,
        },
        Err(e) => engine_failure(e.to_string()),
    };
    SynthTaskResult::new(req, outcome)
}

fn engine_failure(error: String) -> SynthOutcome {
    SynthOutcome::Failed {
        reason: FailureReason::EngineError,
        error,
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
                error: TtsError::Timeout {
                    ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
                }
                .to_string(),
            },
        ),
    }
}

async fn detect_language(
    deps: &SynthTaskDeps,
    pipeline_cfg: &forge_tts_pipeline::PipelineConfig,
    context: &forge_tts_pipeline::PipelineContext<'_>,
    pipeline_input: &str,
    spoken: &str,
) -> Option<DetectedLanguage> {
    if !pipeline_cfg.output.language_aware_voice {
        return None;
    }
    let detector = deps.detector.clone()?;
    let sample = if pipeline_cfg.output.read_display_name_first {
        forge_tts_pipeline::process_for_language(pipeline_input, pipeline_cfg, context)?
    } else {
        spoken.to_owned()
    };

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

fn queue_changed_event(
    high_queue: &VecDeque<SpeakRequest>,
    normal_queue: &VecDeque<SpeakRequest>,
) -> SpeakEvent {
    let order: Vec<QueuedOrderEntry> = high_queue
        .iter()
        .map(|r| QueuedOrderEntry {
            request_id: r.request_id.clone(),
            is_high_priority: true,
        })
        .chain(normal_queue.iter().map(|r| QueuedOrderEntry {
            request_id: r.request_id.clone(),
            is_high_priority: false,
        }))
        .collect();
    SpeakEvent::QueueChanged {
        queue_len: order.len(),
        order,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "actor state is passed as separate borrows"
)]
pub(crate) async fn run_actor(
    mut config: QueueConfig,
    deps: QueueDeps,
    mut cmd_rx: tokio::sync::mpsc::Receiver<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    depth: Arc<AtomicUsize>,
    voices: Shared<Vec<TtsVoice>>,
    disabled_engines: Shared<HashSet<EngineId>>,
    master_volume_bits: Arc<AtomicU32>,
    engine_gains: Shared<HashMap<EngineId, f32>>,
    targeted_legs: TargetedLegs,
) {
    let mut high_queue: VecDeque<SpeakRequest> = VecDeque::new();
    let mut normal_queue: VecDeque<SpeakRequest> = VecDeque::new();
    let mut per_user_counts: HashMap<String, usize> = HashMap::new();
    let mut paused = false;
    let mut voicegate_active = false;
    let mut active_request_id: Option<RequestId> = None;
    let mut last_successful: Option<SpeakRequest> = None;
    let mut current_playback: Option<CurrentPlayback> = None;
    let mut progress_ticker: Option<tokio::time::Interval> = None;
    let mut recent_messages: VecDeque<String> = VecDeque::new();

    let (synth_tx, mut synth_rx) = tokio::sync::mpsc::channel::<SynthTaskResult>(8);
    let (catalog_tx, mut catalog_rx) =
        tokio::sync::mpsc::channel::<CatalogUpdate>(CATALOG_UPDATE_CAPACITY);
    let (detector_tx, mut detector_rx) =
        tokio::sync::mpsc::channel::<Option<Arc<LanguageDetector>>>(1);

    {
        let mut registry = deps.registry.write().unwrap_or_else(|e| e.into_inner());
        for engine_id in &deps.disabled_engines {
            registry.invalidate(engine_id);
        }
    }
    disabled_engines.store(deps.disabled_engines.clone());
    engine_gains.store(deps.engine_gains.clone());
    let mut catalog = VoiceCatalogState::default();
    let initial_catalog = catalog.rebuild(&deps.registry, &deps.disabled_engines, &catalog_tx);
    voices.store_arc(initial_catalog.clone());
    let mut task_deps = SynthTaskDeps {
        resolver: deps.resolver.clone(),
        pipeline: deps.pipeline.clone(),
        registry: deps.registry.clone(),
        voice_catalog: initial_catalog,
        detector: None,
    };

    let mut language_aware = deps.pipeline.load().output.language_aware_voice;

    loop {
        depth.store(high_queue.len() + normal_queue.len(), Ordering::Relaxed);

        let language_aware_now = deps.pipeline.load().output.language_aware_voice;
        if language_aware_now != language_aware {
            language_aware = language_aware_now;
            if language_aware {
                spawn_detector_rebuild(&task_deps.voice_catalog, detector_tx.clone());
            } else {
                task_deps.detector = None;
            }
        }

        if !paused
            && is_held(&current_playback)
            && let Some(CurrentPlayback::Held(clip)) = current_playback.take()
        {
            let gains = engine_gains.load();
            start_clip(
                clip,
                &config,
                &deps,
                &event_tx,
                &mut active_request_id,
                &mut current_playback,
                &mut progress_ticker,
                high_queue.len() + normal_queue.len(),
                gains.as_ref(),
                &targeted_legs,
            )
            .await;
        }

        if active_request_id.is_none()
            && !paused
            && !voicegate_active
            && next_request(&high_queue, &normal_queue).is_some_and(|req| {
                let resolver = deps.resolver.read().unwrap_or_else(|e| e.into_inner());
                catalog.serves(
                    req,
                    resolver.active_alias_engine(&req.viewer_id, &req.viewer_name),
                )
            })
            && let Some(req) = pop_next(&mut high_queue, &mut normal_queue, &mut per_user_counts)
        {
            let queue_len = high_queue.len() + normal_queue.len();
            let emit = SpeakEmitter::new(&event_tx, deps.event_bus.as_ref());
            emit.feed(queue_changed_event(&high_queue, &normal_queue));
            emit.started(&req, None, queue_len);
            active_request_id = Some(req.request_id.clone());
            let tx = synth_tx.clone();
            let task_deps_clone = SynthTaskDeps {
                resolver: task_deps.resolver.clone(),
                pipeline: task_deps.pipeline.clone(),
                registry: task_deps.registry.clone(),
                voice_catalog: task_deps.voice_catalog.clone(),
                detector: task_deps.detector.clone(),
            };
            while recent_messages.len() > config.repeat_window.get() {
                recent_messages.pop_front();
            }
            let recent_snapshot: Vec<String> = recent_messages.iter().cloned().collect();
            while recent_messages.len() >= config.repeat_window.get() {
                recent_messages.pop_front();
            }
            recent_messages.push_back(req.text.clone());
            let timeout = config.timeout_per_item;
            tokio::spawn(async move {
                let result =
                    run_synthesis_bounded(req, task_deps_clone, recent_snapshot, timeout).await;
                if tx.send(result).await.is_err() {
                    tracing::debug!("speak queue stopped before the synthesis result arrived");
                }
            });
        }

        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                    Some(SpeakCommand::RefreshVoiceCatalog) => {
                        deps.registry
                            .write()
                            .unwrap_or_else(|e| e.into_inner())
                            .invalidate_all();
                        let rebuilt = catalog.rebuild(
                            &deps.registry,
                            &disabled_engines.load(),
                            &catalog_tx,
                        );
                        publish_catalog(
                            rebuilt,
                            &voices,
                            &mut task_deps,
                            language_aware,
                            &detector_tx,
                        );
                    }
                    Some(SpeakCommand::SetEngineEnabled(engine_id, enabled)) => {
                        let mut next = (*disabled_engines.load()).clone();
                        let changed = if enabled {
                            next.remove(&engine_id)
                        } else {
                            next.insert(engine_id.clone())
                        };
                        if changed {
                            deps.registry
                                .write()
                                .unwrap_or_else(|e| e.into_inner())
                                .invalidate(&engine_id);
                        }
                        let rebuilt = catalog.rebuild(&deps.registry, &next, &catalog_tx);
                        disabled_engines.store(next);
                        publish_catalog(
                            rebuilt,
                            &voices,
                            &mut task_deps,
                            language_aware,
                            &detector_tx,
                        );
                    }
                    Some(SpeakCommand::SetVolume(volume)) => {
                        handle_command(
                            SpeakCommand::SetVolume(volume),
                            &mut config,
                            &deps,
                            &event_tx,
                            &mut high_queue,
                            &mut normal_queue,
                            &mut per_user_counts,
                            &mut paused,
                            &mut voicegate_active,
                            &mut active_request_id,
                            &last_successful,
                            &mut current_playback,
                            &mut progress_ticker,
                            &task_deps.voice_catalog,
                        );
                        master_volume_bits.store(config.master_volume.to_bits(), Ordering::Relaxed);
                    }
                    Some(SpeakCommand::SetEngineParams(engine_id, defaults, gain)) => {
                        {
                            let mut guard = deps.resolver.write().unwrap_or_else(|e| e.into_inner());
                            guard.set_engine_default(engine_id.clone(), defaults);
                        }
                        let mut next = (*engine_gains.load()).clone();
                        next.insert(engine_id, gain.clamp(0.0, 1.0));
                        engine_gains.store(next);
                    }
                    Some(c) => handle_command(
                        c,
                        &mut config,
                        &deps,
                        &event_tx,
                        &mut high_queue,
                        &mut normal_queue,
                        &mut per_user_counts,
                        &mut paused,
                        &mut voicegate_active,
                        &mut active_request_id,
                        &last_successful,
                        &mut current_playback,
                        &mut progress_ticker,
                        &task_deps.voice_catalog,
                    ),
                }
            }
            result = synth_rx.recv() => {
                if let Some(r) = result {
                    let gains = engine_gains.load();
                    handle_synth_result(
                        r,
                        &config,
                        &deps,
                        &event_tx,
                        &mut active_request_id,
                        &mut current_playback,
                        &mut progress_ticker,
                        paused,
                        high_queue.len() + normal_queue.len(),
                        gains.as_ref(),
                        &targeted_legs,
                    ).await;
                }
            }
            result = catalog_rx.recv() => {
                if let Some(merged) = result.and_then(|update| catalog.apply(update)) {
                    publish_catalog(
                        merged,
                        &voices,
                        &mut task_deps,
                        language_aware,
                        &detector_tx,
                    );
                }
            }
            result = detector_rx.recv() => {
                if let Some(detector) = result {
                    task_deps.detector = detector.filter(|_| language_aware);
                }
            }
            _ = tick_progress(&mut progress_ticker), if progress_ticker.is_some() => {
                if !paused && let Some(CurrentPlayback::Playing(c)) = current_playback.as_mut() {
                    c.elapsed_secs += 1;
                    SpeakEmitter::new(&event_tx, deps.event_bus.as_ref()).feed(SpeakEvent::Progress {
                        request_id: c.request_id.clone(),
                        elapsed_secs: c.elapsed_secs,
                    });
                }
            }
            play_result = poll_current(&mut current_playback), if is_playing(&current_playback) => {
                if let Some(CurrentPlayback::Playing(current)) = current_playback.take() {
                    progress_ticker = None;
                    finish_playback(
                        play_result,
                        &deps,
                        &event_tx,
                        &mut active_request_id,
                        &mut last_successful,
                        current,
                    ).await;
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "actor state is passed as separate borrows"
)]
async fn handle_synth_result(
    result: SynthTaskResult,
    config: &QueueConfig,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
    paused: bool,
    queue_len: usize,
    engine_gains: &HashMap<EngineId, f32>,
    targeted_legs: &TargetedLegs,
) {
    if active_request_id.as_ref() != Some(&result.request.request_id) {
        return;
    }

    match result.outcome {
        SynthOutcome::Skipped { reason, detail } => {
            *active_request_id = None;
            tracing::debug!(request_id = %result.request.request_id.0, ?reason, "speak skipped");
            SpeakEmitter::new(event_tx, deps.event_bus.as_ref())
                .outcome(&result.request, Outcome::Skipped { reason, detail });
        }
        SynthOutcome::Failed { reason, error } => {
            *active_request_id = None;
            tracing::warn!(request_id = %result.request.request_id.0, error = %error, "speak failed");
            SpeakEmitter::new(event_tx, deps.event_bus.as_ref())
                .outcome(&result.request, Outcome::Failed { reason, error });
        }
        SynthOutcome::Speak {
            mut pcm,
            voice_id,
            engine_id,
            language,
        } => {
            if let Some(cap_secs) = deps.pipeline.load().output.max_duration_secs {
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
            if paused {
                *current_playback = Some(CurrentPlayback::Held(clip));
                return;
            }
            start_clip(
                clip,
                config,
                deps,
                event_tx,
                active_request_id,
                current_playback,
                progress_ticker,
                queue_len,
                engine_gains,
                targeted_legs,
            )
            .await;
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "actor state is passed as separate borrows"
)]
async fn start_clip(
    clip: ReadyClip,
    config: &QueueConfig,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
    queue_len: usize,
    engine_gains: &HashMap<EngineId, f32>,
    targeted_legs: &TargetedLegs,
) {
    let ReadyClip {
        request_id,
        request,
        mut pcm,
        voice_id,
        engine_id,
        language,
    } = clip;
    let duration_secs = (pcm.duration_ms() / 1000) as u32;
    let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());
    emit.started(
        &request,
        Some(ResolvedPlayback {
            voice_id: &voice_id,
            engine_id: &engine_id,
            duration_secs,
            language: language.as_ref(),
        }),
        queue_len,
    );
    let engine_gain = engine_gains.get(&engine_id).copied().unwrap_or(1.0);
    pcm.apply_gain(config.master_volume * engine_gain);
    let started = match sink_for(&request, deps, targeted_legs) {
        Ok(sink) => sink.play_controlled(pcm).await,
        Err(e) => Err(e),
    };
    match started {
        Ok(playback) => {
            let mut ticker = tokio::time::interval(PLAYBACK_PROGRESS_TICK);
            ticker.tick().await;
            *progress_ticker = Some(ticker);
            *current_playback = Some(CurrentPlayback::Playing(PlayingClip {
                request_id,
                request,
                playback,
                elapsed_secs: 0,
            }));
        }
        Err(e) => {
            *active_request_id = None;
            tracing::warn!(request_id = %request_id.0, error = %e, "audio playback failed to start");
            emit.outcome(&request, device_failure(&e));
        }
    }
}

fn sink_for(
    request: &SpeakRequest,
    deps: &QueueDeps,
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

async fn finish_playback(
    play_result: Result<(), forge_audio::AudioError>,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    last_successful: &mut Option<SpeakRequest>,
    current: PlayingClip,
) {
    *active_request_id = None;
    let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());

    match play_result {
        Ok(()) => {
            emit.outcome(&current.request, Outcome::Finished);
            *last_successful = Some(current.request);
        }
        Err(e) => {
            tracing::warn!(request_id = %current.request_id.0, error = %e, "audio playback failed");
            emit.outcome(&current.request, device_failure(&e));
        }
    }
}

fn device_failure(error: &AudioError) -> Outcome {
    Outcome::Failed {
        reason: FailureReason::DeviceError,
        error: error.to_string(),
    }
}

fn stop_active(
    reason: SkipReason,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
) {
    let Some(request_id) = active_request_id.take() else {
        return;
    };
    let stopped_request = match current_playback.take() {
        Some(CurrentPlayback::Playing(current)) => {
            current.playback.stop();
            Some(current.request)
        }
        Some(CurrentPlayback::Held(held)) => Some(held.request),
        None => None,
    };
    *progress_ticker = None;
    let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());
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

fn take_from_queues(
    high_queue: &mut VecDeque<SpeakRequest>,
    normal_queue: &mut VecDeque<SpeakRequest>,
    request_id: &RequestId,
) -> Option<SpeakRequest> {
    if let Some(pos) = high_queue.iter().position(|r| &r.request_id == request_id) {
        return high_queue.remove(pos);
    }
    if let Some(pos) = normal_queue
        .iter()
        .position(|r| &r.request_id == request_id)
    {
        return normal_queue.remove(pos);
    }
    None
}

fn enqueue_preview(req: &SpeakRequest, deps: &QueueDeps, catalog: &[TtsVoice]) -> EnqueuePreview {
    let spoken_secs = ((req.text.chars().count() as u32) / 15).max(1);
    let estimated_secs = match deps.pipeline.load().output.max_duration_secs {
        Some(cap_secs) => spoken_secs.min(cap_secs),
        None => spoken_secs,
    };
    let guard = deps.resolver.read().unwrap_or_else(|e| e.into_inner());
    let preview = match guard.resolve(req, None, catalog) {
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

#[expect(
    clippy::too_many_arguments,
    reason = "actor state is passed as separate borrows"
)]
fn admit_request(
    req: SpeakRequest,
    config: &QueueConfig,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    high_queue: &mut VecDeque<SpeakRequest>,
    normal_queue: &mut VecDeque<SpeakRequest>,
    per_user_counts: &mut HashMap<String, usize>,
    voice_catalog: &[TtsVoice],
) {
    let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());
    let total = high_queue.len() + normal_queue.len();
    let user_count = per_user_counts.get(&req.viewer_id).copied().unwrap_or(0);
    let rejection = if total >= config.max_queue_len {
        tracing::debug!(request_id = %req.request_id.0, "queue full, rejecting");
        Some((RejectReason::QueueFull, config.max_queue_len))
    } else if user_count >= config.per_user_limit {
        tracing::debug!(viewer_id = %req.viewer_id, "per-user limit reached");
        Some((RejectReason::PerUserLimit, config.per_user_limit))
    } else {
        None
    };
    if let Some((reason, limit)) = rejection {
        emit.outcome(
            &req,
            Outcome::Rejected {
                reason,
                limit,
                queue_len: total,
            },
        );
        return;
    }
    *per_user_counts.entry(req.viewer_id.clone()).or_insert(0) += 1;
    let preview = enqueue_preview(&req, deps, voice_catalog);
    emit.enqueued(&req, total + 1, preview);
    match req.priority {
        Priority::High => high_queue.push_back(req),
        Priority::Normal => normal_queue.push_back(req),
    }
    emit.feed(queue_changed_event(high_queue, normal_queue));
}

#[expect(
    clippy::too_many_arguments,
    reason = "actor state is passed as separate borrows"
)]
fn handle_command(
    cmd: SpeakCommand,
    config: &mut QueueConfig,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    high_queue: &mut VecDeque<SpeakRequest>,
    normal_queue: &mut VecDeque<SpeakRequest>,
    per_user_counts: &mut HashMap<String, usize>,
    paused: &mut bool,
    voicegate_active: &mut bool,
    active_request_id: &mut Option<RequestId>,
    last_successful: &Option<SpeakRequest>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
    voice_catalog: &[TtsVoice],
) {
    match cmd {
        SpeakCommand::Enqueue(req) => {
            admit_request(
                req,
                config,
                deps,
                event_tx,
                high_queue,
                normal_queue,
                per_user_counts,
                voice_catalog,
            );
        }
        SpeakCommand::Skip => {
            stop_active(
                SkipReason::UserSkip,
                deps,
                event_tx,
                active_request_id,
                current_playback,
                progress_ticker,
            );
        }
        SpeakCommand::PlayNow(request_id) => {
            if let Some(req) = take_from_queues(high_queue, normal_queue, &request_id) {
                high_queue.push_front(req);
                stop_active(
                    SkipReason::Promoted,
                    deps,
                    event_tx,
                    active_request_id,
                    current_playback,
                    progress_ticker,
                );
                SpeakEmitter::new(event_tx, deps.event_bus.as_ref())
                    .feed(queue_changed_event(high_queue, normal_queue));
            }
        }
        SpeakCommand::RemoveQueued(request_id) => {
            remove_queued(
                &request_id,
                deps,
                event_tx,
                high_queue,
                normal_queue,
                per_user_counts,
            );
        }
        SpeakCommand::Cancel(request_id) => {
            if active_request_id.as_ref() == Some(&request_id) {
                stop_active(
                    SkipReason::Cancelled,
                    deps,
                    event_tx,
                    active_request_id,
                    current_playback,
                    progress_ticker,
                );
            } else {
                remove_queued(
                    &request_id,
                    deps,
                    event_tx,
                    high_queue,
                    normal_queue,
                    per_user_counts,
                );
            }
        }
        SpeakCommand::Reorder { request_id, before } => {
            let anchor_in_high = match &before {
                Some(anchor_id) => {
                    if anchor_id == &request_id {
                        return;
                    }
                    if high_queue.iter().any(|r| &r.request_id == anchor_id) {
                        true
                    } else if normal_queue.iter().any(|r| &r.request_id == anchor_id) {
                        false
                    } else {
                        return;
                    }
                }
                None => false,
            };
            let Some(req) = take_from_queues(high_queue, normal_queue, &request_id) else {
                return;
            };
            let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());
            emit.reordered(&req, before.clone());
            match &before {
                None => normal_queue.push_back(req),
                Some(anchor_id) => {
                    let queue = if anchor_in_high {
                        &mut *high_queue
                    } else {
                        &mut *normal_queue
                    };
                    let pos = queue
                        .iter()
                        .position(|r| &r.request_id == anchor_id)
                        .unwrap_or(queue.len());
                    queue.insert(pos, req);
                }
            }
            emit.feed(queue_changed_event(high_queue, normal_queue));
        }
        SpeakCommand::Clear => {
            stop_active(
                SkipReason::Cleared,
                deps,
                event_tx,
                active_request_id,
                current_playback,
                progress_ticker,
            );
            clear_pending(
                false,
                deps,
                event_tx,
                high_queue,
                normal_queue,
                per_user_counts,
            );
        }
        SpeakCommand::ClearPending => {
            clear_pending(
                true,
                deps,
                event_tx,
                high_queue,
                normal_queue,
                per_user_counts,
            );
        }
        SpeakCommand::SetAlias(alias) => {
            deps.resolver
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .set_alias(alias);
        }
        SpeakCommand::SwitchAlias {
            viewer_id,
            engine_id,
            voice_id,
        } => {
            deps.resolver
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .switch_alias(&viewer_id, engine_id, voice_id);
        }
        SpeakCommand::SetStrategy(strategy) => {
            deps.resolver
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .set_strategy(strategy);
        }
        SpeakCommand::SetVolume(volume) => {
            config.master_volume = volume.clamp(0.0, 1.0);
        }
        SpeakCommand::SetRepeatWindow(window) => {
            config.repeat_window = window;
        }
        SpeakCommand::RemoveAlias(id) => {
            deps.resolver
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .remove_alias(&id);
        }
        SpeakCommand::RefreshVoiceCatalog
        | SpeakCommand::SetEngineEnabled(_, _)
        | SpeakCommand::SetEngineParams(_, _, _) => {}
        SpeakCommand::Pause => {
            *paused = true;
            if let Some(CurrentPlayback::Playing(c)) = current_playback.as_ref() {
                c.playback.pause();
            }
            SpeakEmitter::new(event_tx, deps.event_bus.as_ref()).paused(HoldReason::User);
        }
        SpeakCommand::Resume => {
            *paused = false;
            if let Some(CurrentPlayback::Playing(c)) = current_playback.as_ref() {
                c.playback.resume();
            }
            SpeakEmitter::new(event_tx, deps.event_bus.as_ref()).resumed(HoldReason::User);
        }
        SpeakCommand::Replay => {
            if let Some(last) = last_successful {
                let mut replay = last.clone();
                replay.request_id = RequestId::new();
                replay.priority = Priority::High;
                admit_request(
                    replay,
                    config,
                    deps,
                    event_tx,
                    high_queue,
                    normal_queue,
                    per_user_counts,
                    voice_catalog,
                );
            }
        }
        SpeakCommand::VoiceGateActivated => {
            *voicegate_active = true;
            SpeakEmitter::new(event_tx, deps.event_bus.as_ref()).paused(HoldReason::VoiceGate);
        }
        SpeakCommand::VoiceGateDeactivated => {
            *voicegate_active = false;
            SpeakEmitter::new(event_tx, deps.event_bus.as_ref()).resumed(HoldReason::VoiceGate);
        }
    }
}

fn remove_queued(
    request_id: &RequestId,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    high_queue: &mut VecDeque<SpeakRequest>,
    normal_queue: &mut VecDeque<SpeakRequest>,
    per_user_counts: &mut HashMap<String, usize>,
) {
    let Some(req) = take_from_queues(high_queue, normal_queue, request_id) else {
        return;
    };
    if let Some(count) = per_user_counts.get_mut(&req.viewer_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            per_user_counts.remove(&req.viewer_id);
        }
    }
    let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());
    emit.outcome(&req, Outcome::Removed);
    emit.feed(queue_changed_event(high_queue, normal_queue));
}

fn clear_pending(
    keep_current: bool,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    high_queue: &mut VecDeque<SpeakRequest>,
    normal_queue: &mut VecDeque<SpeakRequest>,
    per_user_counts: &mut HashMap<String, usize>,
) {
    let emit = SpeakEmitter::new(event_tx, deps.event_bus.as_ref());
    for req in high_queue.drain(..).chain(normal_queue.drain(..)) {
        emit.outcome(&req, Outcome::Removed);
    }
    per_user_counts.clear();
    emit.cleared(keep_current);
    emit.feed(queue_changed_event(high_queue, normal_queue));
}

fn pop_next(
    high_queue: &mut VecDeque<SpeakRequest>,
    normal_queue: &mut VecDeque<SpeakRequest>,
    per_user_counts: &mut HashMap<String, usize>,
) -> Option<SpeakRequest> {
    let req = if !high_queue.is_empty() {
        high_queue.pop_front()
    } else {
        normal_queue.pop_front()
    };
    if let Some(ref r) = req {
        let count = per_user_counts.entry(r.viewer_id.clone()).or_insert(0);
        *count = count.saturating_sub(1);
        if *count == 0 {
            per_user_counts.remove(&r.viewer_id);
        }
    }
    req
}

fn spawn_detector_rebuild(
    catalog: &[TtsVoice],
    tx: tokio::sync::mpsc::Sender<Option<Arc<LanguageDetector>>>,
) {
    let candidates = candidate_languages(catalog);
    tokio::spawn(async move {
        let built =
            tokio::task::spawn_blocking(move || LanguageDetector::new(&candidates).map(Arc::new))
                .await;
        let detector = match built {
            Ok(detector) => detector,
            Err(e) => {
                tracing::warn!(error = %e, "language detector build failed");
                None
            }
        };
        if tx.send(detector).await.is_err() {
            tracing::debug!("speak queue stopped before the language detector was rebuilt");
        }
    });
}

fn publish_catalog(
    merged: Arc<Vec<TtsVoice>>,
    voices: &Shared<Vec<TtsVoice>>,
    task_deps: &mut SynthTaskDeps,
    language_aware: bool,
    detector_tx: &tokio::sync::mpsc::Sender<Option<Arc<LanguageDetector>>>,
) {
    voices.store_arc(merged.clone());
    if language_aware {
        spawn_detector_rebuild(&merged, detector_tx.clone());
    }
    task_deps.voice_catalog = merged;
}

fn next_request<'a>(
    high_queue: &'a VecDeque<SpeakRequest>,
    normal_queue: &'a VecDeque<SpeakRequest>,
) -> Option<&'a SpeakRequest> {
    high_queue.front().or_else(|| normal_queue.front())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use forge_audio::{AudioError, AudioSink};
    use forge_events::{Event, EventPublisher};
    use forge_speech_types::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults};

    use super::*;

    struct SilentSink;
    #[async_trait]
    impl AudioSink for SilentSink {
        async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
            Ok(())
        }
    }

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn minimal_deps() -> QueueDeps {
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        QueueDeps {
            registry: Arc::new(std::sync::RwLock::new(forge_tts_core::TtsRegistry::new())),
            resolver: Arc::new(std::sync::RwLock::new(resolver)),
            pipeline: crate::PipelineConfigHandle::new(
                forge_tts_pipeline::PipelineConfig::default(),
            ),
            audio_sink: Arc::new(SilentSink),
            event_bus: Arc::new(NullPublisher),
            disabled_engines: HashSet::new(),
            engine_gains: HashMap::new(),
        }
    }

    fn dispatch(
        config: &mut QueueConfig,
        deps: &QueueDeps,
        tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
        cmd: SpeakCommand,
    ) {
        let mut high: VecDeque<SpeakRequest> = VecDeque::new();
        let mut normal: VecDeque<SpeakRequest> = VecDeque::new();
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut paused = false;
        let mut voicegate = false;
        let mut active: Option<RequestId> = None;
        let last: Option<SpeakRequest> = None;
        let mut current_playback: Option<CurrentPlayback> = None;
        let mut progress_ticker: Option<tokio::time::Interval> = None;
        handle_command(
            cmd,
            config,
            deps,
            tx,
            &mut high,
            &mut normal,
            &mut counts,
            &mut paused,
            &mut voicegate,
            &mut active,
            &last,
            &mut current_playback,
            &mut progress_ticker,
            &[],
        );
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

    struct RecordingPublisher {
        events: Arc<std::sync::Mutex<Vec<Event>>>,
    }
    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn recording_deps() -> (QueueDeps, Arc<std::sync::Mutex<Vec<Event>>>) {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let deps = QueueDeps {
            event_bus: Arc::new(RecordingPublisher {
                events: Arc::clone(&events),
            }),
            ..minimal_deps()
        };
        (deps, events)
    }

    struct Harness {
        config: QueueConfig,
        deps: QueueDeps,
        events: Arc<std::sync::Mutex<Vec<Event>>>,
        tx: tokio::sync::broadcast::Sender<SpeakEvent>,
        rx: tokio::sync::broadcast::Receiver<SpeakEvent>,
        high: VecDeque<SpeakRequest>,
        normal: VecDeque<SpeakRequest>,
        counts: HashMap<String, usize>,
        paused: bool,
        voicegate: bool,
        active: Option<RequestId>,
        last: Option<SpeakRequest>,
        current_playback: Option<CurrentPlayback>,
        progress_ticker: Option<tokio::time::Interval>,
    }

    impl Harness {
        fn new() -> Self {
            let (deps, events) = recording_deps();
            let (tx, rx) = tokio::sync::broadcast::channel::<SpeakEvent>(32);
            Self {
                config: QueueConfig::default(),
                deps,
                events,
                tx,
                rx,
                high: VecDeque::new(),
                normal: VecDeque::new(),
                counts: HashMap::new(),
                paused: false,
                voicegate: false,
                active: None,
                last: None,
                current_playback: None,
                progress_ticker: None,
            }
        }

        fn run(&mut self, cmd: SpeakCommand) {
            handle_command(
                cmd,
                &mut self.config,
                &self.deps,
                &self.tx,
                &mut self.high,
                &mut self.normal,
                &mut self.counts,
                &mut self.paused,
                &mut self.voicegate,
                &mut self.active,
                &self.last,
                &mut self.current_playback,
                &mut self.progress_ticker,
                &[],
            );
        }

        fn bus(&self, kind: &str) -> Event {
            self.events
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.kind == kind)
                .cloned()
                .unwrap_or_else(|| panic!("no bus event of kind {kind}"))
        }

        fn published(&self, kind: &str) -> bool {
            self.events.lock().unwrap().iter().any(|e| e.kind == kind)
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

        assert_eq!(texts(&h.normal), vec!["b", "a", "c"]);
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

        assert_eq!(texts(&h.normal), vec!["b", "c", "a"]);
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

        assert_eq!(texts(&h.high), vec!["h1", "n2", "h2"]);
        assert_eq!(texts(&h.normal), vec!["n1"]);
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

            assert_eq!(texts(&h.normal), vec!["a", "b", "c"], "{case}");
            assert!(h.high.is_empty(), "{case}");
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
        let deps = minimal_deps();
        let (tx, _rx) = tokio::sync::broadcast::channel::<SpeakEvent>(8);
        for (input, expected) in [
            (1.5_f32, 1.0_f32),
            (-0.2, 0.0),
            (0.5, 0.5),
            (0.0, 0.0),
            (1.0, 1.0),
        ] {
            let mut config = QueueConfig::default();
            dispatch(&mut config, &deps, &tx, SpeakCommand::SetVolume(input));
            assert_eq!(
                config.master_volume, expected,
                "SetVolume({input}) must set master_volume to {expected}",
            );
        }
    }

    #[test]
    fn enqueue_emits_event_carrying_request_payload() {
        let deps = minimal_deps();
        let mut config = QueueConfig::default();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<SpeakEvent>(8);
        dispatch(
            &mut config,
            &deps,
            &tx,
            SpeakCommand::Enqueue(request("nova", "hi chat", Priority::High)),
        );
        let payload = std::iter::from_fn(|| rx.try_recv().ok())
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
        fn capabilities(&self) -> &forge_tts_core::EngineCapabilities {
            static CAPS: forge_tts_core::EngineCapabilities = forge_tts_core::EngineCapabilities {
                ssml: false,
                neural_voices: false,
                streaming: false,
                custom_lexicons: false,
            };
            &CAPS
        }
        async fn list_voices(&self) -> Result<Vec<TtsVoice>, forge_tts_core::TtsError> {
            Ok(vec![])
        }
        async fn synthesize(
            &self,
            req: SynthesisRequest,
        ) -> Result<PcmBuffer, forge_tts_core::TtsError> {
            self.seen.lock().unwrap().push(req.text);
            Ok(PcmBuffer::new(vec![0i16; 4], 22_050, 1))
        }
    }

    struct CapturingFactory {
        id: EngineId,
        seen: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl forge_tts_core::TtsEngineFactory for CapturingFactory {
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
        registry.register(
            engine_id.clone(),
            Arc::new(CapturingFactory {
                id: engine_id,
                seen: seen.clone(),
            }),
        );
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let deps = SynthTaskDeps {
            resolver: Arc::new(std::sync::RwLock::new(resolver)),
            pipeline: crate::PipelineConfigHandle::new(cfg),
            registry: Arc::new(std::sync::RwLock::new(registry)),
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
            ..Default::default()
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
            let mut cfg = forge_tts_pipeline::PipelineConfig::default();
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
            let mut cfg = crate::build_config_lenient(&[], &settings);
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
            let mut cfg = forge_tts_pipeline::PipelineConfig::default();
            cfg.emote_sources.twitch = true;
            cfg
        };
        let skip_emote_only = || {
            let mut cfg = forge_tts_pipeline::PipelineConfig::default();
            cfg.skip_rules.emote_only = true;
            cfg
        };
        let strip_reward = || forge_tts_pipeline::PipelineConfig {
            strip_reward_emotes: true,
            ..Default::default()
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
                forge_tts_pipeline::PipelineConfig::default(),
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
            h.config = config;
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
        h.last = Some(last);

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
                PipelineSkip::MatchedSkipRule("no urls"),
                (SkipReason::SkipRuleMatched, Some("no urls".to_owned())),
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
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.kind.clone(), e.payload.get("text").cloned(), e.caused_by))
            .collect()
    }

    fn source_of(h: &Harness, text: &str) -> Option<forge_types::EventId> {
        h.high
            .iter()
            .chain(h.normal.iter())
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
        h.events.lock().unwrap().clear();
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
        h.active = Some(current.clone());
        stage_mixed_pending(&mut h);

        h.run(SpeakCommand::ClearPending);

        assert_eq!(h.active, Some(current));
        assert!(!h.published("speak.skipped"));
    }

    #[test]
    fn clear_stops_the_in_flight_message_then_reports_each_pending_removed_then_cleared() {
        let mut h = Harness::new();
        h.active = Some(RequestId::new());
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
        assert!(h.active.is_none());
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
        h.active = Some(in_flight.clone());

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
}
