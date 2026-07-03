use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    let pipeline_result = forge_tts_pipeline::process(&req.text, &pipeline_cfg);
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
            Some(engine_id) => ResolveResult::Speak {
                voice_id: voice_id.clone(),
                engine_id,
                pitch: resolver.defaults.pitch_semitones,
                rate: resolver.defaults.rate_multiplier,
            },
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

fn apply_master_volume(mut buf: PcmBuffer, volume: f32) -> PcmBuffer {
    if (volume - 1.0_f32).abs() < f32::EPSILON {
        return buf;
    }
    buf.samples = buf
        .samples
        .iter()
        .map(|&s| (s as f32 * volume).clamp(i16::MIN as f32, i16::MAX as f32) as i16)
        .collect();
    buf
}

fn publish(bus: &dyn EventPublisher, kind: &str, payload: serde_json::Value) {
    bus.publish(Event::new(EventSource::Audio, kind, payload));
}

pub(crate) async fn run_actor(
    mut config: QueueConfig,
    deps: QueueDeps,
    mut cmd_rx: tokio::sync::mpsc::Receiver<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    depth: Arc<AtomicUsize>,
    voices: Arc<std::sync::RwLock<Arc<Vec<TtsVoice>>>>,
) {
    let mut high_queue: VecDeque<SpeakRequest> = VecDeque::new();
    let mut normal_queue: VecDeque<SpeakRequest> = VecDeque::new();
    let mut per_user_counts: HashMap<String, usize> = HashMap::new();
    let mut paused = false;
    let mut voicegate_active = false;
    let mut active_request_id: Option<RequestId> = None;
    let mut last_successful: Option<SpeakRequest> = None;

    let (synth_tx, mut synth_rx) = tokio::sync::mpsc::channel::<SynthTaskResult>(8);
    let (catalog_tx, mut catalog_rx) = tokio::sync::mpsc::channel::<Arc<Vec<TtsVoice>>>(1);

    let voice_catalog = Arc::new(build_voice_catalog(&deps.registry).await);
    *voices.write().unwrap_or_else(|e| e.into_inner()) = voice_catalog.clone();
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
                        let registry = deps.registry.clone();
                        let tx = catalog_tx.clone();
                        tokio::spawn(async move {
                            let catalog = Arc::new(build_voice_catalog(&registry).await);
                            let _ = tx.send(catalog).await;
                        });
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
                        &mut last_successful,
                        &high_queue,
                        &normal_queue,
                    ).await;
                }
            }
            result = catalog_rx.recv() => {
                if let Some(catalog) = result {
                    *voices.write().unwrap_or_else(|e| e.into_inner()) = catalog.clone();
                    task_deps.voice_catalog = catalog;
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
    last_successful: &mut Option<SpeakRequest>,
    high_queue: &VecDeque<SpeakRequest>,
    normal_queue: &VecDeque<SpeakRequest>,
) {
    if active_request_id.as_ref() != Some(&result.request_id) {
        return;
    }
    *active_request_id = None;
    let queue_len = high_queue.len() + normal_queue.len();

    match result.outcome {
        SynthOutcome::Skipped { reason } => {
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
            let adjusted = apply_master_volume(pcm, config.master_volume);
            match deps.audio_sink.play(adjusted).await {
                Ok(()) => {
                    *last_successful = Some(result.request);
                    let _ = event_tx.send(SpeakEvent::Finished {
                        request_id: result.request_id.clone(),
                    });
                    publish(
                        deps.event_bus.as_ref(),
                        "speak.finished",
                        serde_json::json!({ "request_id": result.request_id.0 }),
                    );
                }
                Err(e) => {
                    tracing::warn!(request_id = %result.request_id.0, error = %e, "audio playback failed");
                    let _ = event_tx.send(SpeakEvent::Failed {
                        request_id: result.request_id.clone(),
                        error: e.to_string(),
                    });
                    publish(
                        deps.event_bus.as_ref(),
                        "speak.failed",
                        serde_json::json!({ "request_id": result.request_id.0, "error": e.to_string() }),
                    );
                }
            }
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len });
        }
    }
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
            *active_request_id = None;
            let total = high_queue.len() + normal_queue.len();
            let _ = event_tx.send(SpeakEvent::QueueChanged { queue_len: total });
        }
        SpeakCommand::Clear => {
            high_queue.clear();
            normal_queue.clear();
            per_user_counts.clear();
            *active_request_id = None;
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
        // rebuild); never reaches this synchronous handler.
        SpeakCommand::RefreshVoiceCatalog => {}
        SpeakCommand::Pause => {
            *paused = true;
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
                high_queue.push_back(replay.clone());
                let _ = event_tx.send(SpeakEvent::Enqueued {
                    request_id: replay.request_id.clone(),
                    queue_len: total,
                    viewer_name: replay.viewer_name.clone(),
                    text: replay.text.clone(),
                    is_high_priority: true,
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

async fn build_voice_catalog(
    registry: &std::sync::RwLock<forge_tts_core::TtsRegistry>,
) -> Vec<TtsVoice> {
    let engine_ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();
    let mut catalog = Vec::new();
    for engine_id in engine_ids {
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
