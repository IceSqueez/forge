use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::{AudioSink, AudioSinkFactory};
use forge_components::{AccentColor, Density, ThemeId};
use forge_events::{EventObserver, EventPublisher};
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_platform_core::PlatformEndpoints;
use forge_runtime::{
    ActionEngineHandle, CatchUpSettle, DateNamesHandle, DonationIngest, DonationOverlayAudience,
    EventBus, EventHistory, FirstChatLedger, FrozenRuntime, LatestValues, NotificationPreferences,
    OverlayConnectFanout, OverlayConnectListener, OverlayFrameSink, OverlaySampleSource,
    OverlaySamples, RuntimeWiring, RuntimeWiringParts, SpeakDispatcher, SystemWallClock,
    TwitchEmoteLexicon, register_core_triggers, register_donation_sub_actions,
    register_latest_sub_actions, register_speech_sub_actions, spawn_chat_history_persistence,
    spawn_event_log_bridge, spawn_latest_overlay_feed, spawn_latest_projector,
    spawn_live_viewer_aggregator, spawn_notification_policy, spawn_stream_live_signal,
    spawn_timer_scheduler, spawn_twitch_emote_learning, spawn_viewer_tracker,
};
use forge_server::DeliveryHub;
use forge_soundboard::{
    BusAudioEventSink, ClipLibrary, SoundboardControl, SoundboardPlayer, SoundboardSettingsHandle,
    load_soundboard_settings, register_soundboard_sub_actions,
};
use forge_storage::settings::core_settings::{ACCENT, FONT_BODY, FONT_MONO, LANGUAGE, THEME};
use forge_storage::{
    CredentialsRepo, DataProvider, GlobalsRepo, SettingsRepo, StorageError, UserGlobalsRepo,
    read_setting, read_setting_or_default, write_setting,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{LatestValueReader, PlatformId, Shared};

use crate::audio_router::{AudioRouter, AudioRouterParts};
use crate::boot_warnings::{BootWarning, BootWarnings};
use crate::integrations::{IntegrationInputs, build_integrations};
use crate::log_tail::LogTail;
use crate::overlay_build_failure::OverlayBuildFailure;
use crate::platform_identities::PlatformIdentities;
use crate::routed_sink::RoutedSink;
use crate::runtime_handles::RuntimeHandles;
use crate::settings_catalog::DENSITY;
use crate::speak_boot::{LocalTtsEngine, build_speak_queue, build_speech_output};
use crate::speak_bridge::SpeakBridge;
use crate::voice_gate::{VoiceGateOwner, config_from_settings};
use crate::window_placement::{WINDOW_PLACEMENT, WindowPlacement};
use forge_hotkey::HotkeySyncedClipsRepo;
use forge_runtime::IntegrationSupervisor;
use forge_soundboard::spawn_clip_hotkey_dispatcher;

pub enum BootFailure {
    UpgradeRequired { expected: u32, found: u32 },
    PreBaseline { found: u32 },
    Retry { reason: String },
}

fn default_db_path() -> PathBuf {
    forge_paths::data_dir().join(forge_storage::DATABASE_FILE_NAME)
}

const APPEARANCE_READ_BUDGET: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct PersistedAppearance {
    pub theme: ThemeId,
    pub accent: AccentColor,
    pub density: Density,
    pub body_font: Option<String>,
    pub mono_font: Option<String>,
}

pub struct BootStorage {
    pub appearance: PersistedAppearance,
    pub window_placement: Option<WindowPlacement>,
    pub backend: Option<Arc<dyn DataProvider>>,
    pub opening: BackendOpening,
}

pub struct BackendOpening(tokio::task::JoinHandle<Result<Arc<dyn DataProvider>, BootFailure>>);

impl BackendOpening {
    pub async fn finished(self) -> Result<Arc<dyn DataProvider>, BootFailure> {
        match self.0.await {
            Ok(opened) => opened,
            Err(e) => Err(BootFailure::Retry {
                reason: e.to_string(),
            }),
        }
    }
}

pub fn open_boot_storage(rt_handle: &tokio::runtime::Handle) -> BootStorage {
    open_boot_storage_with(rt_handle, open_backend())
}

fn open_boot_storage_with(
    rt_handle: &tokio::runtime::Handle,
    open: impl Future<Output = Result<Arc<dyn DataProvider>, BootFailure>> + Send + 'static,
) -> BootStorage {
    let (tx, rx) = std::sync::mpsc::channel();
    let opening = BackendOpening(rt_handle.spawn(async move {
        let opened = open.await;
        let presentation = match &opened {
            Ok(backend) => {
                let appearance = load_appearance(backend.as_ref()).await;
                let window_placement =
                    read_setting_or_default(backend.as_ref(), &WINDOW_PLACEMENT).await;
                (appearance, window_placement, Some(Arc::clone(backend)))
            }
            Err(_) => (PersistedAppearance::default(), None, None),
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the boot thread stops waiting once its read budget runs out"
        )]
        let _ = tx.send(presentation);
        opened
    }));
    let (appearance, window_placement, backend) =
        rx.recv_timeout(APPEARANCE_READ_BUDGET).unwrap_or_default();
    tracing::info!(
        theme = ?appearance.theme,
        accent = ?appearance.accent,
        density = ?appearance.density,
        "applied persisted presentation"
    );
    BootStorage {
        appearance,
        window_placement,
        backend,
        opening,
    }
}

