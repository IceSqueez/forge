use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::{NullAudioEventSink, NullSink};
use forge_events::EventPublisher;
use forge_platform_core::paths;
use forge_runtime::EventBus;
use forge_speak_queue::{PipelineConfigHandle, QueueConfig, QueueDeps, SpeakEventStream};
use forge_storage::{CredentialsRepo, DataProvider};
use forge_tts_core::{EngineId, TtsEngineFactory, TtsRegistry};
use forge_tts_espeak::EspeakEngineFactory;
use forge_tts_nsspeech::NsSpeechEngineFactory;
use forge_tts_piper::{PiperEngine, PiperEngineFactory};
use forge_tts_sapi::SapiEngineFactory;
use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAliasResolver};

use crate::cloud_tts_boot::register_cloud_engines;

async fn is_piper_tts(path: &PathBuf) -> bool {
    let probe = tokio::process::Command::new(path)
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .output();
    match tokio::time::timeout(std::time::Duration::from_secs(2), probe).await {
        Ok(Ok(out)) => {
            let mut text = out.stdout;
            text.extend_from_slice(&out.stderr);
            String::from_utf8_lossy(&text).contains("--model")
        }
        _ => false,
    }
}

async fn find_piper_binary() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("piper"));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for name in ["piper-tts", "piper"] {
            for dir in std::env::split_paths(&paths) {
                candidates.push(dir.join(name));
            }
        }
    }
    for candidate in candidates {
        if candidate.exists() {
            if is_piper_tts(&candidate).await {
                return Some(candidate);
            }
            eprintln!(
                "forge-desktop: ignoring {} - not a piper TTS binary",
                candidate.display()
            );
        }
    }
    None
}

async fn register_local_engines(registry: &mut TtsRegistry) {
    if let Some(piper_binary) = find_piper_binary().await {
        let voices_dir = PiperEngine::voices_dir(&paths::data_dir());
        if let Err(e) = std::fs::create_dir_all(&voices_dir) {
            eprintln!(
                "forge-desktop: failed to create piper voices dir {}: {e}",
                voices_dir.display()
            );
        }
        registry.register(
            EngineId("piper".into()),
            Arc::new(PiperEngineFactory {
                piper_binary: piper_binary.clone(),
                voices_dir,
                timeout: Duration::from_secs(30),
            }),
        );
        eprintln!(
            "forge-desktop: registered Piper TTS engine ({})",
            piper_binary.display()
        );
    } else {
        eprintln!("forge-desktop: Piper binary not found in <exe_dir>/piper or PATH; TTS disabled");
    }

    match EspeakEngineFactory.create() {
        Ok(engine) => {
            registry.register(engine.engine_id().clone(), Arc::new(EspeakEngineFactory));
            eprintln!("forge-desktop: registered eSpeak-NG TTS engine");
        }
        Err(e) => eprintln!("forge-desktop: eSpeak-NG TTS engine unavailable: {e}"),
    }

    match SapiEngineFactory.create() {
        Ok(engine) => {
            registry.register(engine.engine_id().clone(), Arc::new(SapiEngineFactory));
            eprintln!("forge-desktop: registered SAPI 5 TTS engine");
        }
        Err(e) => eprintln!("forge-desktop: SAPI 5 TTS engine unavailable: {e}"),
    }

    match NsSpeechEngineFactory.create() {
        Ok(engine) => {
            registry.register(engine.engine_id().clone(), Arc::new(NsSpeechEngineFactory));
            eprintln!("forge-desktop: registered AVFoundation TTS engine");
        }
        Err(e) => eprintln!("forge-desktop: AVFoundation TTS engine unavailable: {e}"),
    }
}

async fn load_resolver(backend: &Arc<dyn DataProvider>) -> VoiceAliasResolver {
    let voice_alias_repo = backend.voice_alias_repo();
    let loaded = async {
        let aliases = voice_alias_repo.list().await?;
        let strategy = voice_alias_repo.get_strategy().await?;
        let profile = voice_alias_repo.get_ignore_profile().await?;
        Ok::<_, forge_storage::StorageError>((aliases, strategy, profile))
    }
    .await;
    let (aliases, strategy, profile) = match loaded {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("forge-desktop: failed to load voice aliases on boot; using defaults: {e}");
            (
                Vec::new(),
                AssignmentStrategy::default(),
                IgnoreProfile::default(),
            )
        }
    };
    let defaults = match forge_storage::synthesis_defaults(backend.as_ref()).await {
        Ok(defaults) => defaults,
        Err(e) => {
            eprintln!(
                "forge-desktop: failed to load synthesis defaults on boot; using defaults: {e}"
            );
            SynthesisDefaults::default()
        }
    };
    VoiceAliasResolver::new(aliases, strategy, profile, defaults)
}

