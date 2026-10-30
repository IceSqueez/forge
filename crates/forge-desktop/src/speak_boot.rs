use std::sync::Arc;

use forge_audio::{AudioSinkFactory, DeviceSink, OutputDeviceHandle, stored_output_device};
use forge_events::EventPublisher;
use forge_runtime::EventBus;
use forge_speak_queue::{
    PipelineConfigHandle, QueueConfig, QueueDeps, SpeakEventStream, VoiceAliasResolver,
    build_config, repeat_window,
};
use forge_speech_types::EngineId;
use forge_speech_types::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults};
use forge_storage::settings::core_settings::AUDIO_OUTPUT_DEVICE_ID;
use forge_storage::{CredentialsRepo, DataProvider, FilterRule, TtsPipelineSettings, read_setting};
use forge_tts_cloud::CloudGateways;
use forge_tts_core::{TtsEngineFactory, TtsRegistry, build_engine};

use crate::boot_warnings::{BootWarning, BootWarnings};
use crate::cloud_tts_boot::register_cloud_engines;

pub enum TtsEngineAdmission {
    Unprobed,
    Probed,
}

pub struct LocalTtsEngine {
    pub id: EngineId,
    pub factory: Arc<dyn TtsEngineFactory>,
    pub admission: TtsEngineAdmission,
}

async fn register_local_engines(registry: &mut TtsRegistry, engines: Vec<LocalTtsEngine>) {
    for engine in engines {
        match engine.admission {
            TtsEngineAdmission::Unprobed => registry.register(engine.id, engine.factory),
            TtsEngineAdmission::Probed => {
                register_probed(registry, engine.id, engine.factory).await;
            }
        }
    }
}

async fn register_probed(
    registry: &mut TtsRegistry,
    id: EngineId,
    factory: Arc<dyn TtsEngineFactory>,
) {
    let name = forge_tts_core::engine_display_name(&id.0);
    match build_engine(&id, Arc::clone(&factory)).await {
        Ok(engine) => {
            tracing::info!(engine = name, "registered a TTS engine");
            registry.register_built(id, factory, engine);
        }
        Err(e) => tracing::info!(engine = name, error = %e, "TTS engine unavailable"),
    }
}

async fn load_resolver(
    backend: &Arc<dyn DataProvider>,
    warnings: &mut BootWarnings,
) -> VoiceAliasResolver {
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
            warnings.record(BootWarning::VoiceAliases);
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

async fn load_filters(backend: &Arc<dyn DataProvider>) -> (Vec<FilterRule>, TtsPipelineSettings) {
    let filters_repo = backend.tts_filters_repo();
    let loaded = async {
        let rules = filters_repo.list_rules().await?;
        let settings = filters_repo.get_pipeline_settings().await?;
        Ok::<_, forge_storage::StorageError>((rules, settings))
    }
    .await;
    loaded.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to load TTS filters on boot; using defaults");
        (Vec::new(), TtsPipelineSettings::default())
    })
}

pub async fn build_speech_output(
    backend: &Arc<dyn DataProvider>,
    audio_sinks: Arc<dyn AudioSinkFactory>,
) -> Arc<DeviceSink> {
    let stored_device_id = match read_setting(backend.as_ref(), &AUDIO_OUTPUT_DEVICE_ID).await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read the stored audio output device preference");
            None
        }
    };
    Arc::new(DeviceSink::with_factory(
        OutputDeviceHandle::new(stored_output_device(stored_device_id)),
        audio_sinks,
    ))
}

pub async fn build_speak_queue(
    bus: &Arc<EventBus>,
    backend: &Arc<dyn DataProvider>,
    audio_sink: Arc<dyn forge_audio::AudioSink>,
    cloud_gateways: &CloudGateways,
    local_engines: Vec<LocalTtsEngine>,
    warnings: &mut BootWarnings,
) -> (
    Option<forge_speak_queue::SpeakQueueHandle>,
    Option<SpeakEventStream>,
    Option<PipelineConfigHandle>,
    Option<Arc<std::sync::RwLock<TtsRegistry>>>,
) {
    let mut registry = TtsRegistry::new();
    register_local_engines(&mut registry, local_engines).await;

    let registry = std::sync::RwLock::new(registry);
    let creds: Arc<dyn CredentialsRepo> = Arc::clone(backend) as Arc<dyn CredentialsRepo>;
    register_cloud_engines(&registry, creds.as_ref(), cloud_gateways).await;
    let registry = Arc::new(registry);

    let engine_ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();
    let (engine_defaults, engine_gains) = load_engine_params(backend, &engine_ids).await;

    let mut resolver = load_resolver(backend, warnings).await;
    resolver.set_engine_defaults(engine_defaults);
    let resolver = Arc::new(std::sync::RwLock::new(resolver));
    let (filter_rules, filter_settings) = load_filters(backend).await;
    let (pipeline_config, _) = build_config(&filter_rules, &filter_settings).dropping_invalid();
    let pipeline = PipelineConfigHandle::new(pipeline_config);
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
        repeat_window: repeat_window(&filter_settings),
        ..QueueConfig::default()
    };
    let (handle, stream) = forge_speak_queue::spawn(queue_config, deps);
    (Some(handle), Some(stream), Some(pipeline), Some(registry))
}
