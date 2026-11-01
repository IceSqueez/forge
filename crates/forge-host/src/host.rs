use std::path::PathBuf;
use std::sync::Arc;

use forge_audio::{AudioSink, AudioSinkFactory};
use forge_events::{EventObserver, EventPublisher};
use forge_hotkey::HOTKEY_INTEGRATION;
use forge_platform_core::PlatformEndpoints;
use forge_runtime::{
    CatchUpSettle, DateNamesHandle, DonationIngest, FrozenRuntime, IntegrationSupervisor,
    OverlayFrameSink, RuntimeWiring, RuntimeWiringParts, TwitchEmoteLexicon,
    register_core_triggers, register_donation_sub_actions, register_latest_sub_actions,
    register_speech_sub_actions, spawn_live_viewer_aggregator, spawn_stream_live_signal,
    spawn_timer_scheduler, spawn_twitch_emote_learning,
};
use forge_server::DeliveryHub;
use forge_soundboard::spawn_clip_hotkey_dispatcher;
use forge_storage::{DataProvider, SettingsRepo};
use forge_types::{LatestValueReader, Shared, SystemWallClock};

use crate::audio_router::{AudioRouter, AudioRouterParts};
use crate::boot_warnings::{BootWarning, BootWarnings};
use crate::core_boot::build_core;
use crate::host_handles::{
    AudioHandles, AutomationHandles, BootOutcomes, Host, IntegrationHandles, SpeechHandles,
};
use crate::integrations::{IntegrationInputs, build_integrations};
use crate::overlay_boot::{OverlayInputs, build_overlay_kinds, start_overlays};
use crate::platform_identities::PlatformIdentities;
use crate::server_boot::{ServerBoot, ServerInputs, build_server};
use crate::soundboard_boot::{SoundboardInputs, build_soundboard};
use crate::speak_boot::{LocalTtsEngine, Speech, build_speech};
use crate::stop::{OrderedStop, OrderedStopParts};

use forge_paths::AppPaths;

pub struct HostInputs {
    pub provider: Arc<dyn DataProvider>,
    pub data_dir: PathBuf,
    pub endpoints: PlatformEndpoints,
    pub hotkey_main_thread: forge_hotkey::MainThreadLink,
    pub audio_sinks: Arc<dyn AudioSinkFactory>,
    pub local_tts_engines: Vec<LocalTtsEngine>,
    pub platform_identities: PlatformIdentities,
    pub anonymous_donor: Shared<String>,
    pub date_names: DateNamesHandle,
}

