use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::AudioSink;
use forge_components::{Density, ThemeId};
use forge_events::EventPublisher;
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_platform_core::{PlatformEndpoints, paths};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Catalog, CatchUpSettle, Config, DonationIngest,
    DonationOverlayAudience, EventBus, FirstChatLedger, LatestValues, OverlayConnectFanout,
    OverlayConnectListener, OverlayFrameSink, OverlayMediaLibrary, OverlayServiceCell,
    OverlayServiceHandle, QueueScheduler, ScheduledRunsCell, ScheduledRunsParts, SchedulerCell,
    ScriptRegistry, ScriptScheduling, SoundPlayer, SpeakDispatcher, SystemWallClock,
    TwitchEmoteLexicon, register_audio_sub_actions, register_core_sub_actions,
    register_core_triggers, register_donation_sub_actions, register_latest_sub_actions,
    register_scheduled_run_sub_actions, spawn_action_engine, spawn_chat_history_persistence,
    spawn_event_log_bridge, spawn_latest_overlay_feed, spawn_latest_projector,
    spawn_live_viewer_aggregator, spawn_scheduled_runs, spawn_stream_live_signal,
    spawn_timer_scheduler, spawn_trigger_evaluator, spawn_twitch_emote_learning,
    spawn_viewer_tracker,
};
use forge_soundboard::{
    BusAudioEventSink, ClipLibrary, CpalSinkFactory, SoundboardPlayer, SoundboardSettingsHandle,
    load_soundboard_settings,
};
use forge_storage::{
    CredentialsRepo, DataProvider, GlobalsRepo, ScriptRepo, SettingsRepo, StorageError,
    UserGlobalsRepo, reserved_keys,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{LatestValueReader, Shared};

use crate::audio_router::{AudioRouter, AudioRouterParts};
use crate::clip_hotkeys::{HotkeySyncedClipsRepo, spawn_clip_hotkey_dispatcher};
use crate::integration_supervisor::IntegrationSupervisor;
use crate::integrations::build_integrations;
use crate::log_tail::LogTail;
use crate::overlay_frame_sink::ServerOverlayFrameSink;
use crate::routed_sink::RoutedSink;
use crate::runtime_handles::RuntimeHandles;
use crate::speak_boot::{build_speak_queue, build_speech_output};
use crate::speak_bridge::SpeakBridge;
use crate::voice_gate::{VoiceGateOwner, config_from_settings};

pub enum BootFailure {
    UpgradeRequired { expected: u32, found: u32 },
    PreBaseline { found: u32 },
    Retry { reason: String },
}

fn default_db_path() -> PathBuf {
    paths::data_dir().join("forge.db")
}

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

async fn apply_persisted_log_level(repo: &dyn SettingsRepo) {
    if crate::log_level::env_overridden() {
        return;
    }
    match forge_storage::diagnostic_log_level(repo).await {
        Ok(level) => {
            if crate::log_level::apply(&level) {
                tracing::info!(?level, "log level applied from settings");
            } else {
                tracing::warn!("log filter is not reloadable; keeping the boot level");
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not read the persisted log level"),
    }
}

pub async fn build_runtime(
    log_tail: LogTail,
    endpoints: PlatformEndpoints,
    hotkey_main_thread: forge_hotkey::MainThreadLink,
) -> Result<RuntimeHandles, BootFailure> {
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
                StorageError::PreBaselineSchema { found } => BootFailure::PreBaseline { found },
                other => BootFailure::Retry {
                    reason: other.to_string(),
                },
            });
        }
    };

    let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(&backend) as Arc<dyn SettingsRepo>;
    let credentials_key_loss =
        match forge_storage::take_credentials_key_loss(settings_repo.as_ref()).await {
            Ok(loss) => loss,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the credentials key loss record");
                None
            }
        };
    apply_persisted_log_level(settings_repo.as_ref()).await;
    let stay_awake = crate::stay_awake::start_stay_awake(Arc::clone(&settings_repo)).await;

    let (startup_language, persist) =
        crate::i18n::resolve_startup_language(Arc::clone(&settings_repo)).await;
    if let Some(detected) = persist
        && let Err(e) = settings_repo.set_language(detected).await
    {
        eprintln!("forge-desktop: failed to persist detected locale: {e}");
    }

    let bus = EventBus::new(backend.event_log_repo());
    EventBus::spawn_flush_task(Arc::clone(&bus));
    spawn_event_log_bridge(Arc::clone(&bus));
    spawn_chat_history_persistence(
        Arc::clone(&bus),
        backend.chat_history_repo(),
        Arc::clone(&settings_repo),
    );
    let viewer_repo = backend.viewer_repo();
    match FirstChatLedger::load(viewer_repo.as_ref()).await {
        Ok(ledger) => bus.track_first_chats(ledger),
        Err(e) => {
            tracing::warn!(error = %e, "viewer history unreadable; first-time chatters are not detected")
        }
    }
    spawn_viewer_tracker(Arc::clone(&bus), viewer_repo);

    let anonymous_donor = Shared::new(crate::i18n::message_in(
        startup_language,
        crate::i18n::ANONYMOUS_DONOR_KEY,
    ));
    let latest_values = LatestValues::load(
        backend.latest_value_repo(),
        Arc::clone(&bus) as Arc<dyn EventPublisher>,
        anonymous_donor.clone(),
    )
    .await;
    spawn_latest_projector(&bus, latest_values.clone());

    let speech_output = build_speech_output(&backend).await;
    let speech_sink = Arc::new(RoutedSink::new(
        Arc::clone(&speech_output) as Arc<dyn AudioSink>
    ));
    let (speak, speak_events, pipeline_config, tts_registry) = build_speak_queue(
        &bus,
        &backend,
        Arc::clone(&speech_sink) as Arc<dyn AudioSink>,
    )
    .await;
    let voice_gate = build_voice_gate(settings_repo.as_ref(), speak.clone()).await;
    let speak_bridge = speak
        .clone()
        .map(|handle| Arc::new(SpeakBridge::new(Arc::new(handle))));
    let speak_dispatcher: Option<Arc<dyn SpeakDispatcher>> = speak_bridge
        .clone()
        .map(|bridge| bridge as Arc<dyn SpeakDispatcher>);
    let show_speaker = speak_dispatcher.clone();
    let speak_requester: Option<Arc<dyn forge_script::SpeakRequester>> =
        speak_bridge.map(|bridge| bridge as Arc<dyn forge_script::SpeakRequester>);

    let scheduled_runs_cell = ScheduledRunsCell::new();
    let mut script_registry_mut = ScriptRegistry::new();
    script_registry_mut
        .set_latest_values(Arc::new(latest_values.clone()) as Arc<dyn LatestValueReader>);
    script_registry_mut.set_scheduling(ScriptScheduling::new(
        scheduled_runs_cell.clone(),
        backend.action_repo(),
    ));
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

    let donations = Arc::new(
        DonationIngest::new(
            backend.donation_repo(),
            Arc::clone(&bus) as Arc<dyn EventPublisher>,
            anonymous_donor.clone(),
        )
        .holding_catch_up(),
    );
    if let Err(e) =
        register_scheduled_run_sub_actions(&mut sub_action_reg, scheduled_runs_cell.clone())
    {
        eprintln!("forge-desktop: scheduled run sub-action registration failed: {e}");
    }
    if let Err(e) = register_donation_sub_actions(&mut sub_action_reg, Arc::clone(&donations)) {
        eprintln!("forge-desktop: donation sub-action registration failed: {e}");
    }
    if let Err(e) = register_latest_sub_actions(
        &mut sub_action_reg,
        Arc::new(latest_values.clone()) as Arc<dyn LatestValueReader>,
    ) {
        eprintln!("forge-desktop: latest value sub-action registration failed: {e}");
    }

    let mut trigger_reg = TriggerRegistry::new();
    if let Err(e) = register_core_triggers(&mut trigger_reg) {
        eprintln!("forge-desktop: core trigger registration failed: {e}");
    }

    let integrations = build_integrations(
        &mut sub_action_reg,
        &mut trigger_reg,
        &backend,
        &bus,
        &endpoints,
        &donations,
        hotkey_main_thread,
    )
    .await;

    let soundboard_settings_repo: Arc<dyn SettingsRepo> =
        Arc::clone(&backend) as Arc<dyn SettingsRepo>;
    let soundboard_settings = SoundboardSettingsHandle::new(
        load_soundboard_settings(soundboard_settings_repo.as_ref()).await,
    );
    let soundboard_library = Arc::new(ClipLibrary::new(
        HotkeySyncedClipsRepo::wrap(
            backend.soundboard_clips_repo(),
            integrations.hotkey_reconciler.clone(),
        ),
        backend.media_repo(),
    ));
    soundboard_library.install_event_publisher(Arc::clone(&bus) as Arc<dyn EventPublisher>);
    let soundboard_player = Arc::new(SoundboardPlayer::with_settings(
        Arc::new(CpalSinkFactory),
        Arc::new(BusAudioEventSink::new(Arc::clone(&bus))),
        soundboard_library,
        soundboard_settings,
    ));
    soundboard_player.install_settings_store(Arc::clone(&soundboard_settings_repo));
    match speak_dispatcher {
        Some(dispatcher) => {
            let reward_emotes = TwitchEmoteLexicon::default();
            spawn_twitch_emote_learning(&bus, reward_emotes.clone());
            if let Err(e) = register_audio_sub_actions(
                &mut sub_action_reg,
                Arc::clone(&soundboard_player) as Arc<dyn SoundPlayer>,
                dispatcher,
                reward_emotes,
            ) {
                eprintln!("forge-desktop: audio sub-action runner registration failed: {e}");
            }
        }
        None => eprintln!(
            "forge-desktop: no speak dispatcher available; audio sub-action runners not registered"
        ),
    }

    if let Some(reconciler) = &integrations.hotkey_reconciler {
        spawn_clip_hotkey_dispatcher(
            bus.subscribe(),
            reconciler.clip_bindings(),
            Arc::clone(&soundboard_player),
        );
    }

    let sub_action_registry = Arc::new(sub_action_reg);
    let trigger_registry = Arc::new(trigger_reg);
    bus.declare_lanes(&trigger_registry);
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

    let catalog = Catalog::from_provider(backend.as_ref());
    let action_engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&catalog),
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
        Arc::clone(&catalog),
        scheduler.clone(),
        Config::default(),
    );
    let live_viewers = spawn_live_viewer_aggregator();
    let supervisor = IntegrationSupervisor::launch(
        integrations.factories,
        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
        action_engine.integration_gate().clone(),
        integrations.builtins.clone(),
        live_viewers.clone(),
    );
    donations.recover_unannounced().await;
    supervisor.boot().await;
    let first_run = crate::first_run::resolve_first_run(
        backend.as_ref() as &dyn SettingsRepo,
        &supervisor.watch().current(),
    )
    .await;
    if let (Some(reconciler), Some(engine)) = (
        &integrations.hotkey_reconciler,
        supervisor.slot(&forge_hotkey::HOTKEY_INTEGRATION.id),
    ) {
        reconciler.enable_engine_on_new_bindings(engine);
    }
    let stream_live =
        spawn_stream_live_signal(&live_viewers, integrations.obs_install_seed.stream_output());
    let bot_accounts = load_bot_accounts(&backend).await;
    let timer_scheduler = spawn_timer_scheduler(
        Arc::clone(&bus),
        Arc::clone(&catalog),
        stream_live.clone(),
        bot_accounts.clone(),
    );
    let scheduled_runs = spawn_scheduled_runs(ScheduledRunsParts {
        repo: backend.scheduled_run_repo(),
        revision: backend.scheduled_run_revision(),
        catalog: Arc::clone(&catalog),
        actions: backend.action_repo(),
        queues: scheduler.clone(),
        bus: Arc::clone(&bus),
        clock: Arc::new(SystemWallClock),
        catch_up: CatchUpSettle::when(supervisor.watch().until_all_settled()),
    });
    scheduled_runs_cell.set(scheduled_runs.clone());

    let ServerBoot {
        handle: server,
        unavailable_reason: server_unavailable,
    } = build_server(&backend, &bus, &action_engine).await;

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
    )
    .with_media_library(OverlayMediaLibrary::new(
        backend.media_repo(),
        backend.soundboard_clips_repo(),
    ))
    .with_latest_values(Arc::new(latest_values.clone()) as Arc<dyn LatestValueReader>)
    .with_event_wiring(
        Arc::new(forge_runtime::actions::ActionsService::new(
            backend.action_repo(),
            backend.queue_repo(),
            backend.history_repo(),
            backend.trigger_instance_repo(),
            backend.soundboard_clips_repo(),
        )),
        Arc::clone(&sub_action_registry),
        Arc::clone(&trigger_registry),
    );
    let overlays = match show_speaker {
        Some(speaker) => overlays.with_speech(speaker),
        None => overlays,
    };
    overlay_service_cell.set(overlays.clone());
    spawn_latest_overlay_feed(&bus, overlays.clone());
    let donation_audience = DonationOverlayAudience::new(
        Arc::clone(&catalog),
        overlays.clone(),
        Arc::clone(&sub_action_registry),
    );
    let mut connect_listeners = vec![Arc::new(overlays.clone()) as Arc<dyn OverlayConnectListener>];
    if let Some(waker) = donations.catch_up_waker() {
        donation_audience.forward_changes_to(&waker);
        connect_listeners.push(Arc::new(waker) as Arc<dyn OverlayConnectListener>);
    }
    if let Some(handle) = server.clone() {
        handle
            .set_overlay_connect_listener(Arc::new(OverlayConnectFanout::new(connect_listeners)))
            .await;
    }
    donations.spawn_catch_up_release(Arc::new(donation_audience));
    match overlays.materialize_all().await {
        Ok(pass) => tracing::info!(
            materialized = pass.materialized,
            unavailable = pass.unavailable,
            failed = pass.failed,
            "overlay pages materialized"
        ),
        Err(e) => eprintln!("forge-desktop: overlay materialization pass failed: {e}"),
    }

    let audio_router = Arc::new(AudioRouter::new(AudioRouterParts {
        backend: Arc::clone(&backend),
        server: server.clone(),
        overlays: overlays.clone(),
        speech_sink: Arc::clone(&speech_sink),
        speech_output: Arc::clone(&speech_output) as Arc<dyn AudioSink>,
        soundboard_player: Arc::clone(&soundboard_player),
        speak: speak.clone(),
    }));
    audio_router.apply().await;

    Ok(RuntimeHandles {
        rt_handle: tokio::runtime::Handle::current(),
        log_tail,
        backend,
        startup_language,
        anonymous_donor,
        latest_values,
        credentials_key_loss,
        bus,
        script_registry,
        sub_action_registry,
        trigger_registry,
        action_engine,
        scheduler,
        trigger_evaluator,
        scheduled_runs,
        timer_scheduler,
        live_viewers,
        stream_live,
        builtins: integrations.builtins,
        integrations: supervisor,
        obs_install_seed: integrations.obs_install_seed,
        vtube_install_seed: integrations.vtube_install_seed,
        discord_client: integrations.discord_client,
        midi_sink: integrations.midi_sink,
        hotkey_client: integrations.hotkey_client,
        hotkey_reconciler: integrations.hotkey_reconciler,
        donation_services: integrations.donation_services,
        server,
        overlays,
        overlay_kinds,
        speak,
        speak_events,
        pipeline_config,
        bot_accounts,
        tts_registry,
        speech_output,
        speech_sink: speech_sink as Arc<dyn AudioSink>,
        server_unavailable,
        audio_router,
        soundboard_player,
        voice_gate,
        stay_awake,
        first_run,
        endpoints,
    })
}

