#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test fixtures fail loudly by design"
)]

use std::sync::Arc;

use forge_events::{ActionDone, Delivery, Event, EventSource, EventSubscription, TypedEvent};
use forge_host::host_handles::Host;
use forge_host::integration_catalog::declarations;
use forge_host::platform_identities::PlatformIdentities;
use forge_host::test_support::{
    ALL_INTEGRATIONS, RecordingSinkFactory, boot_host, fake_identities, guarded, on_app_runtime,
    open_store, runs_here_in_isolation, seed_integrations_and_server_off, tone,
};
use forge_runtime::LifecycleState;
use forge_runtime::{GLOBAL_SET_KIND_ID, GLOBAL_VALUE_KEY};
use forge_storage::{DataProvider, GlobalsRepo, read_setting_or_default};
use forge_transfer::ScheduleState;
use forge_types::{
    Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMetadata, ExecutionMode, ExecutionOutcome,
    PermissionRung, PlatformScope, Shared, SubActionConfig, SubActionStep, TriggerConfig,
    TriggerInstance, TriggerInstanceId, Variant,
};

const CUSTOM_EVENT_NAME: &str = "boot_smoke";
const GLOBAL_NAME: &str = "boot_smoke_ran";
const GLOBAL_VALUE: &str = "the action ran";

const IDENTITY_GATED_STEPS: &[&str] = &[
    "twitch.chat.send_announcement",
    "twitch.guest_star.end_session",
    "youtube.chat.send_message",
    "youtube.lookup.viewer",
    "kick.chat.send_message",
    "kick.lookup.category",
];

const ALWAYS_WIRED_STEPS: &[&str] = &[
    "obs.collection.switch",
    "vtube.model.set_physics",
    "discord.webhook.send_message",
    "discord.webhook.delete_message",
    "midi.send",
    "tts.speak.text",
    "soundboard.sound.stop",
    "core.latest.get",
    GLOBAL_SET_KIND_ID,
];

async fn seed_action_on_custom_event(backend: &Arc<dyn DataProvider>) -> ActionId {
    let action = Action {
        id: ActionId::new(),
        name: "boot smoke".to_owned(),
        group: None,
        queue_id: DEFAULT_QUEUE_ID,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionStep {
            kind_id: GLOBAL_SET_KIND_ID.to_owned(),
            config: SubActionConfig::from([
                ("name".to_owned(), Variant::String(GLOBAL_NAME.to_owned())),
                (
                    GLOBAL_VALUE_KEY.to_owned(),
                    Variant::String(GLOBAL_VALUE.to_owned()),
                ),
                ("persisted".to_owned(), Variant::Bool(true)),
            ]),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }],
    };
    backend.action_repo().save(&action).await.unwrap();
    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "script.event.custom".to_owned(),
        name: "boot smoke".to_owned(),
        overrides: TriggerConfig::from([(
            "event_name".to_owned(),
            Variant::String(CUSTOM_EVENT_NAME.to_owned()),
        )]),
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    let triggers = backend.trigger_instance_repo();
    triggers.save(&instance).await.unwrap();
    triggers.link_action(action.id, instance.id).await.unwrap();
    action.id
}

struct Booted {
    host: Host,
    backend: Arc<dyn DataProvider>,
    sinks: Arc<RecordingSinkFactory>,
    action: ActionId,
}

async fn boot(platform_identities: PlatformIdentities) -> Booted {
    let backend = open_store().await;
    seed_integrations_and_server_off(backend.settings_repo().as_ref()).await;
    let action = seed_action_on_custom_event(&backend).await;
    let booted = boot_host(
        Arc::clone(&backend),
        platform_identities,
        Shared::new("anonymous".to_owned()),
    )
    .await;
    Booted {
        host: booted.host,
        backend,
        sinks: booted.sinks,
        action,
    }
}

async fn until_action_done(subscription: &mut EventSubscription) {
    loop {
        match subscription.next().await {
            Delivery::Event(event) if event.kind == ActionDone::KIND => return,
            Delivery::Event(_) | Delivery::Skipped(_) => {}
            Delivery::Closed => panic!("the bus closed before the action finished"),
        }
    }
}

