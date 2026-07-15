use std::path::PathBuf;
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_platform_core::paths;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Config, EventBus, QueueScheduler, SchedulerCell,
    ScriptRegistry, SoundPlayer, SpeakDispatcher, TtsTriggerSettingsHandle,
    register_audio_sub_actions, register_core_sub_actions, register_core_triggers,
    spawn_action_engine, spawn_live_viewer_aggregator, spawn_trigger_evaluator,
};
use forge_soundboard::{BusAudioEventSink, CpalSinkFactory, SoundboardPlayer};
use forge_storage::{
    CredentialsRepo, DataProvider, GlobalsRepo, SettingsRepo, StorageError, UserGlobalsRepo,
};
use forge_storage_sqlite::SqliteBackend;

use crate::integrations::build_integrations;
use crate::runtime_handles::RuntimeHandles;
use crate::speak_boot::build_speak_queue;
use crate::speak_bridge::SpeakBridge;

pub enum BootFailure {
    UpgradeRequired { expected: u32, found: u32 },
    Retry { reason: String },
}

fn default_db_path() -> PathBuf {
    paths::data_dir().join("forge.db")
}

/// Must run within the tokio runtime: the engine/scheduler/evaluator spawn tasks internally.
pub async fn build_runtime() -> Result<RuntimeHandles, BootFailure> {
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

    let bus = EventBus::new(backend.event_log_repo());
    EventBus::spawn_flush_task(Arc::clone(&bus));

    let (speak, speak_events, pipeline_config, tts_registry) =
        build_speak_queue(&bus, &backend).await;
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
        Arc::clone(&cancel_registry),
        Config::default(),
    ) {
        eprintln!("forge-desktop: core sub-action registration failed: {e}");
    }

    let tts_trigger_settings = {
        let repo = backend.tts_trigger_settings_repo();
        let loaded = repo.get_trigger_settings().await.unwrap_or_else(|e| {
            eprintln!(
                "forge-desktop: failed to load tts trigger settings on boot, using defaults: {e}"
            );
            forge_storage::TtsTriggerSettings::default()
        });
        TtsTriggerSettingsHandle::new(loaded)
    };
    match speak_dispatcher {
        Some(dispatcher) => {
            let sound_player = Arc::new(SoundboardPlayer::new(
                Arc::new(CpalSinkFactory),
                Arc::new(BusAudioEventSink::new(Arc::clone(&bus))),
                backend.soundboard_clips_repo(),
            ));
            if let Err(e) = register_audio_sub_actions(
                &mut sub_action_reg,
                sound_player as Arc<dyn SoundPlayer>,
                dispatcher,
                tts_trigger_settings.clone(),
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

    Ok(RuntimeHandles {
        rt_handle: tokio::runtime::Handle::current(),
        backend,
        bus,
        script_registry,
        sub_action_registry,
        trigger_registry,
        action_engine,
        scheduler,
        trigger_evaluator,
        live_viewers,
        builtins: integrations.builtins,
        server,
        speak,
        tts_trigger_settings,
        speak_events,
        pipeline_config,
        tts_registry,
    })
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
        Arc::new(action_engine.clone()),
    );
    config.bind_addr = bind_addr;
    config.auth_required_for_reads = settings.auth_required_for_reads;
    config.lan_bind_enabled = settings.lan_bind_enabled;
    config.http_overlay_require_token = settings.http_overlay_require_token;
    config.overlay_cors_any_origin = settings.overlay_cors_any_origin;
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