async fn load_bot_accounts(backend: &Arc<dyn DataProvider>) -> forge_types::Shared<Vec<String>> {
    match backend.tts_filters_repo().get_pipeline_settings().await {
        Ok(settings) => forge_types::Shared::new(settings.bot_accounts),
        Err(e) => {
            eprintln!("forge-desktop: failed to load configured bot accounts on boot: {e}");
            forge_types::Shared::default()
        }
    }
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

struct ServerBoot {
    handle: Option<forge_server::ServerHandle>,
    unavailable_reason: Option<String>,
}

impl ServerBoot {
    fn running(handle: forge_server::ServerHandle) -> Self {
        Self {
            handle: Some(handle),
            unavailable_reason: None,
        }
    }

    fn unavailable(reason: String) -> Self {
        Self {
            handle: None,
            unavailable_reason: Some(reason),
        }
    }
}

async fn build_server(
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    action_engine: &ActionEngineHandle,
) -> ServerBoot {
    let settings = match forge_server::ServerSettings::load(backend.as_ref()).await {
        Ok(settings) => settings,
        Err(e) => {
            tracing::error!(error = %e, "server settings load failed, leaving server off");
            return ServerBoot::unavailable(e.to_string());
        }
    };
    let bind_addr = match settings.bind_address.parse::<std::net::IpAddr>() {
        Ok(ip) => Some(std::net::SocketAddr::new(ip, settings.port)),
        Err(e) => {
            tracing::warn!(
                bind_address = %settings.bind_address,
                error = %e,
                "invalid server bind address"
            );
            None
        }
    };

    let build_config = || {
        let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(backend) as Arc<dyn SettingsRepo>;
        let credentials: Arc<dyn CredentialsRepo> = Arc::clone(backend) as Arc<dyn CredentialsRepo>;
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(backend) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> =
            Arc::clone(backend) as Arc<dyn UserGlobalsRepo>;
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
        if let Some(bind_addr) = bind_addr {
            config.bind_addr = bind_addr;
        }
        config.auth_required_for_reads = settings.auth_required_for_reads;
        config.lan_bind_enabled = settings.lan_bind_enabled;
        config.overlay_cors_any_origin = settings.overlay_cors_any_origin;
        config.additional_origins = settings.additional_origins.clone();
        if let Some(root) = settings
            .overlay_root
            .as_ref()
            .filter(|root| !root.is_empty())
        {
            config.overlay_root = std::path::PathBuf::from(root);
        }
        config
    };

    if settings.enabled && bind_addr.is_some() {
        match forge_server::start_server(build_config()).await {
            Ok(handle) => return ServerBoot::running(handle),
            Err(e) => tracing::error!(error = %e, "server failed to start, leaving it off"),
        }
    }

    match forge_server::stopped_server(build_config()).await {
        Ok(handle) => ServerBoot::running(handle),
        Err(e) => {
            tracing::error!(error = %e, "server state unavailable, leaving it off");
            ServerBoot::unavailable(e.to_string())
        }
    }
}
