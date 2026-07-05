use std::path::PathBuf;
use std::sync::Arc;

use forge_app::App;
use forge_app::Screen;
use forge_app::app::{theme_callback, update};
use forge_app::boot::{
    load_hotkey_and_register, load_obs_and_connect, load_twitch_credential, load_vtube_and_connect,
};
use forge_app::cloud_tts_boot::register_cloud_engines;
use forge_app::speak_bridge::SpeakBridge;
use forge_app::subscriptions::subscription;
use forge_app::view_router::view;
use forge_audio::{CpalSink, DeviceId, NullSink};
use forge_discord::{DiscordClient, DiscordConfig, register_discord_sub_actions};
use forge_events::EventPublisher;
use forge_hotkey::register_hotkey_triggers;
use forge_midi::{MidiClient, MidiConfig, register_midi_sub_actions, register_midi_triggers};
use forge_obs::{ObsSink, SwitchableObsSink, register_obs_sub_actions, register_obs_triggers};
use forge_platform_core::paths;
use forge_platform_twitch::{
    ChatSessionConfig, HelixHttpTransport, HelixTokenRefresher, HelixTransport,
    SubscriptionTracker, TwitchPlatform, register_twitch_sub_actions, register_twitch_triggers,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, EventBus, QueueScheduler, QueueSchedulerHandle,
    SchedulerCell, ScriptRegistry, TriggerEvaluatorHandle, register_audio_sub_actions,
    register_core_sub_actions, register_core_triggers, spawn_action_engine,
    spawn_trigger_evaluator,
};
use forge_soundboard::{BusAudioEventSink, CpalSinkFactory, SoundboardPlayer};
use forge_speak_queue::{QueueConfig, QueueDeps, SpeakQueueHandle};
use forge_storage::{CredentialsRepo, DataProvider, GlobalsRepo, SettingsRepo, UserGlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_tts_core::{EngineId, TtsEngineFactory, TtsRegistry};
use forge_tts_espeak::EspeakEngineFactory;
use forge_tts_nsspeech::NsSpeechEngineFactory;
use forge_tts_piper::{PiperEngine, PiperEngineFactory};
use forge_tts_sapi::SapiEngineFactory;
use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAliasResolver};
use forge_vtube::{
    SwitchableVTubeSink, VTubeClient, VTubeSink, register_vtube_sub_actions,
    register_vtube_triggers,
};

struct KickNoopLimiter;

#[async_trait::async_trait]
impl forge_platform_core::RateLimiter for KickNoopLimiter {
    async fn acquire(
        &self,
        _weight: u32,
    ) -> Result<forge_platform_core::RateLimitOutcome, forge_platform_core::PlatformError> {
        Ok(forge_platform_core::RateLimitOutcome::Granted)
    }

    fn remaining(&self) -> u32 {
        u32::MAX
    }

    async fn observe_remote_throttle(&self, _retry_after: std::time::Duration) {}
}

fn boot_locale(
    rt: &tokio::runtime::Runtime,
    backend: Arc<dyn DataProvider>,
) -> forge_storage::Language {
    let settings: Arc<dyn forge_storage::SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
    let (lang, persist) = rt.block_on(forge_app::i18n::resolve_startup_language(settings.clone()));
    if let Some(detected) = persist
        && let Err(e) = rt.block_on(settings.set_language(detected))
    {
        tracing::warn!(error = %e, "failed to persist detected locale");
    }
    forge_app::i18n::install_language(lang);
    lang
}

fn boot_density(
    rt: &tokio::runtime::Runtime,
    backend: Arc<dyn DataProvider>,
) -> forge_storage::settings::Density {
    let settings: Arc<dyn forge_storage::SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
    let density = match rt.block_on(settings.density()) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read density setting; defaulting to cozy");
            forge_storage::settings::Density::Cozy
        }
    };
    forge_app::ui_settings::install_density(density);
    density
}

fn boot_theme(
    rt: &tokio::runtime::Runtime,
    backend: Arc<dyn DataProvider>,
) -> (iced::Theme, forge_widgets::ForgePalette) {
    let settings: Arc<dyn forge_storage::SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
    let theme_id = match rt.block_on(settings.get_theme()) {
        Ok(Some(key)) => forge_widgets::ThemeId::from_storage_key(&key)
            .unwrap_or(forge_widgets::ThemeId::CatppuccinMocha),
        Ok(None) => forge_widgets::ThemeId::CatppuccinMocha,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read theme setting; using default");
            forge_widgets::ThemeId::CatppuccinMocha
        }
    };
    forge_widgets::theme_assets(theme_id)
}

fn boot_fonts(
    rt: &tokio::runtime::Runtime,
    backend: Arc<dyn DataProvider>,
) -> forge_app::ui_settings::FontSettings {
    let settings: Arc<dyn forge_storage::SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
    let body = rt.block_on(settings.font_body()).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to read interface font setting; using bundled default");
        None
    });
    let mono = rt.block_on(settings.font_mono()).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to read monospace font setting; using bundled default");
        None
    });
    // Optimistic install before enumeration finishes: a missing family falls back to the
    // renderer's per-glyph defaults for at most a few frames, then catalog validation resets it.
    forge_widgets::install_font_override(forge_widgets::FontRole::Body, body.as_deref());
    forge_widgets::install_font_override(forge_widgets::FontRole::Monospace, mono.as_deref());
    forge_app::ui_settings::FontSettings::from_stored(body, mono)
}

fn boot_shortcuts(
    rt: &tokio::runtime::Runtime,
    backend: Arc<dyn DataProvider>,
) -> std::collections::HashMap<String, String> {
    let settings: Arc<dyn forge_storage::SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
    match rt
        .block_on(settings.get_string(forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS))
    {
        Ok(Some(raw)) => forge_app::settings_shortcuts::parse_stored_overrides(&raw),
        Ok(None) => std::collections::HashMap::new(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to read keyboard shortcuts; using defaults");
            std::collections::HashMap::new()
        }
    }
}

fn default_db_path() -> PathBuf {
    paths::data_dir().join("forge.db")
}

fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let console_layer = fmt::layer().with_target(false);

    let log_dir = paths::data_dir().join("logs");
    let (file_layer, guard) = match std::fs::create_dir_all(&log_dir) {
        Ok(()) => {
            let appender = tracing_appender::rolling::daily(&log_dir, "forge.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true);
            (Some(layer), Some(guard))
        }
        Err(_) => (None, None),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    if guard.is_some() {
        tracing::info!(path = %log_dir.display(), "file logging enabled");
    } else {
        tracing::warn!("file logging disabled: could not create log directory");
    }
    guard
}

fn boot_storage() -> (Arc<dyn DataProvider>, Option<String>) {
    let db_path = default_db_path();

    if let Some(parent) = db_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        let reason = format!("failed to create data directory {}: {e}", parent.display());
        tracing::error!("{reason}");
        return open_memory_backend(reason);
    }

    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            let reason = format!("failed to create tokio runtime for storage init: {e}");
            tracing::error!("{reason}");
            return open_memory_backend(reason);
        }
    };

    match rt.block_on(SqliteBackend::open(&url)) {
        Ok(backend) => (Arc::new(backend) as Arc<dyn DataProvider>, None),
        Err(e) => {
            let reason = format!("{e}");
            tracing::error!("failed to open database at {}: {reason}", db_path.display());
            open_memory_backend(reason)
        }
    }
}

#[allow(clippy::expect_used)]
fn open_memory_backend(reason: String) -> (Arc<dyn DataProvider>, Option<String>) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime required for in-memory storage");
    let backend = rt
        .block_on(SqliteBackend::open("sqlite::memory:"))
        .expect("in-memory SQLite must always open");
    (Arc::new(backend) as Arc<dyn DataProvider>, Some(reason))
}

struct RuntimeHandles {
    registry: Arc<ScriptRegistry>,
    engine: ActionEngineHandle,
    scheduler: QueueSchedulerHandle,
    speak_queue: Arc<SpeakQueueHandle>,
    pipeline_config: forge_speak_queue::PipelineConfigHandle,
    tts_trigger_settings: forge_runtime::TtsTriggerSettingsHandle,
    tts_engine_ids: Vec<EngineId>,
    tts_registry: Arc<std::sync::RwLock<TtsRegistry>>,
    sound_player: Arc<SoundboardPlayer>,
    sub_action_reg: Arc<SubActionRegistry>,
    trigger_reg: Arc<TriggerRegistry>,
    trigger_evaluator: TriggerEvaluatorHandle,
    vtube_client: Option<Arc<VTubeClient>>,
    vtube_sink: Arc<SwitchableVTubeSink>,
    obs_sink: Arc<SwitchableObsSink>,
    discord_client: Arc<DiscordClient>,
    midi_client: Option<Arc<MidiClient>>,
    kick_builtin: Option<Arc<forge_platform_kick::KickIntegrationBundle>>,
    youtube_builtin: Option<Arc<forge_platform_youtube::YoutubeIntegrationBundle>>,
}

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
fn resolve_audio_output_device(
    rt: &tokio::runtime::Runtime,
    settings: &Arc<dyn forge_storage::SettingsRepo>,
) -> Option<DeviceId> {
    let devices = match forge_audio::list_output_devices() {
        Ok(devices) => devices,
        Err(e) => {
            tracing::warn!(error = %e, "failed to enumerate audio output devices");
            return None;
        }
    };
    if devices.is_empty() {
        return None;
    }

    let stored = match rt.block_on(settings.audio_output_device_id()) {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read stored audio output device preference");
            None
        }
    };

    if let Some(stored_id) = stored {
        if let Some(found) = devices.iter().find(|d| d.id.as_str() == stored_id) {
            return Some(found.id.clone());
        }
        tracing::warn!(
            device = %stored_id,
            "stored audio output device no longer present; falling back to OS default"
        );
    }

    devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| d.id.clone())
        .or_else(|| devices.first().map(|d| d.id.clone()))
}

fn spawn_speak_queue(
    bus: Arc<EventBus>,
    creds: Arc<dyn forge_storage::CredentialsRepo>,
    filters_repo: Arc<dyn forge_storage::TtsFiltersRepo>,
    voice_alias_repo: Arc<dyn forge_storage::VoiceAliasRepo>,
    settings: Arc<dyn forge_storage::SettingsRepo>,
    rt: &tokio::runtime::Runtime,
) -> (
    Arc<SpeakQueueHandle>,
    Vec<EngineId>,
    forge_speak_queue::PipelineConfigHandle,
    Arc<std::sync::RwLock<TtsRegistry>>,
) {
    let mut registry = TtsRegistry::new();
    if let Some(piper_binary) = find_piper_binary() {
        let voices_dir = PiperEngine::voices_dir(&paths::data_dir());
        if let Err(e) = std::fs::create_dir_all(&voices_dir) {
            tracing::warn!(
                path = %voices_dir.display(),
                error = %e,
                "failed to create piper voices dir"
            );
        }
        registry.register(
            EngineId("piper".into()),
            Arc::new(PiperEngineFactory {
                piper_binary: piper_binary.clone(),
                voices_dir,
                timeout: std::time::Duration::from_secs(30),
            }),
        );
        tracing::info!(binary = %piper_binary.display(), "registered Piper TTS engine");
    } else {
        tracing::warn!("Piper binary not found in <exe_dir>/piper or PATH; TTS disabled");
    }

    match EspeakEngineFactory.create() {
        Ok(engine) => {
            let id = engine.engine_id().clone();
            registry.register(id, Arc::new(EspeakEngineFactory));
            tracing::info!("registered eSpeak-NG TTS engine");
        }
        Err(e) => {
            tracing::debug!(error = %e, "eSpeak-NG TTS engine unavailable");
        }
    }

    match SapiEngineFactory.create() {
        Ok(engine) => {
            let id = engine.engine_id().clone();
            registry.register(id, Arc::new(SapiEngineFactory));
            tracing::info!("registered SAPI 5 TTS engine");
        }
        Err(e) => {
            tracing::debug!(error = %e, "SAPI 5 TTS engine unavailable");
        }
    }

    match NsSpeechEngineFactory.create() {
        Ok(engine) => {
            let id = engine.engine_id().clone();
            registry.register(id, Arc::new(NsSpeechEngineFactory));
            tracing::info!("registered AVFoundation TTS engine");
        }
        Err(e) => {
            tracing::debug!(error = %e, "AVFoundation TTS engine unavailable");
        }
    }

    let registry = std::sync::RwLock::new(registry);
    rt.block_on(register_cloud_engines(&registry, creds.as_ref()));
    let registry = Arc::new(registry);
    let engine_ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();

    let (aliases, strategy, profile) = match rt.block_on(async {
        let aliases = voice_alias_repo.list().await?;
        let strategy = voice_alias_repo.get_strategy().await?;
        let profile = voice_alias_repo.get_ignore_profile().await?;
        Ok::<_, forge_storage::StorageError>((aliases, strategy, profile))
    }) {
        Ok(loaded) => loaded,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load voice aliases on boot; using defaults");
            (
                vec![],
                AssignmentStrategy::default(),
                IgnoreProfile::default(),
            )
        }
    };
    let resolver = Arc::new(std::sync::RwLock::new(VoiceAliasResolver::new(
        aliases,
        strategy,
        profile,
        SynthesisDefaults::default(),
    )));
    let pipeline_config = match rt.block_on(async {
        let rules = filters_repo.list_rules().await?;
        let settings = filters_repo.get_pipeline_settings().await?;
        Ok::<_, forge_storage::StorageError>((rules, settings))
    }) {
        Ok((rules, settings)) => forge_speak_queue::build_config_lenient(&rules, &settings),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load tts filters on boot; using defaults");
            forge_tts_pipeline::PipelineConfig::default()
        }
    };
    let pipeline = forge_speak_queue::PipelineConfigHandle::new(pipeline_config);

    let audio_sink: Arc<dyn forge_audio::AudioSink> =
        match resolve_audio_output_device(rt, &settings) {
            Some(device_id) => {
                let event_sink = Arc::new(forge_audio::NullAudioEventSink);
                tracing::info!(device = %device_id.0, "speak queue audio sink ready");
                Arc::new(CpalSink::new(device_id, None, None, event_sink))
            }
            None => {
                tracing::warn!("no audio output device found; speak queue using NullSink");
                Arc::new(NullSink)
            }
        };

    let registry_handle = Arc::clone(&registry);
    let deps = QueueDeps {
        registry,
        resolver,
        pipeline: pipeline.clone(),
        audio_sink,
        event_bus: bus as Arc<dyn forge_events::EventPublisher>,
    };
    let config = QueueConfig::default();
    let (handle, _stream) = forge_speak_queue::spawn(config, deps);
    (Arc::new(handle), engine_ids, pipeline, registry_handle)
}

