use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{Density, ThemeId};
use forge_events::EventPublisher;
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_platform_core::paths;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Config, EventBus, OverlayConnectListener,
    OverlayFrameSink, OverlayServiceCell, OverlayServiceHandle, QueueScheduler, SchedulerCell,
    ScriptRegistry, SoundPlayer, SpeakDispatcher, register_audio_sub_actions,
    register_core_sub_actions, register_core_triggers, spawn_action_engine,
    spawn_chat_history_persistence, spawn_chat_moderation_persistence,
    spawn_live_viewer_aggregator, spawn_trigger_evaluator, spawn_viewer_tracker,
};
use forge_soundboard::{
    BusAudioEventSink, CpalSinkFactory, SoundboardPlayer, SoundboardSettingsHandle,
    load_soundboard_settings,
};
use forge_storage::{
    CredentialsRepo, DataProvider, GlobalsRepo, ScriptRepo, SettingsRepo, StorageError,
    UserGlobalsRepo, reserved_keys,
};
use forge_storage_sqlite::SqliteBackend;

use crate::integrations::build_integrations;
use crate::log_tail::LogTail;
use crate::overlay_frame_sink::ServerOverlayFrameSink;
use crate::runtime_handles::RuntimeHandles;
use crate::speak_boot::build_speak_queue;
use crate::speak_bridge::SpeakBridge;
use crate::voice_gate::{VoiceGateOwner, config_from_settings};

pub enum BootFailure {
    UpgradeRequired { expected: u32, found: u32 },
    Retry { reason: String },
}

fn default_db_path() -> PathBuf {
    paths::data_dir().join("forge.db")
}

/// Blocks the calling thread (bounded by a timeout) so the window can open already themed.
/// Uses a std channel, not `Handle::block_on`, because the caller sits inside `rt.enter()`.
pub fn read_persisted_presentation(rt_handle: &tokio::runtime::Handle) -> (ThemeId, Density) {
    let (tx, rx) = std::sync::mpsc::channel();
    rt_handle.spawn(async move {
        let _ = tx.send(load_presentation_from_storage().await);
    });
    let (theme, density) = rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default();
    tracing::info!(?theme, ?density, "applied persisted presentation");
    (theme, density)
}

async fn load_presentation_from_storage() -> (ThemeId, Density) {
    let db_path = default_db_path();
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let backend = match SqliteBackend::open(&url).await {
        Ok(backend) => backend,
        Err(_) => return (ThemeId::default(), Density::default()),
    };
    let settings: &dyn SettingsRepo = &backend;
    let theme = match settings.get_theme().await {
        Ok(Some(key)) => ThemeId::from_storage_key(&key).unwrap_or_default(),
        _ => ThemeId::default(),
    };
    let density = match settings.get_string(reserved_keys::DENSITY).await {
        Ok(Some(key)) => Density::from_storage_key(&key).unwrap_or_default(),
        _ => Density::default(),
    };
    (theme, density)
}

