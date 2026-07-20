use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use forge_audio::PcmBuffer;
use forge_events::{Event, EventPublisher, EventSource};
use forge_tts_core::{EngineId, SynthesisRequest, TtsVoice, VoiceId};
use forge_tts_pipeline::PipelineResult;
use forge_voice::{AliasState, ResolveResult, VoiceAliasResolver};

use crate::{
    PipelineConfigHandle, Priority, QueueConfig, QueueDeps, RequestId, SpeakCommand, SpeakEvent,
    SpeakRequest,
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

enum SynthOutcome {
    Speak {
        pcm: PcmBuffer,
        voice_id: VoiceId,
        engine_id: EngineId,
    },
    Skipped {
        reason: String,
    },
    Failed {
        error: String,
    },
}

struct SynthTaskDeps {
    resolver: Arc<std::sync::RwLock<forge_voice::VoiceAliasResolver>>,
    pipeline: PipelineConfigHandle,
    registry: Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>,
    voice_catalog: Arc<Vec<TtsVoice>>,
}

async fn run_synthesis(req: SpeakRequest, deps: SynthTaskDeps) -> SynthTaskResult {
    // Load the current config (atomic Arc clone, read guard dropped immediately).
    let pipeline_cfg = deps.pipeline.load();

    // Reward-origin gating is per-message, not part of the shared PipelineConfig,
    // so it's applied here as a pre-pass rather than inside `process`: reusing the
    // same word-token strip `process`'s emote stage performs, independently of
    // `emote_sources.twitch` (which gates only the general/always-in-the-pipeline
    // strip inside `process`). A no-op when the toggle is off or the message isn't
    // reward-sourced.
    let reward_stripped;
    let text_for_pipeline: &str = if req.is_reward && pipeline_cfg.strip_reward_emotes {
        reward_stripped =
            forge_tts_pipeline::strip_emote_tokens(&req.text, &pipeline_cfg.emote_tokens);
        &reward_stripped
    } else {
        &req.text
    };
    let pipeline_result = forge_tts_pipeline::process(text_for_pipeline, &pipeline_cfg);
    let text_to_speak = match pipeline_result {
        PipelineResult::Speak(t) => t,
        PipelineResult::Skip { reason } => {
            return SynthTaskResult {
                request_id: req.request_id.clone(),
                request: req,
                outcome: SynthOutcome::Skipped {
                    reason: format!("{reason:?}"),
                },
            };
        }
    };

    let resolve_result = {
        let guard = deps.resolver.read().unwrap_or_else(|e| e.into_inner());
        resolve_with_overrides(&guard, &req, &deps.voice_catalog)
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
                    reason: reason.to_owned(),
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
                    error: format!("engine {:?} not registered", engine_id),
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
                    error: e.to_string(),
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
            },
        },
        Err(e) => SynthTaskResult {
            request_id: req.request_id.clone(),
            request: req,
            outcome: SynthOutcome::Failed {
                error: e.to_string(),
            },
        },
    }
}

fn resolve_with_overrides(
    resolver: &VoiceAliasResolver,
    req: &SpeakRequest,
    catalog: &[TtsVoice],
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

    if let Some(engine_id) = &req.engine_override {
        let scoped: Vec<TtsVoice> = catalog
            .iter()
            .filter(|v| &v.engine_id == engine_id)
            .cloned()
            .collect();
        return resolver.resolve(&req.viewer_id, &req.viewer_name, &scoped);
    }

    resolver.resolve(&req.viewer_id, &req.viewer_name, catalog)
}