#[allow(clippy::expect_used)]
fn spawn_runtime(dp: Arc<dyn DataProvider>, bus: Arc<EventBus>) -> Option<RuntimeHandles> {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("failed to create tokio runtime for runtime spawn: {e}");
            return None;
        }
    };

    let queues = match rt.block_on(dp.queue_repo().list()) {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!("failed to load queues on boot, starting with empty set: {e}");
            vec![]
        }
    };

    let creds_repo = Arc::clone(&dp) as Arc<dyn forge_storage::CredentialsRepo>;
    let settings_repo = Arc::clone(&dp) as Arc<dyn forge_storage::SettingsRepo>;
    let (speak_queue, tts_engine_ids, pipeline_config, tts_registry) = spawn_speak_queue(
        Arc::clone(&bus),
        creds_repo,
        dp.tts_filters_repo(),
        dp.voice_alias_repo(),
        settings_repo,
        &rt,
    );
    let _viewer_tracker = forge_app::viewer_tracker::spawn(Arc::clone(&bus), dp.viewer_repo());
    let speak_bridge_concrete = Arc::new(SpeakBridge::new(Arc::clone(&speak_queue)));
    let speak_dispatcher: Arc<dyn forge_runtime::SpeakDispatcher> = speak_bridge_concrete.clone();
    let speak_requester: Arc<dyn forge_script::SpeakRequester> = speak_bridge_concrete;

    let mut registry_mut = ScriptRegistry::new();
    registry_mut.set_speak_requester(speak_requester);
    let registry = Arc::new(registry_mut);
    if let Err(e) = rt.block_on(registry.load_all(dp.as_ref())) {
        tracing::warn!("script registry load failed at boot: {e}");
    }

    let clips_repo = dp.soundboard_clips_repo();
    let sound_player = Arc::new(SoundboardPlayer::new(
        Arc::new(CpalSinkFactory),
        Arc::new(BusAudioEventSink::new(Arc::clone(&bus))),
        clips_repo,
    ));

    let mut sub_action_reg = SubActionRegistry::new();
    let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let scheduler_cell = SchedulerCell::new();
    let cancel_registry = Arc::new(ActionCancelRegistry::new());
    if let Err(e) = register_core_sub_actions(
        &mut sub_action_reg,
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        Arc::clone(&registry),
        publisher,
        Arc::clone(&dp) as Arc<dyn SettingsRepo>,
        scheduler_cell.clone(),
        dp.trigger_instance_repo(),
        dp.action_repo(),
        Arc::clone(&cancel_registry),
        forge_runtime::Config::default(),
    ) {
        tracing::warn!("core sub-action runner registration failed: {e}");
    }
    let tts_trigger_settings = {
        let repo = dp.tts_trigger_settings_repo();
        let loaded = rt
            .block_on(repo.get_trigger_settings())
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "failed to load tts trigger settings on boot, using defaults");
                forge_storage::TtsTriggerSettings::default()
            });
        forge_runtime::TtsTriggerSettingsHandle::new(loaded)
    };
    if let Err(e) = register_audio_sub_actions(
        &mut sub_action_reg,
        Arc::clone(&sound_player) as Arc<dyn forge_runtime::SoundPlayer>,
        speak_dispatcher,
        tts_trigger_settings.clone(),
    ) {
        tracing::warn!("audio sub-action runner registration failed: {e}");
    }
    let discord_publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let discord_creds: Arc<dyn forge_storage::CredentialsRepo> =
        Arc::clone(&dp) as Arc<dyn forge_storage::CredentialsRepo>;
    let discord_client =
        DiscordClient::new(DiscordConfig::default(), discord_publisher, discord_creds);
    if let Err(e) = register_discord_sub_actions(&mut sub_action_reg, Arc::clone(&discord_client)) {
        tracing::warn!("discord sub-action runner registration failed: {e}");
    }
    let midi_publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let midi_client = match MidiClient::start_with_midir(MidiConfig::default(), midi_publisher) {
        Ok(c) => {
            if let Err(e) = register_midi_sub_actions(&mut sub_action_reg, Arc::clone(&c)) {
                tracing::warn!("midi sub-action runner registration failed: {e}");
            }
            Some(c)
        }
        Err(e) => {
            tracing::warn!(error = %e, "MIDI init failed; sub-actions unavailable");
            None
        }
    };
    let vtube_sink = SwitchableVTubeSink::new();
    if let Err(e) = register_vtube_sub_actions(
        &mut sub_action_reg,
        Arc::clone(&vtube_sink) as Arc<dyn VTubeSink>,
    ) {
        tracing::warn!("vtube sub-action runner registration failed: {e}");
    }
    let obs_sink = SwitchableObsSink::new();
    if let Err(e) = register_obs_sub_actions(
        &mut sub_action_reg,
        Arc::clone(&obs_sink) as Arc<dyn ObsSink>,
    ) {
        tracing::warn!("obs sub-action runner registration failed: {e}");
    }
    // One bucket for the whole Twitch Helix budget (800 points / 60s, per
    // client-id and global). Shared with the chat-send bridge below so the two
    // transports draw from the same budget instead of each getting a full one.
    let twitch_rate_limiter: Arc<dyn forge_platform_core::RateLimiter> = Arc::new(
        forge_platform_core::TokenBucketRateLimiter::new(800, std::time::Duration::from_secs(60)),
    );
    match forge_platform_twitch::client_id() {
        Some(cid) => {
            let twitch_creds: Arc<dyn forge_storage::CredentialsRepo> =
                Arc::clone(&dp) as Arc<dyn forge_storage::CredentialsRepo>;
            let twitch_manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
                Arc::clone(&twitch_creds),
                cid.clone(),
            ));
            // The manager is both the proactive token source (refresh-before-expiry on
            // every request) and the reactive 401 refresher.
            let twitch_transport: Arc<dyn HelixTransport> = Arc::new(
                HelixHttpTransport::new(
                    Arc::clone(&twitch_rate_limiter),
                    Arc::clone(&bus) as Arc<dyn EventPublisher>,
                    cid,
                    Arc::clone(&twitch_manager) as Arc<dyn forge_platform_twitch::HelixTokenSource>,
                )
                .with_refresher(Arc::clone(&twitch_manager) as Arc<dyn HelixTokenRefresher>),
            );
            if let Err(e) =
                register_twitch_sub_actions(&mut sub_action_reg, twitch_transport, twitch_creds)
            {
                tracing::warn!("twitch sub-action runner registration failed: {e}");
            }
        }
        None => {
            tracing::warn!("no Twitch client_id configured; twitch sub-actions unavailable");
        }
    }
    let mut youtube_boot_bundle: Option<Arc<forge_platform_youtube::YoutubeIntegrationBundle>> =
        None;
    if let Some((yt_id, yt_secret)) = forge_platform_youtube::client_credentials() {
        let google = forge_platform_youtube::GoogleAuthFlow::new(yt_id, yt_secret);
        let yt_creds: Arc<dyn CredentialsRepo> = Arc::clone(&dp) as Arc<dyn CredentialsRepo>;
        let manager = Arc::new(forge_platform_youtube::YoutubeCredentialsManager::new(
            yt_creds, google,
        ));
        match rt.block_on(manager.load()) {
            Ok(Some(creds)) => {
                let channel_id = creds.channel_id.clone();

                let yt_live_chat_id = forge_platform_youtube::LiveChatIdHandle::new();
                let yt_active_broadcast = forge_platform_youtube::ActiveBroadcastIdHandle::new();
                let yt_quota = Arc::new(tokio::sync::Mutex::new(
                    forge_platform_youtube::QuotaState::default(),
                ));

                let manager_for_send = Arc::clone(&manager);
                let yt_send = Arc::new(forge_platform_youtube::YoutubeSendChat::new(
                    Arc::new(move || {
                        let m = Arc::clone(&manager_for_send);
                        Box::pin(async move { m.get_valid_access_token().await })
                    }),
                    yt_live_chat_id.clone(),
                    Arc::clone(&yt_quota),
                ));

                let manager_for_mod = Arc::clone(&manager);
                let yt_moderation = Arc::new(forge_platform_youtube::YoutubeModeration::new(
                    Arc::new(move || {
                        let m = Arc::clone(&manager_for_mod);
                        Box::pin(async move { m.get_valid_access_token().await })
                    }),
                    yt_live_chat_id.clone(),
                    Arc::clone(&yt_quota),
                ));

                let manager_for_meta = Arc::clone(&manager);
                let yt_metadata = Arc::new(forge_platform_youtube::YoutubeStreamMetadata::new(
                    Arc::new(move || {
                        let m = Arc::clone(&manager_for_meta);
                        Box::pin(async move { m.get_valid_access_token().await })
                    }),
                    yt_active_broadcast.clone(),
                    Arc::clone(&yt_quota),
                ));

                if let Err(e) = forge_platform_youtube::register_youtube_sub_actions(
                    &mut sub_action_reg,
                    yt_send,
                    yt_moderation,
                    yt_metadata,
                ) {
                    tracing::warn!("youtube sub-action runner registration failed: {e}");
                }

                let platform = Arc::new(forge_platform_youtube::YoutubePlatform::new(
                    channel_id.clone(),
                    Arc::clone(&manager),
                    yt_live_chat_id,
                    yt_active_broadcast,
                    Arc::clone(&yt_quota),
                ));
                let chat_platform: Arc<dyn forge_platform_core::ChatPlatform> =
                    Arc::clone(&platform) as _;

                let (youtube_bundle, _youtube_health_tx) =
                    forge_platform_youtube::YoutubeIntegrationBundle::new(
                        channel_id,
                        Arc::clone(&platform),
                        Arc::clone(&manager),
                        Arc::clone(&yt_quota),
                    );
                youtube_boot_bundle = Some(youtube_bundle);

                // Republish the platform's own event stream (chat receive + connection
                // state transitions) onto the global bus.
                let bus_events = Arc::clone(&bus);
                let mut platform_events = chat_platform.events();
                tokio::spawn(async move {
                    loop {
                        match platform_events.recv().await {
                            Ok(event) => bus_events.publish(event),
                            Err(forge_events::EventsError::BusClosed) => break,
                            Err(forge_events::EventsError::LaggingReceiver) => {
                                tracing::warn!("youtube platform event bridge: lagging receiver");
                                continue;
                            }
                            Err(_) => continue,
                        }
                    }
                });

                if let Err(e) = rt.block_on(chat_platform.connect()) {
                    tracing::warn!(error = %e, "youtube chat connect failed");
                }

                // Consume `chat.send.request` targeted at youtube and send through the
                // object, emitting the same chat.send / chat.send.failed outcome events.
                let bus_send = Arc::clone(&bus);
                let send_platform = Arc::clone(&chat_platform);
                tokio::spawn(async move {
                    let mut sub = bus_send.subscribe();
                    loop {
                        let event = match sub.recv().await {
                            Ok(e) => e,
                            Err(forge_events::EventsError::BusClosed) => break,
                            Err(forge_events::EventsError::LaggingReceiver) => {
                                tracing::warn!("youtube_send: lagging receiver");
                                continue;
                            }
                            Err(_) => continue,
                        };
                        if event.source != forge_events::EventSource::Core
                            || event.kind != "chat.send.request"
                        {
                            continue;
                        }
                        let target = event
                            .payload
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if target != "youtube" {
                            continue;
                        }
                        let message = match event
                            .payload
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned)
                        {
                            Some(m) => m,
                            None => continue,
                        };
                        let caused_by = event.id;
                        match send_platform.send_message("youtube", &message).await {
                            Ok(()) => {
                                bus_send.publish(forge_events::Event::caused_by(
                                    forge_events::EventSource::YouTube,
                                    "chat.send",
                                    serde_json::json!({"channel": "youtube", "message": message}),
                                    caused_by,
                                ));
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "youtube chat send failed");
                                bus_send.publish(forge_events::Event::caused_by(
                                    forge_events::EventSource::YouTube,
                                    "chat.send.failed",
                                    serde_json::json!({"target": "youtube", "error": e.to_string()}),
                                    caused_by,
                                ));
                            }
                        }
                    }
                });
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, "failed to load youtube credentials at boot");
            }
        }
    }

    let mut trigger_reg = TriggerRegistry::new();
    if let Err(e) = register_core_triggers(&mut trigger_reg) {
        tracing::warn!("core trigger descriptor registration failed: {e}");
    }
    if let Err(e) = register_twitch_triggers(&mut trigger_reg) {
        tracing::warn!("twitch trigger descriptor registration failed: {e}");
    }
    if let Err(e) = register_obs_triggers(&mut trigger_reg) {
        tracing::warn!("obs trigger descriptor registration failed: {e}");
    }
    if let Err(e) = register_midi_triggers(&mut trigger_reg) {
        tracing::warn!("midi trigger descriptor registration failed: {e}");
    }
    if let Err(e) = register_hotkey_triggers(&mut trigger_reg) {
        tracing::warn!("hotkey trigger descriptor registration failed: {e}");
    }
    if let Err(e) = register_vtube_triggers(&mut trigger_reg) {
        tracing::warn!("vtube trigger descriptor registration failed: {e}");
    }
    if let Err(e) = forge_platform_youtube::register_youtube_triggers(&mut trigger_reg) {
        tracing::warn!("youtube trigger descriptor registration failed: {e}");
    }
    if let Err(e) = forge_platform_kick::register_kick_triggers(&mut trigger_reg) {
        tracing::warn!("kick trigger descriptor registration failed: {e}");
    }
    let trigger_reg = Arc::new(trigger_reg);
    let trigger_instance_repo = dp.trigger_instance_repo();
    for descriptor in trigger_reg.all() {
        let kind_id = descriptor.id();
        let name = descriptor.label();
        if let Err(e) = rt.block_on(trigger_instance_repo.upsert_default(kind_id, name)) {
            tracing::warn!("upsert_default failed for kind_id={kind_id}: {e}");
        }
    }

    let mut kick_boot_bundle: Option<Arc<forge_platform_kick::KickIntegrationBundle>> = None;
    if let Some(kick_client_id) = forge_platform_kick::client_credentials() {
        let kk_creds: Arc<dyn CredentialsRepo> = Arc::clone(&dp) as Arc<dyn CredentialsRepo>;
        let kk_http = reqwest::Client::new();
        let manager = Arc::new(forge_platform_kick::KickCredentialsManager::new(
            kk_creds,
            kk_http.clone(),
            kick_client_id,
        ));
        match rt.block_on(manager.load()) {
            Ok(Some(creds)) => {
                let slug = creds.username.clone();
                let broadcaster_user_id = creds.user_id;

                let kick_rate_limiter: Arc<dyn forge_platform_core::RateLimiter> =
                    Arc::new(forge_platform_core::TokenBucketRateLimiter::new(
                        60,
                        std::time::Duration::from_secs(60),
                    ));

                let platform = Arc::new(forge_platform_kick::KickPlatform::new(
                    slug.clone(),
                    Arc::clone(&manager),
                    Arc::clone(&kick_rate_limiter),
                ));
                let chat_platform: Arc<dyn forge_platform_core::ChatPlatform> =
                    Arc::clone(&platform) as _;

                // Republish the platform's own event stream (chat receive + connection
                // state transitions) onto the global bus.
                let bus_events = Arc::clone(&bus);
                let mut platform_events = chat_platform.events();
                tokio::spawn(async move {
                    loop {
                        match platform_events.recv().await {
                            Ok(event) => bus_events.publish(event),
                            Err(forge_events::EventsError::BusClosed) => break,
                            Err(forge_events::EventsError::LaggingReceiver) => {
                                tracing::warn!("kick platform event bridge: lagging receiver");
                                continue;
                            }
                            Err(_) => continue,
                        }
                    }
                });

                if let Err(e) = rt.block_on(chat_platform.connect()) {
                    tracing::warn!(error = %e, "kick chat connect failed");
                }

                // The poller emits livestream + reward-redemption events; bridge its own
                // channel onto the bus (the chat channel is owned by the platform).
                let (poller_tx, mut poller_rx) =
                    tokio::sync::mpsc::channel::<forge_events::Event>(256);
                let bus_poller = Arc::clone(&bus);
                tokio::spawn(async move {
                    while let Some(event) = poller_rx.recv().await {
                        bus_poller.publish(event);
                    }
                });

                let limiter: Arc<dyn forge_platform_core::RateLimiter> = Arc::new(KickNoopLimiter);
                let sender = Arc::new(forge_platform_kick::KickSendChat::new(limiter));

                let moderation = Arc::new(forge_platform_kick::KickModeration::new(Arc::clone(
                    &kick_rate_limiter,
                )));
                let channel = Arc::new(forge_platform_kick::KickChannel::new(Arc::clone(
                    &kick_rate_limiter,
                )));
                let rewards = Arc::new(forge_platform_kick::KickRewards::new(Arc::clone(
                    &kick_rate_limiter,
                )));

                let manager_for_subactions = Arc::clone(&manager);
                if let Err(e) = forge_platform_kick::register_kick_sub_actions(
                    &mut sub_action_reg,
                    forge_platform_kick::KickSubActionDeps {
                        client: Arc::clone(&sender),
                        token_source: Arc::new(move || {
                            let m = Arc::clone(&manager_for_subactions);
                            Box::pin(async move { m.get_valid_access_token().await })
                        }),
                        broadcaster_user_id,
                        moderation,
                        channel: Arc::clone(&channel),
                        rewards: Arc::clone(&rewards),
                    },
                ) {
                    tracing::warn!("kick sub-action runner registration failed: {e}");
                }

                let manager_for_poller = Arc::clone(&manager);
                forge_platform_kick::spawn_kick_poller(
                    channel,
                    rewards,
                    Arc::new(move || {
                        let m = Arc::clone(&manager_for_poller);
                        Box::pin(async move { m.get_valid_access_token().await })
                    }),
                    poller_tx,
                );

                let (kick_bundle, _kick_health_tx) =
                    forge_platform_kick::KickIntegrationBundle::new(
                        slug,
                        Arc::clone(&platform),
                        Arc::clone(&manager),
                    );
                kick_boot_bundle = Some(Arc::clone(&kick_bundle));

                // Consume `chat.send.request` targeted at kick and send through the object,
                // emitting the same chat.send / chat.send.failed outcome events.
                let bus_send = Arc::clone(&bus);
                let send_platform = Arc::clone(&chat_platform);
                tokio::spawn(async move {
                    let mut sub = bus_send.subscribe();
                    loop {
                        let event = match sub.recv().await {
                            Ok(e) => e,
                            Err(forge_events::EventsError::BusClosed) => break,
                            Err(forge_events::EventsError::LaggingReceiver) => {
                                tracing::warn!("kick_send: lagging receiver");
                                continue;
                            }
                            Err(_) => continue,
                        };
                        if event.source != forge_events::EventSource::Core
                            || event.kind != "chat.send.request"
                        {
                            continue;
                        }
                        let target = event
                            .payload
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if target != "kick" {
                            continue;
                        }
                        let message = match event
                            .payload
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned)
                        {
                            Some(m) => m,
                            None => continue,
                        };
                        let caused_by = event.id;
                        match send_platform.send_message("kick", &message).await {
                            Ok(()) => {
                                bus_send.publish(forge_events::Event::caused_by(
                                    forge_events::EventSource::Kick,
                                    "chat.send",
                                    serde_json::json!({"channel": "kick", "message": message}),
                                    caused_by,
                                ));
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "kick chat send failed");
                                bus_send.publish(forge_events::Event::caused_by(
                                    forge_events::EventSource::Kick,
                                    "chat.send.failed",
                                    serde_json::json!({"target": "kick", "error": e.to_string()}),
                                    caused_by,
                                ));
                            }
                        }
                    }
                });
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, "failed to load kick credentials at boot");
            }
        }
    }

    let sub_action_reg = Arc::new(sub_action_reg);
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::clone(&sub_action_reg),
        cancel_registry,
    );
    let scheduler = QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
    scheduler_cell.set(scheduler.clone());
    let trigger_evaluator = spawn_trigger_evaluator(
        Arc::clone(&bus),
        Arc::clone(&trigger_reg),
        dp.action_repo(),
        dp.trigger_instance_repo(),
        scheduler.clone(),
    );
    // Twitch chat send now routes through the `ChatPlatform` object instead of the
    // removed ChatSendBridge. The platform owns its Helix transport + the shared
    // rate-limit bucket; we bridge its private event stream onto the global bus and
    // feed it the same `chat.send.request` events the bridge consumed.
    if let Some(twitch_client_id) = forge_platform_twitch::client_id() {
        let twitch_creds: Arc<dyn CredentialsRepo> = Arc::clone(&dp) as Arc<dyn CredentialsRepo>;
        let stored = rt
            .block_on(forge_platform_twitch::credentials::load(&*twitch_creds))
            .ok()
            .flatten();
        let user_id = stored.map(|s| s.user_id).unwrap_or_default();
        let config = ChatSessionConfig {
            client_id: twitch_client_id,
            broadcaster_id: user_id.clone(),
            user_id,
        };
        let platform: Arc<dyn forge_platform_core::ChatPlatform> = Arc::new(TwitchPlatform::new(
            config,
            Arc::clone(&twitch_creds),
            SubscriptionTracker::default(),
            Arc::clone(&twitch_rate_limiter),
        ));

        // Republish the platform's own event stream (chat, connection, Helix
        // observability) onto the global bus, preserving the direct-to-bus reach the
        // old per-crate publishers had.
        let bus_events = Arc::clone(&bus);
        let mut platform_events = platform.events();
        tokio::spawn(async move {
            loop {
                match platform_events.recv().await {
                    Ok(event) => bus_events.publish(event),
                    Err(forge_events::EventsError::BusClosed) => break,
                    Err(forge_events::EventsError::LaggingReceiver) => {
                        tracing::warn!("twitch platform event bridge: lagging receiver");
                        continue;
                    }
                    Err(_) => continue,
                }
            }
        });

        // Replaces ChatSendBridge: consume `chat.send.request` targeted at twitch and
        // send through the object, emitting the same chat.send / chat.send.failed
        // outcome events the bridge published.
        let bus_send = Arc::clone(&bus);
        let send_platform = Arc::clone(&platform);
        tokio::spawn(async move {
            let mut sub = bus_send.subscribe();
            loop {
                let event = match sub.recv().await {
                    Ok(e) => e,
                    Err(forge_events::EventsError::BusClosed) => break,
                    Err(forge_events::EventsError::LaggingReceiver) => {
                        tracing::warn!("twitch_send: lagging receiver");
                        continue;
                    }
                    Err(_) => continue,
                };
                if event.source != forge_events::EventSource::Core
                    || event.kind != "chat.send.request"
                {
                    continue;
                }
                let target = event
                    .payload
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if target != "twitch" {
                    continue;
                }
                let message = match event
                    .payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned)
                {
                    Some(m) => m,
                    None => continue,
                };
                let caused_by = event.id;
                match send_platform.send_message("twitch", &message).await {
                    Ok(()) => {
                        bus_send.publish(forge_events::Event::caused_by(
                            forge_events::EventSource::Twitch,
                            "chat.send",
                            serde_json::json!({"channel": "twitch", "message": message}),
                            caused_by,
                        ));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "twitch chat send failed");
                        bus_send.publish(forge_events::Event::caused_by(
                            forge_events::EventSource::Twitch,
                            "chat.send.failed",
                            serde_json::json!({"target": "twitch", "error": e.to_string()}),
                            caused_by,
                        ));
                    }
                }
            }
        });
    }

    Some(RuntimeHandles {
        registry,
        engine,
        scheduler,
        speak_queue,
        pipeline_config,
        tts_trigger_settings,
        tts_engine_ids,
        tts_registry,
        sound_player,
        sub_action_reg,
        trigger_reg,
        trigger_evaluator,
        vtube_client: None,
        vtube_sink,
        obs_sink,
        discord_client,
        midi_client,
        kick_builtin: kick_boot_bundle,
        youtube_builtin: youtube_boot_bundle,
    })
}