pub fn read_persisted_fonts(
    rt_handle: &tokio::runtime::Handle,
) -> (Option<String>, Option<String>) {
    let (tx, rx) = std::sync::mpsc::channel();
    rt_handle.spawn(async move {
        let _ = tx.send(load_fonts_from_storage().await);
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
}

async fn load_fonts_from_storage() -> (Option<String>, Option<String>) {
    let db_path = default_db_path();
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let backend = match SqliteBackend::open(&url).await {
        Ok(backend) => backend,
        Err(_) => return (None, None),
    };
    let settings: &dyn SettingsRepo = &backend;
    let body = settings.font_body().await.ok().flatten();
    let mono = settings.font_mono().await.ok().flatten();
    (body, mono)
}

/// Must run within the tokio runtime: the engine/scheduler/evaluator spawn tasks internally.
pub async fn build_runtime(log_tail: LogTail) -> Result<RuntimeHandles, BootFailure> {
    let db_path = default_db_path();
    if let Some(parent) = db_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return Err(BootFailure::Retry {
            reason: format!("failed to create data directory {}: {e}", parent.display()),
        });
    }
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let backend = match SqliteBackend::open(&url).await {
        Ok(backend) => Arc::new(backend) as Arc<dyn DataProvider>,
        Err(e) => {
            let err: StorageError = e.into();
            return Err(match err {
                StorageError::SchemaMismatch { expected, found } => {
                    BootFailure::UpgradeRequired { expected, found }
                }
                other => BootFailure::Retry {
                    reason: other.to_string(),
                },
            });
        }
    };

    let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(&backend) as Arc<dyn SettingsRepo>;
    let (startup_language, persist) =
        crate::i18n::resolve_startup_language(Arc::clone(&settings_repo)).await;
    if let Some(detected) = persist
        && let Err(e) = settings_repo.set_language(detected).await
    {
        eprintln!("forge-desktop: failed to persist detected locale: {e}");
    }

    let bus = EventBus::new(backend.event_log_repo());
    EventBus::spawn_flush_task(Arc::clone(&bus));
    spawn_chat_history_persistence(
        Arc::clone(&bus),
        backend.chat_history_repo(),
        Arc::clone(&settings_repo),
    );
    spawn_viewer_tracker(Arc::clone(&bus), backend.viewer_repo());
    spawn_chat_moderation_persistence(Arc::clone(&bus), backend.chat_history_repo());

    let (speak, speak_events, pipeline_config, tts_registry) =
        build_speak_queue(&bus, &backend).await;
    let voice_gate = build_voice_gate(settings_repo.as_ref(), speak.clone()).await;
    let speak_bridge = speak
        .clone()
        .map(|handle| Arc::new(SpeakBridge::new(Arc::new(handle))));
    let speak_dispatcher: Option<Arc<dyn SpeakDispatcher>> = speak_bridge
        .clone()
        .map(|bridge| bridge as Arc<dyn SpeakDispatcher>);
    let speak_requester: Option<Arc<dyn forge_script::SpeakRequester>> =
        speak_bridge.map(|bridge| bridge as Arc<dyn forge_script::SpeakRequester>);

    let mut script_registry_mut = ScriptRegistry::new();
    match speak_requester {
        Some(requester) => script_registry_mut.set_speak_requester(requester),
        None => eprintln!("forge-desktop: no speak dispatcher available; scripts cannot speak"),
    }
    if let Err(e) = script_registry_mut.load_all(backend.as_ref()).await {
        eprintln!("forge-desktop: script registry load failed at boot: {e}");
    }
    let script_registry = Arc::new(script_registry_mut);

    let cancel_registry = Arc::new(ActionCancelRegistry::new());
    let scheduler_cell = SchedulerCell::new();
    let overlay_service_cell = OverlayServiceCell::new();
    let mut sub_action_reg = SubActionRegistry::new();
    if let Err(e) = register_core_sub_actions(
        &mut sub_action_reg,
        Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
        Arc::clone(&backend) as Arc<dyn UserGlobalsRepo>,
        Arc::clone(&script_registry),
        Arc::clone(&bus) as Arc<dyn EventPublisher>,
        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
        scheduler_cell.clone(),
        backend.trigger_instance_repo(),
        backend.action_repo(),
        Arc::clone(&backend) as Arc<dyn ScriptRepo>,
        Arc::clone(&cancel_registry),
        overlay_service_cell.clone(),
        Config::default(),
    ) {
        eprintln!("forge-desktop: core sub-action registration failed: {e}");
    }

    let soundboard_settings_repo: Arc<dyn SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn SettingsRepo>;
    let soundboard_settings = SoundboardSettingsHandle::new(
        load_soundboard_settings(soundboard_settings_repo.as_ref()).await,
    );
    let soundboard_player = Arc::new(SoundboardPlayer::with_settings(
        Arc::new(CpalSinkFactory),
        Arc::new(BusAudioEventSink::new(Arc::clone(&bus))),
        backend.soundboard_clips_repo(),
        soundboard_settings,
    ));
    match speak_dispatcher {
        Some(dispatcher) => {
            if let Err(e) = register_audio_sub_actions(
                &mut sub_action_reg,
                Arc::clone(&soundboard_player) as Arc<dyn SoundPlayer>,
                dispatcher,
            ) {
                eprintln!("forge-desktop: audio sub-action runner registration failed: {e}");
            }
        }
        None => eprintln!(
            "forge-desktop: no speak dispatcher available; audio sub-action runners not registered"
        ),
    }

    let mut trigger_reg = TriggerRegistry::new();
    if let Err(e) = register_core_triggers(&mut trigger_reg) {
        eprintln!("forge-desktop: core trigger registration failed: {e}");
    }

    let integrations =
        build_integrations(&mut sub_action_reg, &mut trigger_reg, &backend, &bus).await;

    let sub_action_registry = Arc::new(sub_action_reg);
    let trigger_registry = Arc::new(trigger_reg);
    let trigger_instance_repo = backend.trigger_instance_repo();
    for descriptor in trigger_registry.all() {
        if let Err(e) = trigger_instance_repo
            .upsert_default(descriptor.id(), descriptor.label())
            .await
        {
            eprintln!(
                "forge-desktop: upsert_default failed for kind_id={}: {e}",
                descriptor.id()
            );
        }
    }

    let queues = match backend.queue_repo().list().await {
        Ok(queues) => queues,
        Err(e) => {
            eprintln!("forge-desktop: failed to load queues on boot, starting empty: {e}");
            Vec::new()
        }
    };

    let action_engine = spawn_action_engine(
        Arc::clone(&bus),
        backend.action_repo(),
        backend.history_repo(),
        Arc::clone(&sub_action_registry),
        cancel_registry,
    );
    let scheduler = QueueScheduler::spawn(action_engine.clone(), Arc::clone(&bus), queues);
    scheduler_cell.set(scheduler.clone());
    let trigger_evaluator = spawn_trigger_evaluator(
        Arc::clone(&bus),
        Arc::clone(&trigger_registry),
        backend.action_repo(),
        backend.trigger_instance_repo(),
        scheduler.clone(),
    );
    let live_viewers = spawn_live_viewer_aggregator();
    for source in integrations.viewer_sources {
        live_viewers.register(source);
    }

    let server = build_server(&backend, &bus, &action_engine).await;

    let mut overlay_kinds_mut = OverlayKindRegistry::new();
    if let Err(e) = register_builtin_kinds(&mut overlay_kinds_mut) {
        eprintln!("forge-desktop: overlay type registration failed: {e}");
    }
    let overlay_kinds = Arc::new(overlay_kinds_mut);
    let overlay_frames: Option<Arc<dyn OverlayFrameSink>> = server
        .clone()
        .map(|handle| Arc::new(ServerOverlayFrameSink::new(handle)) as Arc<dyn OverlayFrameSink>);
    let overlays = OverlayServiceHandle::new(
        backend.overlay_repo(),
        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
        Arc::clone(&overlay_kinds),
        Arc::clone(&bus),
        overlay_frames,
    );
    overlay_service_cell.set(overlays.clone());
    if let Some(handle) = server.clone() {
        handle
            .set_overlay_connect_listener(
                Arc::new(overlays.clone()) as Arc<dyn OverlayConnectListener>
            )
            .await;
    }
    match overlays.materialize_all().await {
        Ok(pass) => tracing::info!(
            materialized = pass.materialized,
            unavailable = pass.unavailable,
            failed = pass.failed,
            "overlay pages materialized"
        ),
        Err(e) => eprintln!("forge-desktop: overlay materialization pass failed: {e}"),
    }

    Ok(RuntimeHandles {
        rt_handle: tokio::runtime::Handle::current(),
        log_tail,
        backend,
        startup_language,
        bus,
        script_registry,
        sub_action_registry,
        trigger_registry,
        action_engine,
        scheduler,
        trigger_evaluator,
        live_viewers,
        builtins: integrations.builtins,
        kick_install_seed: integrations.kick_install_seed,
        youtube_install_seed: integrations.youtube_install_seed,
        obs_install_seed: integrations.obs_install_seed,
        vtube_install_seed: integrations.vtube_install_seed,
        discord_client: integrations.discord_client,
        midi_client: integrations.midi_client,
        hotkey_client: integrations.hotkey_client,
        server,
        overlays,
        overlay_kinds,
        speak,
        speak_events,
        pipeline_config,
        tts_registry,
        soundboard_player,
        voice_gate,
    })
}

async fn build_voice_gate(
    settings: &dyn SettingsRepo,
    speak: Option<forge_speak_queue::SpeakQueueHandle>,
) -> Arc<VoiceGateOwner> {
    let owner = Arc::new(VoiceGateOwner::new(
        tokio::runtime::Handle::current(),
        speak,
    ));
    match forge_storage::voice_gate_settings(settings).await {
        Ok(settings) if settings.enabled => owner.start(config_from_settings(&settings)),
        Ok(_) => {}
        Err(e) => {
            eprintln!("forge-desktop: failed to load voice gate settings; leaving it off: {e}")
        }
    }
    owner
}

async fn build_server(
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    action_engine: &ActionEngineHandle,
) -> Option<forge_server::ServerHandle> {
    let settings = match forge_server::ServerSettings::load(backend.as_ref()).await {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("forge-desktop: server settings load failed, leaving server off: {e}");
            return None;
        }
    };
    if !settings.enabled {
        return None;
    }
    let ip: std::net::IpAddr = match settings.bind_address.parse() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!(
                "forge-desktop: invalid server bind address '{}': {e}",
                settings.bind_address
            );
            return None;
        }
    };
    let bind_addr = std::net::SocketAddr::new(ip, settings.port);

    let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let credentials: Arc<dyn CredentialsRepo> = Arc::clone(backend) as Arc<dyn CredentialsRepo>;
    let globals: Arc<dyn GlobalsRepo> = Arc::clone(backend) as Arc<dyn GlobalsRepo>;
    let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(backend) as Arc<dyn UserGlobalsRepo>;
    let mut config = forge_server::ServerConfig::new(
        settings_repo,
        credentials,
        Arc::clone(bus),
        backend.action_repo(),
        globals,
        user_globals,
        backend.overlay_repo(),
        Arc::new(action_engine.clone()),
    );
    config.bind_addr = bind_addr;
    config.auth_required_for_reads = settings.auth_required_for_reads;
    config.lan_bind_enabled = settings.lan_bind_enabled;
    config.overlay_cors_any_origin = settings.overlay_cors_any_origin;
    config.additional_origins = settings.additional_origins.clone();
    if let Some(root) = settings.overlay_root.filter(|root| !root.is_empty()) {
        config.overlay_root = std::path::PathBuf::from(root);
    }

    match forge_server::start_server(config).await {
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("forge-desktop: server failed to start, leaving it off: {e}");
            None
        }
    }
}