fn apply_gain(mut buf: PcmBuffer, gain: f32) -> PcmBuffer {
    if (gain - 1.0_f32).abs() < f32::EPSILON {
        return buf;
    }
    buf.samples = buf
        .samples
        .iter()
        .map(|&s| (s as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16)
        .collect();
    buf
}

fn publish(bus: &dyn EventPublisher, kind: &str, payload: serde_json::Value) {
    bus.publish(Event::new(EventSource::Audio, kind, payload));
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_actor(
    mut config: QueueConfig,
    deps: QueueDeps,
    mut cmd_rx: tokio::sync::mpsc::Receiver<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    depth: Arc<AtomicUsize>,
    voices: Arc<std::sync::RwLock<Arc<Vec<TtsVoice>>>>,
    disabled_engines: Arc<std::sync::RwLock<Arc<HashSet<EngineId>>>>,
    master_volume_bits: Arc<AtomicU32>,
    engine_gains: Arc<std::sync::RwLock<Arc<HashMap<EngineId, f32>>>>,
) {
    let mut high_queue: VecDeque<SpeakRequest> = VecDeque::new();
    let mut normal_queue: VecDeque<SpeakRequest> = VecDeque::new();
    let mut per_user_counts: HashMap<String, usize> = HashMap::new();
    let mut paused = false;
    let mut voicegate_active = false;
    let mut active_request_id: Option<RequestId> = None;
    let mut last_successful: Option<SpeakRequest> = None;
    let mut disabled: HashSet<EngineId> = deps.disabled_engines.clone();
    let mut gains: HashMap<EngineId, f32> = deps.engine_gains.clone();
    let mut current_playback: Option<CurrentPlayback> = None;
    let mut progress_ticker: Option<tokio::time::Interval> = None;

    let (synth_tx, mut synth_rx) = tokio::sync::mpsc::channel::<SynthTaskResult>(8);
    let (catalog_tx, mut catalog_rx) = tokio::sync::mpsc::channel::<Arc<Vec<TtsVoice>>>(1);

    let voice_catalog = Arc::new(build_voice_catalog(&deps.registry, &disabled).await);
    *voices.write().unwrap_or_else(|e| e.into_inner()) = voice_catalog.clone();
    *disabled_engines.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(disabled.clone());
    *engine_gains.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(gains.clone());
    let mut task_deps = SynthTaskDeps {
        resolver: deps.resolver.clone(),
        pipeline: deps.pipeline.clone(),
        registry: deps.registry.clone(),
        voice_catalog: voice_catalog.clone(),
    };

    loop {
        depth.store(high_queue.len() + normal_queue.len(), Ordering::Relaxed);

        if active_request_id.is_none()
            && !paused
            && !voicegate_active
            && let Some(req) = pop_next(&mut high_queue, &mut normal_queue, &mut per_user_counts)
        {
            let queue_len = high_queue.len() + normal_queue.len();
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
                serde_json::json!({ "request_id": req.request_id.0, "queue_len": queue_len }),
            );
            active_request_id = Some(req.request_id.clone());
            let tx = synth_tx.clone();
            let task_deps_clone = SynthTaskDeps {
                resolver: task_deps.resolver.clone(),
                pipeline: task_deps.pipeline.clone(),
                registry: task_deps.registry.clone(),
                voice_catalog: task_deps.voice_catalog.clone(),
            };
            tokio::spawn(async move {
                let result = run_synthesis(req, task_deps_clone).await;
                let _ = tx.send(result).await;
            });
        }

        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                    Some(SpeakCommand::RefreshVoiceCatalog) => {
                        spawn_catalog_rebuild(deps.registry.clone(), disabled.clone(), catalog_tx.clone());
                    }
                    Some(SpeakCommand::SetEngineEnabled(engine_id, enabled)) => {
                        if enabled {
                            disabled.remove(&engine_id);
                        } else {
                            disabled.insert(engine_id);
                        }
                        *disabled_engines.write().unwrap_or_else(|e| e.into_inner()) =
                            Arc::new(disabled.clone());
                        spawn_catalog_rebuild(deps.registry.clone(), disabled.clone(), catalog_tx.clone());
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
                        gains.insert(engine_id, gain.clamp(0.0, 1.0));
                        *engine_gains.write().unwrap_or_else(|e| e.into_inner()) =
                            Arc::new(gains.clone());
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
                        &gains,
                    ).await;
                }
            }
            result = catalog_rx.recv() => {
                if let Some(catalog) = result {
                    *voices.write().unwrap_or_else(|e| e.into_inner()) = catalog.clone();
                    task_deps.voice_catalog = catalog;
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
                        &high_queue,
                        &normal_queue,
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
        SynthOutcome::Skipped { reason } => {
            *active_request_id = None;
            tracing::debug!(request_id = %result.request_id.0, %reason, "speak skipped");
            let _ = event_tx.send(SpeakEvent::Skipped {
                request_id: result.request_id.clone(),
                reason: reason.clone(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.skipped",
                serde_json::json!({ "request_id": result.request_id.0, "reason": reason }),
            );
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len });
        }
        SynthOutcome::Failed { error } => {
            *active_request_id = None;
            tracing::warn!(request_id = %result.request_id.0, %error, "speak failed");
            let _ = event_tx.send(SpeakEvent::Failed {
                request_id: result.request_id.clone(),
                error: error.clone(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.failed",
                serde_json::json!({ "request_id": result.request_id.0, "error": error }),
            );
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len });
        }
        SynthOutcome::Speak {
            pcm,
            voice_id,
            engine_id,
        } => {
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
                }),
            );
            let engine_gain = engine_gains.get(&engine_id).copied().unwrap_or(1.0);
            let adjusted = apply_gain(pcm, config.master_volume * engine_gain);
            match deps.audio_sink.play_controlled(adjusted).await {
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
                        serde_json::json!({ "request_id": result.request_id.0, "error": e.to_string() }),
                    );
                    let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len });
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn finish_playback(
    play_result: Result<(), forge_audio::AudioError>,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    last_successful: &mut Option<SpeakRequest>,
    current: CurrentPlayback,
    high_queue: &VecDeque<SpeakRequest>,
    normal_queue: &VecDeque<SpeakRequest>,
) {
    *active_request_id = None;
    let queue_len = high_queue.len() + normal_queue.len();

    match play_result {
        Ok(()) => {
            let _ = event_tx.send(SpeakEvent::Finished {
                request_id: current.request_id.clone(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.finished",
                serde_json::json!({ "request_id": current.request_id.0 }),
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
                serde_json::json!({ "request_id": current.request_id.0, "error": e.to_string() }),
            );
        }
    }
    let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len });
}

