use std::sync::Arc;
use std::time::Duration;

const PIPER_SYNTHESIS_TIMEOUT: Duration = Duration::from_secs(30);

use forge_audio::{DeviceSink, NullAudioEventSink, OutputDeviceHandle, stored_output_device};
use forge_events::EventPublisher;
use forge_runtime::EventBus;
use forge_speak_queue::{PipelineConfigHandle, QueueConfig, QueueDeps, SpeakEventStream};
use forge_storage::paths;
use forge_storage::{CredentialsRepo, DataProvider};
use forge_tts_cloud::CloudGateways;
use forge_tts_core::{EngineId, TtsEngineFactory, TtsRegistry, build_engine};
use forge_tts_espeak::EspeakEngineFactory;
use forge_tts_nsspeech::NsSpeechEngineFactory;
use forge_tts_piper::{PiperEngine, PiperEngineFactory};
use forge_tts_sapi::SapiEngineFactory;
use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAliasResolver};

use crate::cloud_tts_boot::register_cloud_engines;

async fn register_local_engines(registry: &mut TtsRegistry) {
    if let Some(piper_binary) = PiperEngine::locate().await {
        let voices_dir = PiperEngine::voices_dir(&paths::data_dir());
        if let Err(e) = std::fs::create_dir_all(&voices_dir) {
            tracing::warn!(error = %e, path = %voices_dir.display(), "failed to create the piper voices dir");
        }
        registry.register(
            EngineId(forge_tts_core::ENGINE_ID_PIPER.into()),
            Arc::new(PiperEngineFactory {
                piper_binary: piper_binary.clone(),
                voices_dir,
                timeout: PIPER_SYNTHESIS_TIMEOUT,
            }),
        );
        tracing::info!(path = %piper_binary.display(), "registered the Piper TTS engine");
    } else {
        tracing::info!(
            "Piper binary not found next to the executable or on PATH; Piper TTS disabled"
        );
    }

    register_probed(
        registry,
        forge_tts_core::ENGINE_ID_ESPEAK_NG,
        Arc::new(EspeakEngineFactory),
    )
    .await;
    register_probed(
        registry,
        forge_tts_core::ENGINE_ID_SAPI,
        Arc::new(SapiEngineFactory),
    )
    .await;
    register_probed(
        registry,
        forge_tts_core::ENGINE_ID_NSSPEECH,
        Arc::new(NsSpeechEngineFactory),
    )
    .await;
}

async fn register_probed(
    registry: &mut TtsRegistry,
    engine_id: &str,
    factory: Arc<dyn TtsEngineFactory>,
) {
    let id = EngineId(engine_id.into());
    let name = forge_tts_core::engine_display_name(engine_id);
    match build_engine(&id, Arc::clone(&factory)).await {
        Ok(engine) => {
            registry.register_built(id, factory, engine);
            tracing::info!(engine = name, "registered a TTS engine");
        }
        Err(e) => tracing::info!(engine = name, error = %e, "TTS engine unavailable"),
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
            tracing::warn!(error = %e, "failed to load voice aliases on boot; using defaults");
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
            tracing::warn!(error = %e, "failed to load synthesis defaults on boot; using defaults");
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
            Err(e) => tracing::warn!(
                error = %e,
                engine = %engine_id.0,
                "failed to load TTS engine params on boot; using defaults"
            ),
        }
    }
    (defaults, gains)
}

async fn load_master_volume(backend: &Arc<dyn DataProvider>) -> f32 {
    match forge_storage::master_volume(backend.as_ref()).await {
        Ok(volume) => volume,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load the master volume on boot; using full volume");
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
            tracing::warn!(error = %e, "failed to load the disabled TTS engines on boot; all engines stay enabled");
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
            tracing::warn!(error = %e, "failed to load TTS filters on boot; using defaults");
            forge_tts_pipeline::PipelineConfig::default()
        }
    }
}

pub async fn build_speech_output(backend: &Arc<dyn DataProvider>) -> Arc<DeviceSink> {
    let stored_device_id = match backend.audio_output_device_id().await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read the stored audio output device preference");
            None
        }
    };
    Arc::new(DeviceSink::new(
        OutputDeviceHandle::new(stored_output_device(stored_device_id)),
        Arc::new(NullAudioEventSink),
    ))
}

pub async fn build_speak_queue(
    bus: &Arc<EventBus>,
    backend: &Arc<dyn DataProvider>,
    audio_sink: Arc<dyn forge_audio::AudioSink>,
    cloud_gateways: &CloudGateways,
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
    register_cloud_engines(&registry, creds.as_ref(), cloud_gateways).await;
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