async fn load_engine_params(
    backend: &Arc<dyn DataProvider>,
    engine_ids: &[EngineId],
) -> (
    std::collections::HashMap<EngineId, SynthesisDefaults>,
    std::collections::HashMap<EngineId, f32>,
) {
    let mut defaults = std::collections::HashMap::new();
    let mut gains = std::collections::HashMap::new();
    for engine_id in engine_ids {
        match forge_storage::engine_params(backend.as_ref(), &engine_id.0).await {
            Ok(params) => {
                defaults.insert(
                    engine_id.clone(),
                    SynthesisDefaults {
                        pitch_semitones: params.pitch_semitones,
                        rate_multiplier: params.rate_multiplier,
                    },
                );
                gains.insert(engine_id.clone(), params.gain);
            }
            Err(e) => eprintln!(
                "forge-desktop: failed to load engine params for {}; using defaults: {e}",
                engine_id.0
            ),
        }
    }
    (defaults, gains)
}

async fn load_master_volume(backend: &Arc<dyn DataProvider>) -> f32 {
    match forge_storage::master_volume(backend.as_ref()).await {
        Ok(volume) => volume,
        Err(e) => {
            eprintln!("forge-desktop: failed to load master volume on boot; using 1.0: {e}");
            1.0
        }
    }
}

async fn load_disabled_engines(
    backend: &Arc<dyn DataProvider>,
) -> std::collections::HashSet<EngineId> {
    match forge_storage::disabled_tts_engines(backend.as_ref()).await {
        Ok(ids) => ids.into_iter().map(EngineId).collect(),
        Err(e) => {
            eprintln!(
                "forge-desktop: failed to load disabled TTS engines on boot; using none: {e}"
            );
            std::collections::HashSet::new()
        }
    }
}

async fn load_pipeline(backend: &Arc<dyn DataProvider>) -> forge_tts_pipeline::PipelineConfig {
    let filters_repo = backend.tts_filters_repo();
    let loaded = async {
        let rules = filters_repo.list_rules().await?;
        let settings = filters_repo.get_pipeline_settings().await?;
        Ok::<_, forge_storage::StorageError>((rules, settings))
    }
    .await;
    match loaded {
        Ok((rules, settings)) => forge_speak_queue::build_config_lenient(&rules, &settings),
        Err(e) => {
            eprintln!("forge-desktop: failed to load tts filters on boot; using defaults: {e}");
            forge_tts_pipeline::PipelineConfig::default()
        }
    }
}

pub async fn build_speak_queue(
    bus: &Arc<EventBus>,
    backend: &Arc<dyn DataProvider>,
) -> (
    Option<forge_speak_queue::SpeakQueueHandle>,
    Option<SpeakEventStream>,
    Option<PipelineConfigHandle>,
    Option<Arc<std::sync::RwLock<TtsRegistry>>>,
) {
    let mut registry = TtsRegistry::new();
    register_local_engines(&mut registry).await;

    let registry = std::sync::RwLock::new(registry);
    let creds: Arc<dyn CredentialsRepo> = Arc::clone(backend) as Arc<dyn CredentialsRepo>;
    register_cloud_engines(&registry, creds.as_ref()).await;
    let registry = Arc::new(registry);

    let engine_ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();
    let (engine_defaults, engine_gains) = load_engine_params(backend, &engine_ids).await;

    let mut resolver = load_resolver(backend).await;
    resolver.engine_defaults = engine_defaults;
    let resolver = Arc::new(std::sync::RwLock::new(resolver));
    let pipeline = PipelineConfigHandle::new(load_pipeline(backend).await);
    let disabled_engines = load_disabled_engines(backend).await;

    let stored_device_id = match backend.audio_output_device_id().await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("forge-desktop: failed to read stored audio output device preference: {e}");
            None
        }
    };
    let audio_sink: Arc<dyn forge_audio::AudioSink> =
        match forge_audio::resolve_output_device(stored_device_id).await {
            Ok(device_id) => {
                eprintln!(
                    "forge-desktop: speak queue audio sink ready on device {}",
                    device_id.0
                );
                forge_audio::build_cpal_sink(device_id, Arc::new(NullAudioEventSink))
            }
            Err(e) => {
                eprintln!(
                    "forge-desktop: no audio output device found; speak queue using NullSink: {e}"
                );
                Arc::new(NullSink)
            }
        };

    let deps = QueueDeps {
        registry: Arc::clone(&registry),
        resolver,
        pipeline: pipeline.clone(),
        audio_sink,
        event_bus: Arc::clone(bus) as Arc<dyn EventPublisher>,
        disabled_engines,
        engine_gains,
    };
    let queue_config = QueueConfig {
        master_volume: load_master_volume(backend).await,
        ..QueueConfig::default()
    };
    let (handle, stream) = forge_speak_queue::spawn(queue_config, deps);
    (Some(handle), Some(stream), Some(pipeline), Some(registry))
}