async fn load_appearance(settings: &dyn SettingsRepo) -> PersistedAppearance {
    let theme = match read_setting(settings, &THEME).await {
        Ok(Some(key)) => ThemeId::from_storage_key(&key).unwrap_or_default(),
        _ => ThemeId::default(),
    };
    let accent = match read_setting(settings, &ACCENT).await {
        Ok(Some(key)) => AccentColor::from_storage_key(&key).unwrap_or_default(),
        _ => AccentColor::default(),
    };
    let density = match read_setting(settings, &DENSITY).await {
        Ok(Some(key)) => Density::from_storage_key(&key).unwrap_or_default(),
        _ => Density::default(),
    };
    PersistedAppearance {
        theme,
        accent,
        density,
        body_font: read_setting(settings, &FONT_BODY).await.ok().flatten(),
        mono_font: read_setting(settings, &FONT_MONO).await.ok().flatten(),
    }
}

async fn open_backend() -> Result<Arc<dyn DataProvider>, BootFailure> {
    let db_path = default_db_path();
    if let Some(parent) = db_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return Err(BootFailure::Retry {
            reason: format!("failed to create data directory {}: {e}", parent.display()),
        });
    }
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    match SqliteBackend::open(&url).await {
        Ok(backend) => Ok(Arc::new(backend) as Arc<dyn DataProvider>),
        Err(e) => {
            let err: StorageError = e.into();
            Err(match err {
                StorageError::SchemaMismatch { expected, found } => {
                    BootFailure::UpgradeRequired { expected, found }
                }
                StorageError::PreBaselineSchema { found } => BootFailure::PreBaseline { found },
                other => BootFailure::Retry {
                    reason: other.to_string(),
                },
            })
        }
    }
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

pub struct RuntimeInputs {
    pub log_tail: LogTail,
    pub endpoints: PlatformEndpoints,
    pub hotkey_main_thread: forge_hotkey::MainThreadLink,
    pub preopened: Option<Arc<dyn DataProvider>>,
    pub audio_sinks: Arc<dyn AudioSinkFactory>,
    pub local_tts_engines: Vec<LocalTtsEngine>,
    pub platform_identities: PlatformIdentities,
}