fn missing_steps(host: &Host, kind_ids: &[&'static str]) -> Vec<&'static str> {
    kind_ids
        .iter()
        .copied()
        .filter(|kind_id| host.automation.sub_action_registry.get(kind_id).is_none())
        .collect()
}

#[test]
fn every_integration_with_stored_sign_in_gets_a_sign_out_route_at_boot() {
    if !runs_here_in_isolation(
        "every_integration_with_stored_sign_in_gets_a_sign_out_route_at_boot",
    ) {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;

        let routed: Vec<String> = declarations()
            .into_iter()
            .filter(|declaration| {
                booted
                    .host
                    .integrations
                    .sign_out
                    .entry(&declaration.id)
                    .is_some()
            })
            .map(|declaration| declaration.id.to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        assert_eq!(routed, ["kick", "obs", "twitch", "vtube", "youtube"]);
    });
}

#[test]
fn supplied_identities_register_every_platform_step_list_from_first_to_last() {
    if !runs_here_in_isolation(
        "supplied_identities_register_every_platform_step_list_from_first_to_last",
    ) {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;

        let missing = missing_steps(
            &booted.host,
            &[IDENTITY_GATED_STEPS, ALWAYS_WIRED_STEPS].concat(),
        );

        assert_eq!(missing, Vec::<&str>::new());
    });
}

#[test]
fn a_missing_identity_drops_only_the_steps_that_need_one() {
    if !runs_here_in_isolation("a_missing_identity_drops_only_the_steps_that_need_one") {
        return;
    }
    on_app_runtime(async {
        let booted = boot(PlatformIdentities::default()).await;

        let missing_gated = missing_steps(&booted.host, IDENTITY_GATED_STEPS);
        let missing_wired = missing_steps(&booted.host, ALWAYS_WIRED_STEPS);

        assert_eq!(
            (missing_gated, missing_wired),
            (IDENTITY_GATED_STEPS.to_vec(), Vec::new())
        );
    });
}

#[test]
fn every_integration_is_wired_and_stays_disabled_when_seeded_off() {
    if !runs_here_in_isolation("every_integration_is_wired_and_stays_disabled_when_seeded_off") {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;

        let states = booted.host.integrations.supervisor.watch().current();

        for integration in &ALL_INTEGRATIONS {
            assert!(
                matches!(states.get(&integration.id), Some(LifecycleState::Disabled)),
                "{}: {:?}",
                integration.brand_name,
                states.get(&integration.id)
            );
        }
    });
}

#[test]
fn the_scheduler_and_the_stopped_server_answer_after_boot() {
    if !runs_here_in_isolation("the_scheduler_and_the_stopped_server_answer_after_boot") {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;
        let host = &booted.host;

        let queues = guarded("the scheduler", host.automation.scheduler.queue_states())
            .await
            .unwrap();
        let server = host.server.as_ref().unwrap();
        guarded("the server", server.snapshot()).await;

        assert!(queues.contains_key(&DEFAULT_QUEUE_ID));
        assert!(!server.run_state().is_running());
    });
}

#[test]
fn a_custom_event_runs_its_seeded_action_and_leaves_history_and_a_global() {
    if !runs_here_in_isolation(
        "a_custom_event_runs_its_seeded_action_and_leaves_history_and_a_global",
    ) {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;
        let host = &booted.host;
        let mut subscription = host.core.bus.subscribe();
        let event = Event::new(
            EventSource::Server,
            format!("custom.{CUSTOM_EVENT_NAME}"),
            serde_json::json!({}),
        );
        let event_id = event.id;

        host.core.bus.publish(event);
        guarded("the action run", until_action_done(&mut subscription)).await;
        host.core.bus.shutdown();
        guarded("the event and history flush", host.core.bus.await_flush()).await;

        let runs = booted
            .backend
            .history_repo()
            .recent_for_action(booted.action, 10)
            .await
            .unwrap();
        let [run] = runs.as_slice() else {
            panic!("expected one recorded run, got {}", runs.len());
        };
        assert_eq!(run.outcome, ExecutionOutcome::Success);
        assert!(matches!(
            run.metadata,
            ExecutionMetadata::Trigger { event_id: recorded, .. } if recorded == event_id
        ));
        let globals: Arc<dyn GlobalsRepo> = booted.backend.globals_repo();
        assert_eq!(
            globals.get(GLOBAL_NAME).await.unwrap(),
            Some(Variant::String(GLOBAL_VALUE.to_owned()))
        );
    });
}

#[test]
fn applying_routes_opens_no_sink_and_speech_then_reaches_the_injected_factory() {
    if !runs_here_in_isolation(
        "applying_routes_opens_no_sink_and_speech_then_reaches_the_injected_factory",
    ) {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;
        let host = &booted.host;

        let applied = guarded("the route apply", host.audio.audio_router.apply()).await;
        let builds_after_apply = booted.sinks.builds();
        host.audio
            .audio_router
            .speech_sink()
            .play(tone())
            .await
            .unwrap();

        assert!(applied.is_some());
        assert_eq!(
            (
                builds_after_apply,
                booted.sinks.builds(),
                booted.sinks.sink.calls()
            ),
            (0, 1, 1)
        );
    });
}

#[test]
fn the_graceful_stop_returns_and_leaves_a_store_that_reopens_with_its_settings() {
    if !runs_here_in_isolation(
        "the_graceful_stop_returns_and_leaves_a_store_that_reopens_with_its_settings",
    ) {
        return;
    }
    on_app_runtime(async {
        let booted = boot(fake_identities()).await;
        let host = &booted.host;

        guarded("the graceful stop", host.stop.clone().run()).await;
        guarded("the flush tasks", host.core.bus.await_flush()).await;
        guarded("the storage close", booted.backend.shutdown()).await;

        let mut backups = host.core.scheduled_backups.status();
        guarded(
            "the scheduled backup stop",
            backups.wait_for(|status| status.state != ScheduleState::Running),
        )
        .await
        .unwrap();
        let reopened = guarded("the reopen", open_store()).await;
        let server_enabled = read_setting_or_default(
            reopened.settings_repo().as_ref(),
            &forge_server::settings::ENABLED,
        )
        .await;
        assert!(!server_enabled);
    });
}
