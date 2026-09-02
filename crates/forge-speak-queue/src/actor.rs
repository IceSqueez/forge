use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use forge_audio::PcmBuffer;
use forge_events::{Event, EventPublisher, EventSource};
use forge_tts_core::{EngineId, SynthesisRequest, TtsVoice, VoiceId};
use forge_tts_pipeline::{DetectionOutcome, LanguageCode, LanguageDetector, PipelineResult};
use forge_types::Shared;
use forge_voice::{
    AliasState, ResolveResult, VoiceAliasResolver, candidate_languages, voice_speaks_language,
};

use crate::{
    PipelineConfigHandle, Priority, QueueConfig, QueueDeps, QueuedOrderEntry, RequestId,
    SpeakCommand, SpeakEvent, SpeakRequest,
};

struct SynthTaskResult {
    request_id: RequestId,
    request: SpeakRequest,
    outcome: SynthOutcome,
}

struct CurrentPlayback {
    request_id: RequestId,
    request: SpeakRequest,
    playback: forge_audio::ControlledPlayback,
    elapsed_secs: u32,
}

async fn poll_current(
    current: &mut Option<CurrentPlayback>,
) -> Result<(), forge_audio::AudioError> {
    match current {
        Some(c) => (&mut c.playback).await,
        None => std::future::pending().await,
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

struct DetectedLanguage {
    code: LanguageCode,
    confidence: f64,
}

enum SynthOutcome {
    Speak {
        pcm: PcmBuffer,
        voice_id: VoiceId,
        engine_id: EngineId,
        /// Set only when the guess narrowed the candidate voices.
        language: Option<DetectedLanguage>,
    },
    Skipped {
        human: String,
        token: String,
        detail: Option<String>,
    },
    Failed {
        reason: &'static str,
        detail: String,
    },
}

fn pipeline_skip_token(reason: &forge_tts_pipeline::SkipReason) -> (String, Option<String>) {
    use forge_tts_pipeline::SkipReason;
    match reason {
        SkipReason::MatchedSkipRule(name) => {
            ("skip_rule_matched".to_owned(), Some((*name).to_owned()))
        }
        SkipReason::BlockedByWordFilter => ("blocked_by_word_filter".to_owned(), None),
        SkipReason::EmptyAfterProcessing => ("empty_after_processing".to_owned(), None),
    }
}

fn resolve_skip_token(reason: &'static str) -> &'static str {
    match reason {
        "blocked by alias" => "blocked_by_alias",
        "no voices available" => "no_voices_available",
        "voice override not found in catalog" => "voice_override_not_found",
        _ => "voice_resolution_failed",
    }
}

struct SynthTaskDeps {
    resolver: Arc<std::sync::RwLock<forge_voice::VoiceAliasResolver>>,
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
    let pipeline_cfg = deps.pipeline.load();

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
            let (token, detail) = pipeline_skip_token(&reason);
            return SynthTaskResult {
                request_id: req.request_id.clone(),
                request: req,
                outcome: SynthOutcome::Skipped {
                    human: format!("{reason:?}"),
                    token,
                    detail,
                },
            };
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
        resolve_with_overrides(
            &guard,
            &req,
            &deps.voice_catalog,
            detected.as_ref().map(|d| d.code),
        )
    };

    let (voice_id, engine_id, pitch, rate) = match resolve_result {
        ResolveResult::Speak {
            voice_id,
            engine_id,
            pitch,
            rate,
        } => (voice_id, engine_id, pitch, rate),
        ResolveResult::Skip { reason } => {
            return SynthTaskResult {
                request_id: req.request_id.clone(),
                request: req,
                outcome: SynthOutcome::Skipped {
                    human: reason.to_owned(),
                    token: resolve_skip_token(reason).to_owned(),
                    detail: None,
                },
            };
        }
    };

    let factory = match deps
        .registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&engine_id)
    {
        Some(f) => f,
        None => {
            return SynthTaskResult {
                request_id: req.request_id.clone(),
                request: req,
                outcome: SynthOutcome::Failed {
                    reason: "engine_error",
                    detail: format!("engine {:?} not registered", engine_id),
                },
            };
        }
    };

    let engine = match factory.create() {
        Ok(e) => e,
        Err(e) => {
            return SynthTaskResult {
                request_id: req.request_id.clone(),
                request: req,
                outcome: SynthOutcome::Failed {
                    reason: "engine_error",
                    detail: e.to_string(),
                },
            };
        }
    };

    let synthesis_req = SynthesisRequest {
        text: text_to_speak,
        voice_id: voice_id.clone(),
        pitch_semitones: pitch,
        rate_multiplier: rate,
        ssml: false,
    };

    match engine.synthesize(synthesis_req).await {
        Ok(pcm) => SynthTaskResult {
            request_id: req.request_id.clone(),
            request: req,
            outcome: SynthOutcome::Speak {
                pcm,
                voice_id,
                engine_id,
                language: detected,
            },
        },
        Err(e) => SynthTaskResult {
            request_id: req.request_id.clone(),
            request: req,
            outcome: SynthOutcome::Failed {
                reason: "engine_error",
                detail: e.to_string(),
            },
        },
    }
}

/// `None` unless the guess is confident AND some installed voice serves it - a guess that
/// no voice can honour must leave resolution against the full catalog, not skip the message.
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

fn scope_to_engine(catalog: &[TtsVoice], engine_id: &EngineId) -> Vec<TtsVoice> {
    catalog
        .iter()
        .filter(|v| &v.engine_id == engine_id)
        .cloned()
        .collect()
}

fn resolve_with_overrides(
    resolver: &VoiceAliasResolver,
    req: &SpeakRequest,
    catalog: &[TtsVoice],
    language: Option<LanguageCode>,
) -> ResolveResult {
    if let Some(voice_id) = &req.voice_override {
        let engine_id = req.engine_override.clone().or_else(|| {
            catalog
                .iter()
                .find(|v| &v.id == voice_id)
                .map(|v| v.engine_id.clone())
        });
        return match engine_id {
            Some(engine_id) => {
                let engine_defaults = resolver.defaults_for(&engine_id);
                ResolveResult::Speak {
                    voice_id: voice_id.clone(),
                    engine_id,
                    pitch: engine_defaults.pitch_semitones,
                    rate: engine_defaults.rate_multiplier,
                }
            }
            None => ResolveResult::Skip {
                reason: "voice override not found in catalog",
            },
        };
    }

    let language_scoped = language.and_then(|code| {
        let scoped: Vec<TtsVoice> = catalog
            .iter()
            .filter(|v| voice_speaks_language(v, code))
            .cloned()
            .collect();
        (!scoped.is_empty()).then_some(scoped)
    });
    let candidates: &[TtsVoice] = language_scoped.as_deref().unwrap_or(catalog);

    if let Some(engine_id) = &req.engine_override {
        let mut scoped = scope_to_engine(candidates, engine_id);
        if scoped.is_empty() {
            // An inferred language must never empty an explicitly requested engine.
            scoped = scope_to_engine(catalog, engine_id);
        }
        return resolver.resolve(&req.viewer_id, &req.viewer_name, &scoped);
    }

    resolver.resolve(&req.viewer_id, &req.viewer_name, candidates)
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

fn publish(
    bus: &dyn EventPublisher,
    kind: &str,
    payload: serde_json::Value,
    caused_by: Option<forge_types::EventId>,
) {
    let event = match caused_by {
        Some(parent) => Event::caused_by(EventSource::Audio, kind, payload, parent),
        None => Event::new(EventSource::Audio, kind, payload),
    };
    bus.publish(event);
}

#[allow(clippy::too_many_arguments)]
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
    let (catalog_tx, mut catalog_rx) = tokio::sync::mpsc::channel::<Arc<Vec<TtsVoice>>>(1);
    let (detector_tx, mut detector_rx) =
        tokio::sync::mpsc::channel::<Option<Arc<LanguageDetector>>>(1);

    let voice_catalog = Arc::new(build_voice_catalog(&deps.registry, &deps.disabled_engines).await);
    voices.store_arc(voice_catalog.clone());
    disabled_engines.store(deps.disabled_engines.clone());
    engine_gains.store(deps.engine_gains.clone());
    let mut task_deps = SynthTaskDeps {
        resolver: deps.resolver.clone(),
        pipeline: deps.pipeline.clone(),
        registry: deps.registry.clone(),
        voice_catalog: voice_catalog.clone(),
        detector: None,
    };

    let mut language_aware = deps.pipeline.load().output.language_aware_voice;
    if language_aware {
        spawn_detector_rebuild(&voice_catalog, detector_tx.clone());
    }

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

        if active_request_id.is_none()
            && !paused
            && !voicegate_active
            && let Some(req) = pop_next(&mut high_queue, &mut normal_queue, &mut per_user_counts)
        {
            let queue_len = high_queue.len() + normal_queue.len();
            let _ = event_tx.send(queue_changed_event(&high_queue, &normal_queue));
            let _ = event_tx.send(SpeakEvent::Started {
                request_id: req.request_id.clone(),
                voice_id: VoiceId(String::new()),
                engine_id: EngineId(String::new()),
                viewer_name: req.viewer_name.clone(),
                text: req.text.clone(),
                duration_secs: 0,
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.started",
                serde_json::json!({
                    "request_id": req.request_id.0,
                    "voice_id": null,
                    "engine_id": null,
                    "queue_len": queue_len,
                    "viewer_name": req.viewer_name,
                    "text": req.text,
                    "detected_language": null,
                    "language_confidence": null,
                }),
                req.source_event_id,
            );
            active_request_id = Some(req.request_id.clone());
            let tx = synth_tx.clone();
            let task_deps_clone = SynthTaskDeps {
                resolver: task_deps.resolver.clone(),
                pipeline: task_deps.pipeline.clone(),
                registry: task_deps.registry.clone(),
                voice_catalog: task_deps.voice_catalog.clone(),
                detector: task_deps.detector.clone(),
            };
            let recent_snapshot: Vec<String> = recent_messages.iter().cloned().collect();
            let window = deps.pipeline.load().skip_rules.window.max(1);
            while recent_messages.len() >= window {
                recent_messages.pop_front();
            }
            recent_messages.push_back(req.text.clone());
            tokio::spawn(async move {
                let result = run_synthesis(req, task_deps_clone, recent_snapshot).await;
                let _ = tx.send(result).await;
            });
        }

        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                    Some(SpeakCommand::RefreshVoiceCatalog) => {
                        spawn_catalog_rebuild(
                            deps.registry.clone(),
                            (*disabled_engines.load()).clone(),
                            catalog_tx.clone(),
                        );
                    }
                    Some(SpeakCommand::SetEngineEnabled(engine_id, enabled)) => {
                        let mut next = (*disabled_engines.load()).clone();
                        if enabled {
                            next.remove(&engine_id);
                        } else {
                            next.insert(engine_id);
                        }
                        disabled_engines.store(next.clone());
                        spawn_catalog_rebuild(deps.registry.clone(), next, catalog_tx.clone());
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
                            guard.engine_defaults.insert(engine_id.clone(), defaults);
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
                        &high_queue,
                        &normal_queue,
                        gains.as_ref(),
                    ).await;
                }
            }
            result = catalog_rx.recv() => {
                if let Some(catalog) = result {
                    voices.store_arc(catalog.clone());
                    if language_aware {
                        spawn_detector_rebuild(&catalog, detector_tx.clone());
                    }
                    task_deps.voice_catalog = catalog;
                }
            }
            result = detector_rx.recv() => {
                if let Some(detector) = result {
                    task_deps.detector = detector.filter(|_| language_aware);
                }
            }
            _ = tick_progress(&mut progress_ticker), if progress_ticker.is_some() => {
                if !paused && let Some(c) = current_playback.as_mut() {
                    c.elapsed_secs += 1;
                    let _ = event_tx.send(SpeakEvent::Progress {
                        request_id: c.request_id.clone(),
                        elapsed_secs: c.elapsed_secs,
                    });
                }
            }
            play_result = poll_current(&mut current_playback), if current_playback.is_some() => {
                if let Some(current) = current_playback.take() {
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

#[allow(clippy::too_many_arguments)]
async fn handle_synth_result(
    result: SynthTaskResult,
    config: &QueueConfig,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
    high_queue: &VecDeque<SpeakRequest>,
    normal_queue: &VecDeque<SpeakRequest>,
    engine_gains: &HashMap<EngineId, f32>,
) {
    if active_request_id.as_ref() != Some(&result.request_id) {
        return;
    }
    let queue_len = high_queue.len() + normal_queue.len();

    match result.outcome {
        SynthOutcome::Skipped {
            human,
            token,
            detail,
        } => {
            *active_request_id = None;
            tracing::debug!(request_id = %result.request_id.0, reason = %token, "speak skipped");
            let _ = event_tx.send(SpeakEvent::Skipped {
                request_id: result.request_id.clone(),
                reason: human,
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.skipped",
                serde_json::json!({
                    "request_id": result.request_id.0,
                    "reason": token,
                    "detail": detail,
                    "viewer_name": result.request.viewer_name,
                    "text": result.request.text,
                }),
                result.request.source_event_id,
            );
        }
        SynthOutcome::Failed { reason, detail } => {
            *active_request_id = None;
            tracing::warn!(request_id = %result.request_id.0, error = %detail, "speak failed");
            let _ = event_tx.send(SpeakEvent::Failed {
                request_id: result.request_id.clone(),
                error: detail.clone(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.failed",
                serde_json::json!({
                    "request_id": result.request_id.0,
                    "reason": reason,
                    "error": detail,
                    "viewer_name": result.request.viewer_name,
                    "text": result.request.text,
                }),
                result.request.source_event_id,
            );
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
            let duration_secs = (pcm.duration_ms() / 1000) as u32;
            let _ = event_tx.send(SpeakEvent::Started {
                request_id: result.request_id.clone(),
                voice_id: voice_id.clone(),
                engine_id: engine_id.clone(),
                viewer_name: result.request.viewer_name.clone(),
                text: result.request.text.clone(),
                duration_secs,
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.started",
                serde_json::json!({
                    "request_id": result.request_id.0,
                    "voice_id": voice_id.0,
                    "engine_id": engine_id.0,
                    "queue_len": queue_len,
                    "viewer_name": result.request.viewer_name,
                    "text": result.request.text,
                    "detected_language": language.as_ref().map(|l| l.code.to_string()),
                    "language_confidence": language.as_ref().map(|l| l.confidence),
                }),
                result.request.source_event_id,
            );
            let engine_gain = engine_gains.get(&engine_id).copied().unwrap_or(1.0);
            pcm.apply_gain(config.master_volume * engine_gain);
            match deps.audio_sink.play_controlled(pcm).await {
                Ok(playback) => {
                    let mut ticker = tokio::time::interval(Duration::from_secs(1));
                    ticker.tick().await;
                    *progress_ticker = Some(ticker);
                    *current_playback = Some(CurrentPlayback {
                        request_id: result.request_id,
                        request: result.request,
                        playback,
                        elapsed_secs: 0,
                    });
                }
                Err(e) => {
                    *active_request_id = None;
                    tracing::warn!(request_id = %result.request_id.0, error = %e, "audio playback failed to start");
                    let _ = event_tx.send(SpeakEvent::Failed {
                        request_id: result.request_id.clone(),
                        error: e.to_string(),
                    });
                    publish(
                        deps.event_bus.as_ref(),
                        "speak.failed",
                        serde_json::json!({
                            "request_id": result.request_id.0,
                            "reason": "device_error",
                            "error": e.to_string(),
                            "viewer_name": result.request.viewer_name,
                            "text": result.request.text,
                        }),
                        result.request.source_event_id,
                    );
                }
            }
        }
    }
}

async fn finish_playback(
    play_result: Result<(), forge_audio::AudioError>,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    last_successful: &mut Option<SpeakRequest>,
    current: CurrentPlayback,
) {
    *active_request_id = None;

    match play_result {
        Ok(()) => {
            let _ = event_tx.send(SpeakEvent::Finished {
                request_id: current.request_id.clone(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.finished",
                serde_json::json!({
                    "request_id": current.request_id.0,
                    "viewer_name": current.request.viewer_name,
                    "text": current.request.text,
                }),
                current.request.source_event_id,
            );
            *last_successful = Some(current.request);
        }
        Err(e) => {
            tracing::warn!(request_id = %current.request_id.0, error = %e, "audio playback failed");
            let _ = event_tx.send(SpeakEvent::Failed {
                request_id: current.request_id.clone(),
                error: e.to_string(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.failed",
                serde_json::json!({
                    "request_id": current.request_id.0,
                    "reason": "device_error",
                    "error": e.to_string(),
                    "viewer_name": current.request.viewer_name,
                    "text": current.request.text,
                }),
                current.request.source_event_id,
            );
        }
    }
}

fn stop_active(
    human_reason: &str,
    reason_token: &'static str,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
) {
    let Some(request_id) = active_request_id.take() else {
        return;
    };
    let (viewer_name, text, caused_by) = match current_playback.take() {
        Some(current) => {
            current.playback.stop();
            (
                Some(current.request.viewer_name.clone()),
                Some(current.request.text.clone()),
                current.request.source_event_id,
            )
        }
        None => (None, None, None),
    };
    *progress_ticker = None;
    let _ = event_tx.send(SpeakEvent::Skipped {
        request_id: request_id.clone(),
        reason: human_reason.to_owned(),
    });
    publish(
        deps.event_bus.as_ref(),
        "speak.skipped",
        serde_json::json!({
            "request_id": request_id.0,
            "reason": reason_token,
            "detail": null,
            "viewer_name": viewer_name,
            "text": text,
        }),
        caused_by,
    );
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

fn enqueue_preview(req: &SpeakRequest, deps: &QueueDeps, catalog: &[TtsVoice]) -> (String, u32) {
    let estimated_secs = ((req.text.chars().count() as u32) / 15).max(1);
    let guard = deps.resolver.read().unwrap_or_else(|e| e.into_inner());
    let preview = match resolve_with_overrides(&guard, req, catalog, None) {
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
            format!("{} \u{b7} {}", engine_label(&engine_id.0), name)
        }
        ResolveResult::Skip { .. } => String::new(),
    };
    (preview, estimated_secs)
}

fn engine_label(id: &str) -> String {
    match id {
        "piper" => "Piper",
        "espeak-ng" => "eSpeak-NG",
        "sapi" => "Microsoft SAPI 5",
        "nsspeech" => "Apple AVSpeech",
        "azure" => "Azure Speech",
        "elevenlabs" => "ElevenLabs",
        "openai" => "OpenAI TTS",
        "polly" => "Amazon Polly",
        other => return other.to_owned(),
    }
    .to_owned()
}

#[allow(clippy::too_many_arguments)]
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
            let total = high_queue.len() + normal_queue.len();
            if total >= config.max_queue_len {
                tracing::debug!(request_id = %req.request_id.0, "queue full, rejecting");
                let _ = event_tx.send(SpeakEvent::Rejected {
                    request_id: req.request_id.clone(),
                    reason: format!("queue full (max {})", config.max_queue_len),
                });
                publish(
                    deps.event_bus.as_ref(),
                    "speak.rejected",
                    serde_json::json!({
                        "request_id": req.request_id.0,
                        "reason": "queue_full",
                        "limit": config.max_queue_len,
                        "queue_len": total,
                        "viewer_name": req.viewer_name,
                        "text": req.text,
                    }),
                    req.source_event_id,
                );
                return;
            }
            let user_count = per_user_counts.get(&req.viewer_id).copied().unwrap_or(0);
            if user_count >= config.per_user_limit {
                tracing::debug!(viewer_id = %req.viewer_id, "per-user limit reached");
                let _ = event_tx.send(SpeakEvent::Rejected {
                    request_id: req.request_id.clone(),
                    reason: format!("per-user limit (max {})", config.per_user_limit),
                });
                publish(
                    deps.event_bus.as_ref(),
                    "speak.rejected",
                    serde_json::json!({
                        "request_id": req.request_id.0,
                        "reason": "per_user_limit",
                        "limit": config.per_user_limit,
                        "queue_len": total,
                        "viewer_name": req.viewer_name,
                        "text": req.text,
                    }),
                    req.source_event_id,
                );
                return;
            }
            *per_user_counts.entry(req.viewer_id.clone()).or_insert(0) += 1;
            let total_after = total + 1;
            let (voice_preview, estimated_secs) = enqueue_preview(&req, deps, voice_catalog);
            match req.priority {
                Priority::High => high_queue.push_back(req.clone()),
                Priority::Normal => normal_queue.push_back(req.clone()),
            }
            let _ = event_tx.send(SpeakEvent::Enqueued {
                request_id: req.request_id.clone(),
                queue_len: total_after,
                viewer_name: req.viewer_name.clone(),
                text: req.text.clone(),
                is_high_priority: matches!(req.priority, Priority::High),
                voice_preview,
                estimated_secs,
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.enqueued",
                serde_json::json!({
                    "request_id": req.request_id.0,
                    "queue_len": total_after,
                    "viewer_name": req.viewer_name,
                    "text": req.text,
                }),
                req.source_event_id,
            );
            let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
        }
        SpeakCommand::Skip => {
            stop_active(
                "skipped by user",
                "user_skip",
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
                    "promoted by user",
                    "promoted",
                    deps,
                    event_tx,
                    active_request_id,
                    current_playback,
                    progress_ticker,
                );
                let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
            }
        }
        SpeakCommand::RemoveQueued(request_id) => {
            if let Some(req) = take_from_queues(high_queue, normal_queue, &request_id) {
                if let Some(count) = per_user_counts.get_mut(&req.viewer_id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        per_user_counts.remove(&req.viewer_id);
                    }
                }
                let _ = event_tx.send(SpeakEvent::Removed {
                    request_id: request_id.clone(),
                });
                publish(
                    deps.event_bus.as_ref(),
                    "speak.removed",
                    serde_json::json!({
                        "request_id": request_id.0,
                        "viewer_name": req.viewer_name,
                        "text": req.text,
                    }),
                    req.source_event_id,
                );
                let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
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
            let viewer_name = req.viewer_name.clone();
            let text = req.text.clone();
            let source_event_id = req.source_event_id;
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
            let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
            publish(
                deps.event_bus.as_ref(),
                "speak.reordered",
                serde_json::json!({
                    "request_id": request_id.0,
                    "before": before.map(|id| id.0),
                    "viewer_name": viewer_name,
                    "text": text,
                }),
                source_event_id,
            );
        }
        SpeakCommand::Clear => {
            stop_active(
                "stopped by clear",
                "cleared",
                deps,
                event_tx,
                active_request_id,
                current_playback,
                progress_ticker,
            );
            high_queue.clear();
            normal_queue.clear();
            per_user_counts.clear();
            let _ = event_tx.send(SpeakEvent::Cleared);
            publish(
                deps.event_bus.as_ref(),
                "speak.cleared",
                serde_json::json!({ "keep_current": false }),
                None,
            );
            let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
        }
        SpeakCommand::ClearPending => {
            high_queue.clear();
            normal_queue.clear();
            per_user_counts.clear();
            publish(
                deps.event_bus.as_ref(),
                "speak.cleared",
                serde_json::json!({ "keep_current": true }),
                None,
            );
            let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
        }
        SpeakCommand::SetAlias(alias) => {
            let mut guard = deps.resolver.write().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = guard
                .aliases
                .iter_mut()
                .find(|a| a.viewer_id == alias.viewer_id)
            {
                *existing = alias;
            } else {
                guard.aliases.push(alias);
            }
        }
        SpeakCommand::SwitchAlias {
            viewer_id,
            engine_id,
            voice_id,
        } => {
            let mut guard = deps.resolver.write().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = guard.aliases.iter_mut().find(|a| a.viewer_id == viewer_id) {
                existing.engine_id = engine_id;
                existing.voice_id = voice_id;
                existing.state = AliasState::Active;
            }
        }
        SpeakCommand::SetStrategy(strategy) => {
            let mut guard = deps.resolver.write().unwrap_or_else(|e| e.into_inner());
            guard.strategy = strategy;
        }
        SpeakCommand::SetVolume(volume) => {
            config.master_volume = volume.clamp(0.0, 1.0);
        }
        SpeakCommand::RemoveAlias(id) => {
            let mut guard = deps.resolver.write().unwrap_or_else(|e| e.into_inner());
            guard.aliases.retain(|a| a.id != id);
        }
        SpeakCommand::RefreshVoiceCatalog
        | SpeakCommand::SetEngineEnabled(_, _)
        | SpeakCommand::SetEngineParams(_, _, _) => {}
        SpeakCommand::Pause => {
            *paused = true;
            if let Some(c) = current_playback.as_ref() {
                c.playback.pause();
            }
            let _ = event_tx.send(SpeakEvent::Paused {
                reason: "user paused".into(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.paused",
                serde_json::json!({ "reason": "user" }),
                None,
            );
        }
        SpeakCommand::Resume => {
            *paused = false;
            if let Some(c) = current_playback.as_ref() {
                c.playback.resume();
            }
            let _ = event_tx.send(SpeakEvent::Resumed);
            publish(
                deps.event_bus.as_ref(),
                "speak.resumed",
                serde_json::json!({ "reason": "user" }),
                None,
            );
        }
        SpeakCommand::Replay => {
            if let Some(last) = last_successful {
                let mut replay = last.clone();
                replay.request_id = RequestId::new();
                replay.priority = Priority::High;
                let total = high_queue.len() + normal_queue.len() + 1;
                let (voice_preview, estimated_secs) = enqueue_preview(&replay, deps, voice_catalog);
                high_queue.push_back(replay.clone());
                let _ = event_tx.send(SpeakEvent::Enqueued {
                    request_id: replay.request_id.clone(),
                    queue_len: total,
                    viewer_name: replay.viewer_name.clone(),
                    text: replay.text.clone(),
                    is_high_priority: true,
                    voice_preview,
                    estimated_secs,
                });
                publish(
                    deps.event_bus.as_ref(),
                    "speak.enqueued",
                    serde_json::json!({
                        "request_id": replay.request_id.0,
                        "queue_len": total,
                        "viewer_name": replay.viewer_name,
                        "text": replay.text,
                    }),
                    replay.source_event_id,
                );
                let _ = event_tx.send(queue_changed_event(high_queue, normal_queue));
            }
        }
        SpeakCommand::VoiceGateActivated => {
            *voicegate_active = true;
            let _ = event_tx.send(SpeakEvent::VoiceGateHeld);
            publish(
                deps.event_bus.as_ref(),
                "speak.paused",
                serde_json::json!({ "reason": "voicegate" }),
                None,
            );
        }
        SpeakCommand::VoiceGateDeactivated => {
            *voicegate_active = false;
            let _ = event_tx.send(SpeakEvent::VoiceGateReleased);
            publish(
                deps.event_bus.as_ref(),
                "speak.resumed",
                serde_json::json!({ "reason": "voicegate" }),
                None,
            );
        }
    }
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
        let _ = tx.send(detector).await;
    });
}

fn spawn_catalog_rebuild(
    registry: Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>,
    disabled: HashSet<EngineId>,
    tx: tokio::sync::mpsc::Sender<Arc<Vec<TtsVoice>>>,
) {
    tokio::spawn(async move {
        let catalog = Arc::new(build_voice_catalog(&registry, &disabled).await);
        let _ = tx.send(catalog).await;
    });
}

async fn build_voice_catalog(
    registry: &std::sync::RwLock<forge_tts_core::TtsRegistry>,
    disabled: &HashSet<EngineId>,
) -> Vec<TtsVoice> {
    let engine_ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();
    let mut catalog = Vec::new();
    for engine_id in engine_ids {
        if disabled.contains(&engine_id) {
            continue;
        }
        let factory = registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&engine_id);
        if let Some(factory) = factory {
            match factory.create() {
                Ok(engine) => match engine.list_voices().await {
                    Ok(voices) => catalog.extend(voices),
                    Err(e) => tracing::warn!(engine = %engine_id.0, "list_voices: {e}"),
                },
                Err(e) => tracing::warn!(engine = %engine_id.0, "create engine: {e}"),
            }
        }
    }
    catalog
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use async_trait::async_trait;
    use forge_audio::{AudioError, AudioSink};
    use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults};

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
            alias_override: None,
            engine_override: None,
            voice_override: None,
            source_event_id: Some(forge_types::EventId::new()),
            is_reward: false,
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

    #[tokio::test]
    async fn reward_emote_strip_fires_only_when_reward_and_toggle_both_set() {
        for (is_reward, strip_reward_emotes, expected) in [
            (true, true, "hi"),
            (false, true, "hi LUL"),
            (true, false, "hi LUL"),
            (false, false, "hi LUL"),
        ] {
            let mut emote_tokens = forge_tts_pipeline::EmoteTokenSet::default();
            emote_tokens.tokens.insert("LUL".into());
            let cfg = forge_tts_pipeline::PipelineConfig {
                emote_tokens,
                strip_reward_emotes,
                ..Default::default()
            };
            let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
            let engine_id = EngineId("cap".into());
            let voice = TtsVoice {
                id: VoiceId("cap-1".into()),
                name: "cap-1".into(),
                locale: "en-US".into(),
                gender: forge_tts_core::VoiceGender::Neutral,
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

            let mut req = request("nova", "hi LUL", Priority::Normal);
            req.voice_override = Some(voice.id.clone());
            req.is_reward = is_reward;

            let result = run_synthesis(req, deps, Vec::new()).await;
            assert!(
                matches!(result.outcome, SynthOutcome::Speak { .. }),
                "expected Speak for is_reward={is_reward} toggle={strip_reward_emotes}",
            );
            let spoken = seen.lock().unwrap().clone();
            assert_eq!(
                spoken,
                vec![expected.to_owned()],
                "is_reward={is_reward} strip_reward_emotes={strip_reward_emotes}",
            );
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
    fn cleared_bus_event_always_carries_keep_current_flag() {
        for (cmd, keep_current) in [
            (SpeakCommand::Clear, false),
            (SpeakCommand::ClearPending, true),
        ] {
            let mut h = Harness::new();
            h.run(cmd);
            let ev = h.bus("speak.cleared");
            assert_eq!(ev.payload["keep_current"].as_bool(), Some(keep_current));
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
    fn resolve_skip_token_maps_each_human_phrase_to_its_stable_bus_token() {
        for (phrase, token) in [
            ("blocked by alias", "blocked_by_alias"),
            ("no voices available", "no_voices_available"),
            (
                "voice override not found in catalog",
                "voice_override_not_found",
            ),
            (
                "something the resolver has never emitted",
                "voice_resolution_failed",
            ),
        ] {
            assert_eq!(resolve_skip_token(phrase), token);
        }
    }

    #[test]
    fn pipeline_skip_token_carries_rule_name_only_for_matched_rule() {
        use forge_tts_pipeline::SkipReason;
        assert_eq!(
            pipeline_skip_token(&SkipReason::MatchedSkipRule("no urls")),
            ("skip_rule_matched".to_owned(), Some("no urls".to_owned())),
        );
        assert_eq!(
            pipeline_skip_token(&SkipReason::BlockedByWordFilter),
            ("blocked_by_word_filter".to_owned(), None),
        );
        assert_eq!(
            pipeline_skip_token(&SkipReason::EmptyAfterProcessing),
            ("empty_after_processing".to_owned(), None),
        );
    }

    fn localized_voice(id: &str, engine: &str, locale: &str) -> TtsVoice {
        TtsVoice {
            id: VoiceId(id.into()),
            name: id.into(),
            locale: locale.into(),
            gender: forge_tts_core::VoiceGender::Neutral,
            engine_id: EngineId(engine.into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    fn bilingual_catalog() -> Vec<TtsVoice> {
        vec![
            localized_voice("alpha-en", "alpha", "en-US"),
            localized_voice("alpha-uk", "alpha", "uk-UA"),
            localized_voice("beta-uk", "beta", "uk_UA"),
        ]
    }

    fn plain_resolver() -> VoiceAliasResolver {
        VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        )
    }

    fn spoken_voice(result: ResolveResult) -> String {
        match result {
            ResolveResult::Speak { voice_id, .. } => voice_id.0,
            ResolveResult::Skip { reason } => {
                panic!("expected a resolved voice, got skip: {reason}")
            }
        }
    }

    fn language(code: &str) -> LanguageCode {
        LanguageCode::from_locale(code).unwrap()
    }

    #[test]
    fn a_detected_language_narrows_resolution_to_voices_that_speak_it() {
        // Both viewers land on the opposite language unnarrowed, so a narrowing that did
        // nothing would fail here rather than pass by luck of the deterministic hash.
        let catalog = bilingual_catalog();
        for (viewer, code, suffix) in [("zoryana", "uk", "-uk"), ("nova", "en", "-en")] {
            let req = request(viewer, "message", Priority::Normal);
            let voice = spoken_voice(resolve_with_overrides(
                &plain_resolver(),
                &req,
                &catalog,
                Some(language(code)),
            ));
            assert!(
                voice.ends_with(suffix),
                "{viewer} narrowed to {code} must not keep a {voice} voice"
            );
        }
    }

    #[test]
    fn a_detected_language_no_voice_speaks_leaves_the_full_catalog_rather_than_skipping() {
        let catalog = vec![localized_voice("alpha-en", "alpha", "en-US")];
        let req = request("nova", "привіт", Priority::Normal);
        assert_eq!(
            spoken_voice(resolve_with_overrides(
                &plain_resolver(),
                &req,
                &catalog,
                Some(language("uk")),
            )),
            "alpha-en",
            "an inference must never turn into a no-voices-available skip"
        );
    }

    #[test]
    fn an_explicit_alias_outranks_the_detected_language() {
        let alias = forge_voice::VoiceAlias {
            id: forge_voice::AliasId::new(),
            viewer_id: "nova".into(),
            viewer_name: "nova".into(),
            engine_id: EngineId("alpha".into()),
            voice_id: VoiceId("alpha-en".into()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        };
        let resolver = VoiceAliasResolver::new(
            vec![alias],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let req = request("nova", "привіт", Priority::Normal);
        assert_eq!(
            spoken_voice(resolve_with_overrides(
                &resolver,
                &req,
                &bilingual_catalog(),
                Some(language("uk")),
            )),
            "alpha-en",
            "a user instruction outranks an inference"
        );
    }

    #[test]
    fn an_explicit_voice_override_outranks_the_detected_language() {
        let mut req = request("nova", "привіт", Priority::Normal);
        req.voice_override = Some(VoiceId("alpha-en".into()));
        assert_eq!(
            spoken_voice(resolve_with_overrides(
                &plain_resolver(),
                &req,
                &bilingual_catalog(),
                Some(language("uk")),
            )),
            "alpha-en"
        );
    }

    #[test]
    fn an_engine_override_intersects_with_the_detected_language() {
        // `mira` resolves to alpha-en across the whole alpha engine, so the uk answer here
        // can only come from the language narrowing being applied on top of the engine one.
        let mut req = request("mira", "привіт", Priority::Normal);
        req.engine_override = Some(EngineId("alpha".into()));
        assert_eq!(
            spoken_voice(resolve_with_overrides(
                &plain_resolver(),
                &req,
                &bilingual_catalog(),
                Some(language("uk")),
            )),
            "alpha-uk"
        );
    }

    #[test]
    fn an_empty_language_and_engine_intersection_falls_back_to_the_engine_alone() {
        let mut req = request("nova", "hello there", Priority::Normal);
        req.engine_override = Some(EngineId("beta".into()));
        assert_eq!(
            spoken_voice(resolve_with_overrides(
                &plain_resolver(),
                &req,
                &bilingual_catalog(),
                Some(language("en")),
            )),
            "beta-uk",
            "an inference must not empty an explicitly requested engine"
        );
    }

    #[test]
    fn resolution_without_a_detected_language_delegates_to_the_unnarrowed_resolver() {
        let catalog = bilingual_catalog();
        let resolver = plain_resolver();
        for viewer in ["nova", "koval", "zoryana"] {
            let req = request(viewer, "hello", Priority::Normal);
            assert_eq!(
                spoken_voice(resolve_with_overrides(&resolver, &req, &catalog, None)),
                spoken_voice(resolver.resolve(&req.viewer_id, &req.viewer_name, &catalog)),
                "viewer {viewer} must keep the pre-language selection"
            );
        }
    }
}