pub async fn build_runtime(inputs: RuntimeInputs) -> Result<RuntimeHandles, BootFailure> {
    let RuntimeInputs {
        log_tail,
        endpoints,
        hotkey_main_thread,
        preopened,
        audio_sinks,
        local_tts_engines,
        platform_identities,
    } = inputs;
    let backend = match preopened {
        Some(backend) => backend,
        None => open_backend().await?,
    };

    let mut boot_warnings = BootWarnings::default();
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
    let settings_catalog = Arc::new(crate::settings_catalog::registered_settings_catalog());
    let stay_awake = crate::stay_awake::start_stay_awake(Arc::clone(&settings_repo)).await;

    let (startup_language, persist) =
        crate::i18n::resolve_startup_language(Arc::clone(&settings_repo)).await;
    if let Some(detected) = persist
        && let Err(e) =
            write_setting(settings_repo.as_ref(), &LANGUAGE, &detected.to_string()).await
    {
        tracing::warn!(error = %e, "failed to persist detected locale");
    }
    let startup_display_formats =
        crate::i18n::resolve_startup_display_formats(settings_repo.as_ref(), startup_language)
            .await;

    let viewer_repo = backend.viewer_repo();
    let first_chats = match FirstChatLedger::load(viewer_repo.as_ref()).await {
        Ok(ledger) => Some(ledger),
        Err(e) => {
            tracing::warn!(error = %e, "viewer history unreadable; first-time chatters are not detected");
            None
        }
    };
    let bus = EventBus::with_first_chats(backend.event_log_repo(), first_chats);
    EventBus::spawn_flush_task(Arc::clone(&bus));
    spawn_event_log_bridge(Arc::clone(&bus));
    if let Err(e) = forge_runtime::sub_action_runners::SystemNotifyPort::adopt_app_identity() {
        tracing::warn!(error = %e, "desktop notifications keep the default app identity");
    }
    let notification_policy = spawn_notification_policy(
        &bus,
        NotificationPreferences::load(settings_repo.as_ref()).await,
    );
    let transfer_exclusivity = forge_transfer::TransferExclusivity::new();
    crate::safety_copy_prune::spawn_boot_prune(
        Arc::clone(&backend),
        forge_paths::data_dir(),
        transfer_exclusivity.clone(),
    );
    let scheduled_backups =
        forge_transfer::spawn_scheduled_backups(forge_transfer::ScheduledBackupParts {
            provider: Arc::clone(&backend),
            data_dir: forge_paths::data_dir(),
            exclusivity: transfer_exclusivity.clone(),
            clock: Arc::new(forge_transfer::SystemBackupClock),
        });
    let chat_history_retention = spawn_chat_history_persistence(
        Arc::clone(&bus),
        backend.chat_history_repo(),
        Arc::clone(&settings_repo),
    );
    spawn_viewer_tracker(Arc::clone(&bus), viewer_repo);

    let date_names = DateNamesHandle;
    date_names.set_language(crate::i18n::date_names_language(startup_language));
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

    let speech_output = build_speech_output(&backend, Arc::clone(&audio_sinks)).await;
    let speech_sink = Arc::new(RoutedSink::new(
        Arc::clone(&speech_output) as Arc<dyn AudioSink>
    ));
    let cloud_tts_gateways = forge_tts_cloud::CloudGateways::new();
    let (speak, speak_events, pipeline_config, tts_registry) = build_speak_queue(
        &bus,
        &backend,
        Arc::clone(&speech_sink) as Arc<dyn AudioSink>,
        &cloud_tts_gateways,
        local_tts_engines,
        &mut boot_warnings,
    )
    .await;
    let voice_gate =
        build_voice_gate(settings_repo.as_ref(), speak.clone(), &mut boot_warnings).await;
    let speak_bridge = speak
        .clone()
        .map(|handle| Arc::new(SpeakBridge::new(Arc::new(handle))));
    let speak_dispatcher: Option<Arc<dyn SpeakDispatcher>> = speak_bridge
        .clone()
        .map(|bridge| bridge as Arc<dyn SpeakDispatcher>);
    let show_speaker = speak_dispatcher.clone();
    let speak_requester: Option<Arc<dyn forge_script::SpeakRequester>> =
        speak_bridge.map(|bridge| bridge as Arc<dyn forge_script::SpeakRequester>);

    let mut overlay_kinds_mut = OverlayKindRegistry::new();
    if let Err(e) = register_builtin_kinds(&mut overlay_kinds_mut) {
        tracing::error!(error = %e, "overlay type registration failed");
        boot_warnings.record(BootWarning::OverlayTypes);
    }
    let overlay_kinds = Arc::new(overlay_kinds_mut);
    let delivery_hub = DeliveryHub::new(Arc::clone(&bus) as Arc<dyn EventObserver>);
    let mut wiring = RuntimeWiring::new(RuntimeWiringParts {
        bus: Arc::clone(&bus),
        provider: Arc::clone(&backend),
        latest_values: latest_values.clone(),
        script_speech: speak_requester,
        overlay_speech: show_speaker,
        overlay_kinds: Arc::clone(&overlay_kinds),
        overlay_frames: Arc::new(delivery_hub.clone()) as Arc<dyn OverlayFrameSink>,
        clock: Arc::new(SystemWallClock),
    });
    if let Err(e) = wiring.scripts().load_all(backend.as_ref()).await {
        tracing::warn!(error = %e, "script registry load failed at boot");
        boot_warnings.record(BootWarning::Scripts);
    }
    if let Err(e) = wiring.register_core_sub_actions() {
        tracing::error!(error = %e, "core sub-action registration failed");
        boot_warnings.record(BootWarning::CoreSubActions);
    }

    let donations = Arc::new(
        DonationIngest::new(
            backend.donation_repo(),
            Arc::clone(&bus) as Arc<dyn EventPublisher>,
            anonymous_donor.clone(),
        )
        .holding_catch_up(),
    );
    if let Err(e) = wiring.register_scheduled_run_sub_actions() {
        tracing::error!(error = %e, "scheduled run sub-action registration failed");
        boot_warnings.record(BootWarning::ScheduledRunSubActions);
    }
    if let Err(e) = register_donation_sub_actions(wiring.sub_actions_mut(), Arc::clone(&donations))
    {
        tracing::error!(error = %e, "donation sub-action registration failed");
        boot_warnings.record(BootWarning::DonationSubActions);
    }
    if let Err(e) = register_latest_sub_actions(
        wiring.sub_actions_mut(),
        Arc::new(latest_values.clone()) as Arc<dyn LatestValueReader>,
    ) {
        tracing::error!(error = %e, "latest value sub-action registration failed");
        boot_warnings.record(BootWarning::LatestValueSubActions);
    }

    if let Err(e) = register_core_triggers(wiring.triggers_mut()) {
        tracing::error!(error = %e, "core trigger registration failed");
        boot_warnings.record(BootWarning::CoreTriggers);
    }

    let (sub_action_reg, trigger_reg) = wiring.registries_mut();
    let integrations = build_integrations(
        sub_action_reg,
        trigger_reg,
        &backend,
        &bus,
        &donations,
        IntegrationInputs {
            endpoints,
            hotkey_main_thread,
            platform_identities,
        },
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
        audio_sinks,
        Arc::new(BusAudioEventSink::new(
            Arc::clone(&bus) as Arc<dyn EventPublisher>
        )),
        soundboard_library,
        soundboard_settings,
    ));
    soundboard_player.install_settings_store(Arc::clone(&soundboard_settings_repo));
    if let Err(e) = register_soundboard_sub_actions(
        wiring.sub_actions_mut(),
        Arc::clone(&soundboard_player) as Arc<dyn SoundboardControl>,
    ) {
        tracing::error!(error = %e, "soundboard sub-action runner registration failed");
        boot_warnings.record(BootWarning::SoundboardSubActions);
    }
    match speak_dispatcher {
        Some(dispatcher) => {
            let reward_emotes = TwitchEmoteLexicon::default();
            spawn_twitch_emote_learning(&bus, reward_emotes.clone());
            if let Err(e) =
                register_speech_sub_actions(wiring.sub_actions_mut(), dispatcher, reward_emotes)
            {
                tracing::error!(error = %e, "speech sub-action runner registration failed");
                boot_warnings.record(BootWarning::SpeechSubActions);
            }
        }
        None => {
            tracing::warn!(
                "no speak dispatcher available; speech sub-action runners not registered"
            );
            boot_warnings.record(BootWarning::SpeechSubActions);
        }
    }

    if let Some(reconciler) = &integrations.hotkey_reconciler {
        spawn_clip_hotkey_dispatcher(
            bus.as_ref(),
            reconciler.clip_bindings(),
            Arc::clone(&soundboard_player),
        );
    }

    let FrozenRuntime {
        catalog,
        scripts: script_registry,
        overlays,
        sub_action_registry,
        trigger_registry,
        action_engine,
        scheduler,
        trigger_evaluator,
        scheduled_runs,
        scheduled_runs_worker,
        trigger_defaults_stored,
        queues_loaded,
    } = wiring.freeze().await;
    if !trigger_defaults_stored {
        boot_warnings.record(BootWarning::TriggerDefaults);
    }
    if !queues_loaded {
        boot_warnings.record(BootWarning::Queues);
    }
    let live_viewers = spawn_live_viewer_aggregator();
    let supervisor = IntegrationSupervisor::launch(
        integrations.factories,
        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
        action_engine.integration_gate().clone(),
        Arc::new(integrations.builtins.clone()),
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
        reconciler.enable_engine_on_new_bindings(move || engine.request_enable());
    }
    let stream_live = spawn_stream_live_signal(
        &live_viewers,
        integrations
            .obs_install_seed
            .stream_output()
            .into_receiver(),
    );
    let bot_accounts = load_bot_accounts(&backend, &mut boot_warnings).await;
    let timer_scheduler = spawn_timer_scheduler(
        Arc::clone(&bus),
        Arc::clone(&catalog),
        stream_live.clone(),
        bot_accounts.clone(),
    );
    scheduled_runs_worker.start(CatchUpSettle::when(supervisor.watch().until_all_settled()));

    let ServerBoot {
        handle: server,
        unavailable_reason: server_unavailable,
    } = build_server(
        &backend,
        &bus,
        &action_engine,
        trigger_registry.platforms(),
        &delivery_hub,
    )
    .await;

    let overlay_actions = Arc::new(forge_runtime::actions::ActionsService::new(
        backend.action_repo(),
        backend.queue_repo(),
        backend.history_repo(),
        backend.trigger_instance_repo(),
    ));
    let overlay_samples: Arc<dyn OverlaySampleSource> = Arc::new(OverlaySamples::new(
        Arc::clone(&overlay_actions),
        Arc::clone(&sub_action_registry),
        Arc::clone(&trigger_registry),
    ));
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
    delivery_hub
        .set_overlay_connect_listener(Arc::new(OverlayConnectFanout::new(connect_listeners)));
    donations.spawn_catch_up_release(Arc::new(donation_audience));
    match forge_transfer::sweep_abandoned_overlay_imports(
        backend.overlay_repo().as_ref(),
        &overlays.root().await,
    )
    .await
    {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, "abandoned overlay imports removed"),
        Err(error) => tracing::warn!(%error, "abandoned overlay imports could not be swept"),
    }
    let overlay_pass = overlays.materialize_all(overlay_samples.as_ref()).await;
    match &overlay_pass {
        Ok(pass) => tracing::info!(
            materialized = pass.materialized,
            unavailable = pass.unavailable,
            failed = pass.failed,
            "overlay pages materialized"
        ),
        Err(e) => tracing::error!(error = %e, "overlay materialization pass failed"),
    }
    let overlay_build_failure = OverlayBuildFailure::from_outcome(&overlay_pass);

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
        settings_catalog,
        startup_language,
        startup_display_formats,
        anonymous_donor,
        date_names,
        latest_values,
        credentials_key_loss,
        overlay_build_failure,
        boot_warnings,
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
        chat_history_retention,
        notification_policy,
        transfer_exclusivity,
        scheduled_backups,
        builtins: integrations.builtins,
        integrations: supervisor,
        obs_install_seed: integrations.obs_install_seed,
        vtube_install_seed: integrations.vtube_install_seed,
        discord_client: integrations.discord_client,
        midi_sink: integrations.midi_sink,
        hotkey_client: integrations.hotkey_client,
        hotkey_reconciler: integrations.hotkey_reconciler,
        donation_services: integrations.donation_services,
        twitch_bot: integrations.twitch_bot,
        twitch_sign_in: integrations.twitch_sign_in,
        youtube_sign_in: integrations.youtube_sign_in,
        kick_sign_in: integrations.kick_sign_in,
        platform_credentials: integrations.platform_credentials,
        server,
        overlays,
        overlay_samples,
        overlay_actions,
        overlay_kinds,
        speak,
        speak_events,
        pipeline_config,
        bot_accounts,
        tts_registry,
        cloud_tts_gateways,
        speech_output,
        server_unavailable,
        audio_router,
        soundboard_player,
        voice_gate,
        stay_awake,
        first_run,
    })
}