pub async fn build_host(inputs: HostInputs) -> Host {
    let HostInputs {
        provider: backend,
        data_dir,
        endpoints,
        hotkey_main_thread,
        audio_sinks,
        local_tts_engines,
        platform_identities,
        anonymous_donor,
        date_names,
    } = inputs;
    let mut boot_warnings = BootWarnings::default();
    let paths = AppPaths::at(data_dir.clone());
    let core = build_core(&backend, data_dir, anonymous_donor.clone()).await;
    let bus = &core.bus;
    let services = &core.services;

    let Speech {
        output: speech_output,
        routed_sink: speech_sink,
        cloud_gateways: cloud_tts_gateways,
        queue: speak,
        events: speak_events,
        pipeline_config,
        voice_gate,
        ports: speech_ports,
        show_speaker,
        script_requester: speak_requester,
    } = build_speech(
        bus,
        &backend,
        Arc::clone(&audio_sinks),
        local_tts_engines,
        &mut boot_warnings,
    )
    .await;

    let overlay_kinds = build_overlay_kinds(&mut boot_warnings);
    let delivery_hub = DeliveryHub::new(Arc::clone(bus) as Arc<dyn EventObserver>);
    let mut wiring = RuntimeWiring::new(RuntimeWiringParts {
        tasks: services.clone(),
        bus: Arc::clone(bus),
        provider: Arc::clone(&backend),
        latest_values: core.latest_values.clone(),
        script_speech: speak_requester,
        overlay_speech: show_speaker,
        overlay_kinds: Arc::clone(&overlay_kinds),
        overlay_frames: Arc::new(delivery_hub.clone()) as Arc<dyn OverlayFrameSink>,
        clock: Arc::new(SystemWallClock),
        date_names: date_names.clone(),
        paths: paths.clone(),
        config: forge_runtime::Config::default(),
        os: crate::os_adapters::system_os_ports(),
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
            Arc::clone(bus) as Arc<dyn EventPublisher>,
            anonymous_donor,
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
        Arc::new(core.latest_values.clone()) as Arc<dyn LatestValueReader>,
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
        bus,
        &donations,
        IntegrationInputs {
            endpoints,
            hotkey_main_thread,
            platform_identities,
        },
    )
    .await;

    let soundboard_player = build_soundboard(
        wiring.sub_actions_mut(),
        SoundboardInputs {
            backend: &backend,
            bus,
            audio_sinks,
            hotkey_reconciler: integrations.hotkey_reconciler.clone(),
        },
        &mut boot_warnings,
    )
    .await;
    match speech_ports {
        Some(ports) => {
            let reward_emotes = TwitchEmoteLexicon::default();
            spawn_twitch_emote_learning(bus, reward_emotes.clone(), services);
            if let Err(e) =
                register_speech_sub_actions(wiring.sub_actions_mut(), ports, reward_emotes)
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

    spawn_clip_hotkey_dispatcher(
        bus.as_ref(),
        integrations.hotkey_reconciler.clip_bindings(),
        Arc::clone(&soundboard_player),
    );

    let FrozenRuntime {
        catalog,
        scripts: script_registry,
        overlays: overlay_service,
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
    let live_viewers = spawn_live_viewer_aggregator(services);
    let supervisor = IntegrationSupervisor::launch(
        integrations.factories,
        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
        action_engine.integration_gate().clone(),
        Arc::new(integrations.builtins.clone()),
        live_viewers.clone(),
    );
    let sign_out = integrations
        .sign_out_routes
        .over(Arc::new(supervisor.clone()));
    donations.recover_unannounced().await;
    supervisor.boot().await;
    let integration_states = supervisor.watch().current();
    if let Some(engine) = supervisor.slot(&HOTKEY_INTEGRATION.id) {
        integrations
            .hotkey_reconciler
            .enable_engine_on_new_bindings(move || engine.request_enable());
    }
    let stream_live = spawn_stream_live_signal(
        &live_viewers,
        integrations
            .obs_install_seed
            .stream_output()
            .into_receiver(),
        services,
    );
    let bot_accounts = load_bot_accounts(&backend, &mut boot_warnings).await;
    let timer_scheduler = spawn_timer_scheduler(
        Arc::clone(bus),
        Arc::clone(&catalog),
        stream_live.clone(),
        bot_accounts.clone(),
    );
    scheduled_runs_worker.start(CatchUpSettle::when(supervisor.watch().until_all_settled()));

    let ServerBoot {
        handle: server,
        unavailable: server_unavailable,
    } = build_server(
        ServerInputs {
            backend: &backend,
            bus,
            action_engine: &action_engine,
            available_platforms: trigger_registry.platforms(),
            default_overlay_root: paths.overlays_dir(),
            delivery_hub: &delivery_hub,
        },
        &mut boot_warnings,
    )
    .await;

    let (overlays, overlay_pass) = start_overlays(OverlayInputs {
        backend: &backend,
        bus,
        services,
        catalog: &catalog,
        sub_actions: &sub_action_registry,
        triggers: &trigger_registry,
        donations: &donations,
        delivery_hub: &delivery_hub,
        service: overlay_service,
        kinds: overlay_kinds,
        date_names,
    })
    .await;

    let audio_router = AudioRouter::started(AudioRouterParts {
        backend: Arc::clone(&backend),
        server: server.clone(),
        overlays: overlays.service.clone(),
        speech_sink: Arc::clone(&speech_sink),
        speech_output: Arc::clone(&speech_output) as Arc<dyn AudioSink>,
        soundboard_player: Arc::clone(&soundboard_player),
        speak: speak.clone(),
    })
    .await;

    let stop = OrderedStop::new(OrderedStopParts {
        bus: Arc::clone(bus),
        action_engine: action_engine.clone(),
        scheduler: scheduler.clone(),
        trigger_evaluator: trigger_evaluator.clone(),
        timer_scheduler: timer_scheduler.clone(),
        scheduled_runs: scheduled_runs.clone(),
        server: server.clone(),
        speak: speak.clone(),
        hotkey: integrations.hotkey_client.clone(),
        integrations: supervisor.clone(),
        services: services.clone(),
        storage: backend,
        scheduled_backups: core.scheduled_backups.clone(),
    });

    Host {
        core,
        automation: AutomationHandles {
            script_registry,
            sub_action_registry,
            trigger_registry,
            action_engine,
            scheduler,
            trigger_evaluator,
            scheduled_runs,
            timer_scheduler,
            live_viewers,
            bot_accounts,
        },
        integrations: IntegrationHandles {
            supervisor,
            builtins: integrations.builtins,
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
            sign_out,
        },
        speech: SpeechHandles {
            speak,
            speak_events,
            pipeline_config,
            cloud_tts_gateways,
            speech_output,
            voice_gate,
        },
        audio: AudioHandles {
            audio_router,
            soundboard_player,
        },
        overlays,
        server,
        outcomes: BootOutcomes {
            warnings: boot_warnings,
            server_unavailable,
            overlay_pass,
            integration_states,
        },
        stop,
    }
}

async fn load_bot_accounts(
    backend: &Arc<dyn DataProvider>,
    warnings: &mut BootWarnings,
) -> Shared<Vec<String>> {
    match backend.tts_filters_repo().get_pipeline_settings().await {
        Ok(settings) => Shared::new(settings.bot_accounts),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load configured bot accounts on boot");
            warnings.record(BootWarning::BotAccounts);
            Shared::default()
        }
    }
}
