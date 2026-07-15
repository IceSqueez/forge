use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::{CpalSink, DeviceId, NullAudioEventSink, NullSink};
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

fn find_piper_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled = dir.join("piper");
        if bundled.exists() {
            return Some(bundled);
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join("piper"))
            .find(|p| p.exists())
    })
}

/// Resolves the output device to open at boot: the user's persisted preference if it
/// still exists in the current device list, else the OS default, else the first
/// enumerated device. Returns `None` only when no output devices exist at all.
async fn resolve_audio_output_device(backend: &Arc<dyn DataProvider>) -> Option<DeviceId> {
    let devices = match forge_audio::list_output_devices() {
        Ok(devices) => devices,
        Err(e) => {
            eprintln!("forge-desktop: failed to enumerate audio output devices: {e}");
            return None;
        }
    };
    if devices.is_empty() {
        return None;
    }

    let stored = match backend.audio_output_device_id().await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("forge-desktop: failed to read stored audio output device preference: {e}");
            None
        }
    };

    if let Some(stored_id) = stored
        && let Some(found) = devices.iter().find(|d| d.id.as_str() == stored_id)
    {
        return Some(found.id.clone());
    }

    devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| d.id.clone())
        .or_else(|| devices.first().map(|d| d.id.clone()))
}

fn register_local_engines(registry: &mut TtsRegistry) {
    if let Some(piper_binary) = find_piper_binary() {
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
    VoiceAliasResolver::new(aliases, strategy, profile, SynthesisDefaults::default())
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

/// Brings up the speak-queue actor from persisted TTS config: the engine registry
/// (local engines + credentialed cloud engines), the voice-alias resolver, the
/// message-preprocessing pipeline, and the audio output sink. Mirrors the pre-cutover
/// boot sequence's fallbacks 1:1 — an absent output device degrades to `NullSink`, an
/// engine whose prerequisites are missing is skipped with a log line — so every
/// failure point is data-safe and boot always proceeds.
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
    register_local_engines(&mut registry);

    let registry = std::sync::RwLock::new(registry);
    let creds: Arc<dyn CredentialsRepo> = Arc::clone(backend) as Arc<dyn CredentialsRepo>;
    register_cloud_engines(&registry, creds.as_ref()).await;
    let registry = Arc::new(registry);

    let resolver = Arc::new(std::sync::RwLock::new(load_resolver(backend).await));
    let pipeline = PipelineConfigHandle::new(load_pipeline(backend).await);

    let audio_sink: Arc<dyn forge_audio::AudioSink> = match resolve_audio_output_device(backend)
        .await
    {
        Some(device_id) => {
            eprintln!(
                "forge-desktop: speak queue audio sink ready on device {}",
                device_id.0
            );
            Arc::new(CpalSink::new(
                device_id,
                None,
                None,
                Arc::new(NullAudioEventSink),
            ))
        }
        None => {
            eprintln!("forge-desktop: no audio output device found; speak queue using NullSink");
            Arc::new(NullSink)
        }
    };

    let deps = QueueDeps {
        registry: Arc::clone(&registry),
        resolver,
        pipeline: pipeline.clone(),
        audio_sink,
        event_bus: Arc::clone(bus) as Arc<dyn EventPublisher>,
    };
    let (handle, stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    (Some(handle), Some(stream), Some(pipeline), Some(registry))
}