fn stop_active(
    reason: &str,
    deps: &QueueDeps,
    event_tx: &tokio::sync::broadcast::Sender<SpeakEvent>,
    active_request_id: &mut Option<RequestId>,
    current_playback: &mut Option<CurrentPlayback>,
    progress_ticker: &mut Option<tokio::time::Interval>,
) {
    let Some(request_id) = active_request_id.take() else {
        return;
    };
    if let Some(current) = current_playback.take() {
        current.playback.stop();
    }
    *progress_ticker = None;
    let _ = event_tx.send(SpeakEvent::Skipped {
        request_id: request_id.clone(),
        reason: reason.to_owned(),
    });
    publish(
        deps.event_bus.as_ref(),
        "speak.skipped",
        serde_json::json!({ "request_id": request_id.0, "reason": reason }),
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
    let preview = match resolve_with_overrides(&guard, req, catalog) {
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
                        "reason": format!("queue full (max {})", config.max_queue_len),
                    }),
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
                        "reason": format!("per-user limit (max {})", config.per_user_limit),
                    }),
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
                serde_json::json!({ "request_id": req.request_id.0, "queue_len": total_after }),
            );
            let _ = event_tx.send(SpeakEvent::QueueChanged {
                queue_len: total_after,
            });
        }
        SpeakCommand::Skip => {
            stop_active(
                "skipped by user",
                deps,
                event_tx,
                active_request_id,
                current_playback,
                progress_ticker,
            );
            let total = high_queue.len() + normal_queue.len();
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: total });
        }
        SpeakCommand::PlayNow(request_id) => {
            if let Some(req) = take_from_queues(high_queue, normal_queue, &request_id) {
                high_queue.push_front(req);
                stop_active(
                    "promoted by user",
                    deps,
                    event_tx,
                    active_request_id,
                    current_playback,
                    progress_ticker,
                );
                let total = high_queue.len() + normal_queue.len();
                let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: total });
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
                    serde_json::json!({ "request_id": request_id.0 }),
                );
                let total = high_queue.len() + normal_queue.len();
                let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: total });
            }
        }
        SpeakCommand::Clear => {
            stop_active(
                "stopped by clear",
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
                serde_json::json!({}),
            );
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: 0 });
        }
        SpeakCommand::ClearPending => {
            high_queue.clear();
            normal_queue.clear();
            per_user_counts.clear();
            publish(
                deps.event_bus.as_ref(),
                "speak.cleared",
                serde_json::json!({ "keep_current": true }),
            );
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: 0 });
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
        // Intercepted in `run_actor` before dispatch (needs to spawn an async
        // rebuild, or update the handle-visible gain/resolver mirrors); never
        // reaches this synchronous handler.
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
                serde_json::json!({ "reason": "user paused" }),
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
                serde_json::json!({}),
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
                let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: total });
            }
        }
        SpeakCommand::VoiceGateActivated => {
            *voicegate_active = true;
            let _ = event_tx.send(SpeakEvent::Paused {
                reason: "voicegate".into(),
            });
            publish(
                deps.event_bus.as_ref(),
                "speak.paused",
                serde_json::json!({ "reason": "voicegate" }),
            );
        }
        SpeakCommand::VoiceGateDeactivated => {
            *voicegate_active = false;
            let _ = event_tx.send(SpeakEvent::Resumed);
            publish(
                deps.event_bus.as_ref(),
                "speak.resumed",
                serde_json::json!({}),
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
            source_event_id: forge_types::EventId::new(),
            is_reward: false,
        }
    }

    #[test]
    fn set_volume_clamps_into_unit_range() {
        let deps = minimal_deps();
        let (tx, _rx) = tokio::sync::broadcast::channel::<SpeakEvent>(8);
        // Boundaries (0.0, 1.0) pass through; out-of-range values clamp; an
        // in-range value is preserved. Removing the clamp fails the 1.5/-0.2 rows.
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
        // The dashboard now-speaking/queue rows read these fields off the event;
        // before the fix Enqueued shipped only request_id/queue_len.
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

    /// Records the text of every `SynthesisRequest` it receives so a test can
    /// observe what `run_synthesis` actually handed to the engine.
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
        // TTS-7: the reward pre-pass strips `emote_tokens` from the spoken text
        // ONLY when the message is reward-sourced AND the persisted toggle is on.
        // Any weaker gate (OR, ignoring one operand, always-on) fails a row.
        // The engine records the text it actually received to synthesize.
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
            };

            let mut req = request("nova", "hi LUL", Priority::Normal);
            req.voice_override = Some(voice.id.clone());
            req.is_reward = is_reward;

            let result = run_synthesis(req, deps).await;
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
}