fn main() -> iced::Result {
    let _log_guard = init_tracing();
    tracing::info!("forge starting");

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("failed to create tokio runtime: {e}");
            return Ok(());
        }
    };
    let _runtime_guard = runtime.enter();

    let (backend, storage_failure) = boot_storage();
    let storage_offline = storage_failure.is_some();
    let boot_language = boot_locale(&runtime, Arc::clone(&backend));
    let boot_density = boot_density(&runtime, Arc::clone(&backend));
    let boot_theme = boot_theme(&runtime, Arc::clone(&backend));
    let boot_font_settings = boot_fonts(&runtime, Arc::clone(&backend));
    let boot_shortcut_overrides = boot_shortcuts(&runtime, Arc::clone(&backend));
    // A real DB-open failure must be unmissable: open on the error screen so the user
    // never silently runs against throwaway in-memory storage.
    let initial_screen = match storage_failure {
        Some(reason) => Screen::Error(reason),
        None => Screen::Home,
    };

    let event_log = backend.event_log_repo();
    let bus = EventBus::new(event_log);
    EventBus::spawn_flush_task(Arc::clone(&bus));

    let (
        script_registry,
        action_engine,
        scheduler,
        speak_queue,
        pipeline_config,
        tts_trigger_settings,
        tts_engine_ids,
        tts_registry,
        sound_player,
        sub_action_reg,
        trigger_reg,
        _trigger_evaluator,
        _vtube_client,
        vtube_sink,
        obs_sink,
        discord_client,
        midi_client,
        kick_builtin_handle,
        youtube_builtin_handle,
    ) = if storage_offline {
        let dc_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as _;
        let dc_creds: Arc<dyn forge_storage::CredentialsRepo> = Arc::clone(&backend) as _;
        let dc = DiscordClient::new(DiscordConfig::default(), dc_pub, dc_creds);
        let mc_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as _;
        let mc = MidiClient::start_with_midir(MidiConfig::default(), mc_pub).ok();
        let vs = SwitchableVTubeSink::new();
        let mut sar = SubActionRegistry::new();
        if let Err(e) = register_vtube_sub_actions(&mut sar, Arc::clone(&vs) as Arc<dyn VTubeSink>)
        {
            tracing::warn!("vtube sub-action runner registration failed: {e}");
        }
        let os = SwitchableObsSink::new();
        if let Err(e) = register_obs_sub_actions(&mut sar, Arc::clone(&os) as Arc<dyn ObsSink>) {
            tracing::warn!("obs sub-action runner registration failed: {e}");
        }
        (
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
            None,
            None,
            Vec::<EngineId>::new(),
            None,
            None,
            Arc::new(sar),
            Arc::new(TriggerRegistry::new()),
            None,
            None::<Arc<VTubeClient>>,
            vs,
            os,
            dc,
            mc,
            None::<Arc<forge_platform_kick::KickIntegrationBundle>>,
            None::<Arc<forge_platform_youtube::YoutubeIntegrationBundle>>,
        )
    } else {
        match spawn_runtime(Arc::clone(&backend), Arc::clone(&bus)) {
            Some(h) => (
                h.registry,
                Some(h.engine),
                Some(h.scheduler),
                Some(h.speak_queue),
                Some(h.pipeline_config),
                Some(h.tts_trigger_settings),
                h.tts_engine_ids,
                Some(h.tts_registry),
                Some(h.sound_player),
                h.sub_action_reg,
                h.trigger_reg,
                Some(h.trigger_evaluator),
                h.vtube_client,
                h.vtube_sink,
                h.obs_sink,
                h.discord_client,
                h.midi_client,
                h.kick_builtin,
                h.youtube_builtin,
            ),
            None => {
                let dc_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as _;
                let dc_creds: Arc<dyn forge_storage::CredentialsRepo> = Arc::clone(&backend) as _;
                let dc = DiscordClient::new(DiscordConfig::default(), dc_pub, dc_creds);
                let mc_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as _;
                let mc = MidiClient::start_with_midir(MidiConfig::default(), mc_pub).ok();
                let vs = SwitchableVTubeSink::new();
                let mut sar = SubActionRegistry::new();
                if let Err(e) =
                    register_vtube_sub_actions(&mut sar, Arc::clone(&vs) as Arc<dyn VTubeSink>)
                {
                    tracing::warn!("vtube sub-action runner registration failed: {e}");
                }
                let os = SwitchableObsSink::new();
                if let Err(e) =
                    register_obs_sub_actions(&mut sar, Arc::clone(&os) as Arc<dyn ObsSink>)
                {
                    tracing::warn!("obs sub-action runner registration failed: {e}");
                }
                (
                    Arc::new(ScriptRegistry::new()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Vec::<EngineId>::new(),
                    None,
                    None,
                    Arc::new(sar),
                    Arc::new(TriggerRegistry::new()),
                    None,
                    None::<Arc<VTubeClient>>,
                    vs,
                    os,
                    dc,
                    mc,
                    None::<Arc<forge_platform_kick::KickIntegrationBundle>>,
                    None::<Arc<forge_platform_youtube::YoutubeIntegrationBundle>>,
                )
            }
        }
    };

    let backend_boot: Arc<dyn DataProvider> = Arc::clone(&backend);
    let boot_screen = Arc::new(initial_screen);
    let bus_boot = Arc::clone(&bus);
    let boot = move || {
        let mut app = App::default_with(
            (*boot_screen).clone(),
            Arc::clone(&backend_boot),
            storage_offline,
            Arc::clone(&script_registry),
            action_engine.clone(),
            scheduler.clone(),
            sound_player.clone(),
            boot_language,
            boot_density,
            boot_font_settings.clone(),
        );
        app.ui.settings_shortcuts.overrides = boot_shortcut_overrides.clone();
        app.theme = boot_theme.0.clone();
        app.palette = boot_theme.1;
        app.rt.bus = Arc::clone(&bus_boot);
        app.rt.speak_queue = speak_queue.clone();
        app.rt.pipeline_config = pipeline_config.clone();
        app.rt.tts_trigger_settings = tts_trigger_settings.clone();
        app.rt.tts_engine_ids = tts_engine_ids.clone();
        app.rt.tts_registry = tts_registry.clone();
        app.rt.sub_action_registry = Arc::clone(&sub_action_reg);
        app.rt.trigger_registry = Arc::clone(&trigger_reg);
        app.rt.vtube_sink = Arc::clone(&vtube_sink);
        app.rt.obs_sink = Arc::clone(&obs_sink);
        let obs_creds: Arc<dyn forge_storage::CredentialsRepo> =
            Arc::clone(&backend_boot) as Arc<dyn forge_storage::CredentialsRepo>;
        let obs_task = iced::Task::perform(
            load_obs_and_connect(obs_creds, Arc::clone(&bus_boot)),
            |r| forge_app::Message::Boot(forge_app::BootMsg::Obs(r)),
        );
        let twitch_creds: Arc<dyn forge_storage::CredentialsRepo> =
            Arc::clone(&backend_boot) as Arc<dyn forge_storage::CredentialsRepo>;
        let twitch_task = iced::Task::perform(load_twitch_credential(twitch_creds), |r| {
            forge_app::Message::Boot(forge_app::BootMsg::Twitch(r))
        });
        let vtube_creds: Arc<dyn forge_storage::CredentialsRepo> =
            Arc::clone(&backend_boot) as Arc<dyn forge_storage::CredentialsRepo>;
        let vtube_task = iced::Task::perform(
            load_vtube_and_connect(vtube_creds, Arc::clone(&bus_boot)),
            |r| forge_app::Message::Boot(forge_app::BootMsg::Vtube(r)),
        );
        let kick_task = match kick_builtin_handle.clone() {
            Some(bundle) => iced::Task::done(forge_app::Message::Boot(forge_app::BootMsg::Kick(
                Ok(forge_app::message::KickBundleRef::new(bundle)),
            ))),
            None => iced::Task::none(),
        };
        let youtube_task = match youtube_builtin_handle.clone() {
            Some(bundle) => iced::Task::done(forge_app::Message::Boot(
                forge_app::BootMsg::Youtube(Ok(forge_app::message::YoutubeBundleRef::new(bundle))),
            )),
            None => iced::Task::none(),
        };
        let discord_task =
            iced::Task::done(forge_app::Message::Boot(forge_app::BootMsg::Discord(Ok(
                forge_app::message::DiscordClientRef::new(Arc::clone(&discord_client)),
            ))));
        let midi_task = match midi_client.as_ref() {
            Some(c) => iced::Task::done(forge_app::Message::Boot(forge_app::BootMsg::Midi(Ok(
                forge_app::message::MidiClientRef::new(Arc::clone(c)),
            )))),
            None => iced::Task::done(forge_app::Message::Boot(forge_app::BootMsg::Midi(Err(
                "MIDI unavailable".to_owned(),
            )))),
        };
        let font_catalog_task = iced::Task::perform(
            async {
                match tokio::task::spawn_blocking(forge_widgets::enumerate_font_families).await {
                    Ok(families) => families,
                    Err(e) => {
                        tracing::warn!(error = %e, "font enumeration task failed");
                        Vec::new()
                    }
                }
            },
            |families| {
                forge_app::Message::Settings(forge_app::message::SettingsMsg::FontCatalogLoaded(
                    families,
                ))
            },
        );
        let bus_hotkey = Arc::clone(&bus_boot);
        let hotkey_backend = Arc::clone(&backend_boot);
        let hotkey_task = iced::Task::perform(
            async move {
                let client = load_hotkey_and_register(hotkey_backend, bus_hotkey).await;
                Ok::<forge_app::message::HotkeyClientRef, String>(client)
            },
            |r| forge_app::Message::Boot(forge_app::BootMsg::Hotkey(r)),
        );
        // Cold-boot Home stats: re-enter the tested `HomeMsg::LoadStats` handler
        // (its own `Task::perform` off-thread compute) so first launch shows real
        // dashboard numbers immediately instead of em-dash placeholders.
        let home_stats_task =
            iced::Task::done(forge_app::Message::Home(forge_app::HomeMsg::LoadStats));
        let boot_task = match app.rt.action_engine.clone() {
            Some(engine) => {
                let dp = Arc::clone(&backend_boot);
                let server_boot_task = iced::Task::perform(
                    forge_app::server_subsystem::load_server_settings_and_start(
                        dp,
                        Arc::clone(&bus_boot),
                        std::sync::Arc::new(engine),
                        Arc::clone(&app.rt.server_subsystem),
                    ),
                    |r| forge_app::Message::Boot(forge_app::BootMsg::Server(r)),
                );
                iced::Task::batch([
                    obs_task,
                    twitch_task,
                    kick_task,
                    youtube_task,
                    vtube_task,
                    discord_task,
                    midi_task,
                    hotkey_task,
                    font_catalog_task,
                    server_boot_task,
                    home_stats_task,
                ])
            }
            None => iced::Task::batch([
                obs_task,
                twitch_task,
                kick_task,
                youtube_task,
                vtube_task,
                discord_task,
                midi_task,
                hotkey_task,
                font_catalog_task,
                home_stats_task,
            ]),
        };
        (app, boot_task)
    };

    let mut app = iced::application(boot, update, view)
        .title("forge")
        .subscription(subscription)
        .theme(theme_callback)
        .exit_on_close_request(false);
    for font_bytes in forge_widgets::load_fonts() {
        app = app.font(font_bytes);
    }
    app.run()
}