async fn load_bot_accounts(
    backend: &Arc<dyn DataProvider>,
    warnings: &mut BootWarnings,
) -> forge_types::Shared<Vec<String>> {
    match backend.tts_filters_repo().get_pipeline_settings().await {
        Ok(settings) => forge_types::Shared::new(settings.bot_accounts),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load configured bot accounts on boot");
            warnings.record(BootWarning::BotAccounts);
            forge_types::Shared::default()
        }
    }
}

async fn build_voice_gate(
    settings: &dyn SettingsRepo,
    speak: Option<forge_speak_queue::SpeakQueueHandle>,
    warnings: &mut BootWarnings,
) -> Arc<VoiceGateOwner> {
    let owner = Arc::new(VoiceGateOwner::new(
        tokio::runtime::Handle::current(),
        speak,
    ));
    match forge_storage::voice_gate_settings(settings).await {
        Ok(settings) if settings.enabled => owner.start(config_from_settings(&settings)),
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "failed to load voice gate settings; leaving it off");
            warnings.record(BootWarning::VoiceGate);
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
    available_platforms: std::collections::BTreeSet<PlatformId>,
    delivery_hub: &DeliveryHub,
) -> ServerBoot {
    let settings = match forge_server::ServerSettings::load(backend.as_ref()).await {
        Ok(settings) => settings,
        Err(e) => {
            crate::boot_signal::server_settings_unavailable(&e);
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
        let event_history: Arc<dyn EventHistory> = Arc::clone(bus) as Arc<dyn EventHistory>;
        let mut config = forge_server::ServerConfig::new(
            settings_repo,
            credentials,
            event_history,
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
        config.available_platforms = available_platforms.clone();
        config.delivery_hub = Some(delivery_hub.clone());
        if let Some(root) = settings
            .overlay_root
            .as_ref()
            .filter(|root| !root.is_empty())
        {
            config.overlay_root = std::path::PathBuf::from(root);
        }
        config
    };

    match (settings.enabled, bind_addr) {
        (true, Some(addr)) => match forge_server::start_server(build_config()).await {
            Ok(handle) => {
                crate::boot_signal::server_bound(handle.bind_addr().await);
                return ServerBoot::running(handle);
            }
            Err(e) => crate::boot_signal::server_bind_failed(addr, &e),
        },
        (true, None) => crate::boot_signal::server_address_invalid(&settings.bind_address),
        (false, _) => crate::boot_signal::server_disabled(),
    }

    match forge_server::stopped_server(build_config()).await {
        Ok(handle) => ServerBoot::running(handle),
        Err(e) => {
            tracing::error!(error = %e, "server state unavailable, leaving it off");
            ServerBoot::unavailable(e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{AccentColor, Density, ThemeId};
    use forge_storage::settings::core_settings::{ACCENT, FONT_BODY, FONT_MONO, THEME};
    use forge_storage::write_setting;

    use crate::settings_catalog::DENSITY;

    use super::{BackendOpening, BootFailure, load_appearance, open_boot_storage_with};
    use crate::test_support::{test_backend, worker_runtime};
    use forge_storage::DataProvider;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn the_stored_theme_accent_density_and_both_fonts_come_back_in_their_own_slots() {
        let (backend, _writes) = test_backend();
        write_setting(
            backend.as_ref(),
            &THEME,
            &Some(ThemeId::TokyoNight.storage_key().to_owned()),
        )
        .await
        .unwrap();
        write_setting(
            backend.as_ref(),
            &ACCENT,
            &Some(AccentColor::Green.storage_key().to_owned()),
        )
        .await
        .unwrap();
        write_setting(
            backend.as_ref(),
            &DENSITY,
            &Some(Density::Spacious.storage_key().to_owned()),
        )
        .await
        .unwrap();
        write_setting(backend.as_ref(), &FONT_BODY, &Some("Inter".to_owned()))
            .await
            .unwrap();
        write_setting(backend.as_ref(), &FONT_MONO, &Some("Iosevka".to_owned()))
            .await
            .unwrap();

        let appearance = load_appearance(backend.as_ref()).await;

        assert_eq!(
            (
                appearance.theme,
                appearance.accent,
                appearance.density,
                appearance.body_font.as_deref(),
                appearance.mono_font.as_deref()
            ),
            (
                ThemeId::TokyoNight,
                AccentColor::Green,
                Density::Spacious,
                Some("Inter"),
                Some("Iosevka")
            )
        );
    }

    #[tokio::test]
    async fn unrecognised_or_missing_appearance_settings_fall_back_to_the_defaults() {
        let (backend, _writes) = test_backend();
        backend.seed_raw(&THEME, "solarized");
        backend.seed_raw(&ACCENT, "teal");
        backend.seed_raw(&DENSITY, "roomy");

        let appearance = load_appearance(backend.as_ref()).await;

        assert_eq!(
            (
                appearance.theme,
                appearance.accent,
                appearance.density,
                appearance.body_font,
                appearance.mono_font
            ),
            (
                ThemeId::default(),
                AccentColor::Mauve,
                Density::default(),
                None,
                None
            )
        );
    }

    #[tokio::test]
    async fn a_store_without_an_accent_boots_with_mauve() {
        let (backend, _writes) = test_backend();

        let appearance = load_appearance(backend.as_ref()).await;

        assert_eq!(appearance.accent, AccentColor::Mauve);
    }

    fn opened_backend() -> Arc<dyn DataProvider> {
        test_backend().0
    }

    #[tokio::test]
    async fn a_slow_open_is_waited_for_and_hands_over_the_backend_it_opened() {
        let backend = opened_backend();
        let handed = Arc::clone(&backend);
        let (finish_open, open_gate) = tokio::sync::oneshot::channel::<()>();
        let opening = BackendOpening(tokio::spawn(async move {
            open_gate.await.ok();
            Ok(handed)
        }));

        let waiting = tokio::spawn(opening.finished());
        tokio::task::yield_now().await;
        let still_waiting = !waiting.is_finished();
        finish_open.send(()).unwrap();
        let finished = waiting.await.unwrap();

        assert!(still_waiting);
        assert!(finished.is_ok_and(|opened| Arc::ptr_eq(&opened, &backend)));
    }

    #[tokio::test]
    async fn an_upgrade_failure_passes_through_with_both_versions() {
        let opening = BackendOpening(tokio::spawn(async {
            Err(BootFailure::UpgradeRequired {
                expected: 9,
                found: 4,
            })
        }));

        let failure = opening.finished().await.err();

        assert!(matches!(
            failure,
            Some(BootFailure::UpgradeRequired {
                expected: 9,
                found: 4
            })
        ));
    }

    #[tokio::test]
    async fn a_pre_baseline_failure_passes_through_with_its_version() {
        let opening = BackendOpening(tokio::spawn(async {
            Err(BootFailure::PreBaseline { found: 3 })
        }));

        let failure = opening.finished().await.err();

        assert!(matches!(
            failure,
            Some(BootFailure::PreBaseline { found: 3 })
        ));
    }

    #[tokio::test]
    async fn a_retryable_failure_passes_through_with_its_reason() {
        let opening = BackendOpening(tokio::spawn(async {
            Err(BootFailure::Retry {
                reason: "disk gone".to_owned(),
            })
        }));

        let failure = opening.finished().await.err();

        assert!(matches!(
            failure,
            Some(BootFailure::Retry { reason }) if reason == "disk gone"
        ));
    }

    #[tokio::test]
    async fn an_open_that_died_reads_as_a_retryable_failure() {
        let dying = tokio::spawn(std::future::pending());
        dying.abort();

        let failure = BackendOpening(dying).finished().await.err();

        assert!(matches!(failure, Some(BootFailure::Retry { .. })));
    }

    #[test]
    fn boot_storage_runs_the_open_once_and_hands_that_backend_to_the_waiter() {
        let rt = worker_runtime();
        let backend = opened_backend();
        let handed = Arc::clone(&backend);
        let opens = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&opens);

        let storage = open_boot_storage_with(rt.handle(), async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(handed)
        });
        let finished = rt.block_on(storage.opening.finished());

        assert_eq!(
            (
                opens.load(Ordering::SeqCst),
                finished.is_ok_and(|opened| Arc::ptr_eq(&opened, &backend))
            ),
            (1, true)
        );
    }

    #[test]
    fn boot_storage_passes_a_failed_open_through_without_a_backend() {
        let rt = worker_runtime();

        let storage = open_boot_storage_with(rt.handle(), async {
            Err(BootFailure::PreBaseline { found: 3 })
        });
        let early_backend = storage.backend.is_some();
        let failure = rt.block_on(storage.opening.finished()).err();

        assert!(!early_backend);
        assert!(matches!(
            failure,
            Some(BootFailure::PreBaseline { found: 3 })
        ));
    }
}
