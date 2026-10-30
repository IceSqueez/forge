use std::future::Future;
use std::net::TcpListener;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forge_audio::{AudioError, AudioSink, AudioSinkFactory};
use forge_events::{ActionDone, Delivery, Event, EventSource, EventSubscription, TypedEvent};
use forge_platform_core::{EndpointSurface, IntegrationDeclaration, PlatformEndpoints};
use forge_runtime::LifecycleState;
use forge_runtime::sub_action_runners::{GLOBAL_SET_KIND_ID, GLOBAL_VALUE_KEY};
use forge_storage::{
    DataProvider, GlobalsRepo, SettingsRepo, read_setting_or_default, set_integration_enabled,
    write_setting,
};
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::ScheduleState;
use forge_types::{
    Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMetadata, ExecutionMode, ExecutionOutcome,
    OutputDevice, PermissionRung, PlatformScope, SubActionConfig, SubActionStep, TriggerConfig,
    TriggerInstance, TriggerInstanceId, Variant,
};
use tokio::runtime::Builder;

use crate::boot::{BootFailure, RuntimeInputs, build_runtime};
use crate::log_tail::LogTail;
use crate::platform_identities::{OAuthClientIdentity, PlatformIdentities};
use crate::runtime_handles::RuntimeHandles;
use crate::shutdown::{RUNTIME_SHUTDOWN_BUDGET, ShutdownHandles, stop_scheduled_backups};
use crate::stay_awake::STAY_AWAKE;
use crate::test_support::{RecordingSink, tone};

const HANG_GUARD: Duration = Duration::from_secs(60);
const CHILD_MARKER: &str = "FORGE_BOOT_SMOKE_CHILD";
const CUSTOM_EVENT_NAME: &str = "boot_smoke";
const GLOBAL_NAME: &str = "boot_smoke_ran";
const GLOBAL_VALUE: &str = "the action ran";

const ALL_INTEGRATIONS: [IntegrationDeclaration; 10] = [
    forge_platform_twitch::TWITCH_INTEGRATION,
    forge_platform_youtube::YOUTUBE_INTEGRATION,
    forge_platform_kick::KICK_INTEGRATION,
    forge_obs::OBS_INTEGRATION,
    forge_vtube::VTUBE_INTEGRATION,
    forge_discord::DISCORD_INTEGRATION,
    forge_midi::MIDI_INTEGRATION,
    forge_hotkey::HOTKEY_INTEGRATION,
    forge_donatello::DONATELLO_INTEGRATION,
    forge_monobank::MONOBANK_INTEGRATION,
];

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

fn test_path(test_name: &str) -> String {
    let (_crate_name, module) = module_path!().split_once("::").unwrap();
    format!("{module}::{test_name}")
}

fn endpoint_override(surface: EndpointSurface, port: u16) -> String {
    let scheme = if surface.env_var().contains("_WS_") {
        "ws"
    } else {
        "http"
    };
    format!("{scheme}://127.0.0.1:{port}")
}

fn assert_nothing_dialled(listener: &TcpListener) {
    listener.set_nonblocking(true).unwrap();
    let error = listener.accept().map(|_| ()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

fn runs_here_in_isolation(test_name: &str) -> bool {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return true;
    }
    let home = tempfile::tempdir().unwrap();
    let data_dir = home.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let network_sentinel = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = network_sentinel.local_addr().unwrap().port();

    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            test_path(test_name).as_str(),
            "--exact",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        .env("FORGE_DATA_DIR", &data_dir)
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", home.path())
        .env_remove("FORGE_CREDENTIAL_KEY_FILE");
    for surface in EndpointSurface::ALL {
        child.env(surface.env_var(), endpoint_override(surface, port));
    }
    let absent_bus = format!("unix:path={}", home.path().join("absent-bus").display());
    child
        .env("DBUS_SESSION_BUS_ADDRESS", &absent_bus)
        .env("DBUS_SYSTEM_BUS_ADDRESS", &absent_bus);

    let output = child.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "isolated run of {test_name} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_nothing_dialled(&network_sentinel);
    false
}

fn on_app_runtime(body: impl Future<Output = ()>) {
    let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| runtime.block_on(body)));
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_BUDGET);
    if let Err(panic) = outcome {
        resume_unwind(panic);
    }
}

async fn guarded<F: Future>(work: &str, future: F) -> F::Output {
    tokio::time::timeout(HANG_GUARD, future)
        .await
        .unwrap_or_else(|_| panic!("{work} hung"))
}

#[derive(Default)]
struct RecordingSinkFactory {
    builds: AtomicUsize,
    sink: Arc<RecordingSink>,
}

impl RecordingSinkFactory {
    fn builds(&self) -> usize {
        self.builds.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AudioSinkFactory for RecordingSinkFactory {
    async fn build(&self, _: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::clone(&self.sink) as Arc<dyn AudioSink>)
    }
}

fn fake_identities() -> PlatformIdentities {
    let identity = |name: &str| OAuthClientIdentity {
        client_id: format!("{name}-client"),
        client_secret: format!("{name}-secret"),
    };
    PlatformIdentities {
        twitch_client_id: Some("twitch-client".to_owned()),
        youtube: Some(identity("youtube")),
        kick: Some(identity("kick")),
    }
}

fn database_url() -> String {
    let path = forge_paths::data_dir().join(forge_storage::DATABASE_FILE_NAME);
    format!("sqlite://{}?mode=rwc", path.display())
}

async fn open_store() -> Arc<dyn DataProvider> {
    Arc::new(SqliteBackend::open(&database_url()).await.unwrap())
}

async fn seed_everything_idle(settings: &dyn SettingsRepo) {
    for integration in &ALL_INTEGRATIONS {
        set_integration_enabled(settings, &integration.id, false)
            .await
            .unwrap();
    }
    write_setting(settings, &forge_server::settings::ENABLED, &false)
        .await
        .unwrap();
    write_setting(settings, &STAY_AWAKE, &false).await.unwrap();
}

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
    handles: RuntimeHandles,
    sinks: Arc<RecordingSinkFactory>,
    action: ActionId,
}

async fn boot(platform_identities: PlatformIdentities) -> Booted {
    let backend = open_store().await;
    seed_everything_idle(backend.as_ref()).await;
    let action = seed_action_on_custom_event(&backend).await;
    let sinks = Arc::new(RecordingSinkFactory::default());
    let inputs = RuntimeInputs {
        log_tail: LogTail::new(),
        endpoints: PlatformEndpoints::from_env().unwrap(),
        hotkey_main_thread: forge_hotkey::testing::offline_main_thread_link(),
        preopened: Some(backend),
        audio_sinks: Arc::clone(&sinks) as Arc<dyn AudioSinkFactory>,
        local_tts_engines: Vec::new(),
        platform_identities,
    };
    let handles = match guarded("the boot", build_runtime(inputs)).await {
        Ok(handles) => handles,
        Err(BootFailure::UpgradeRequired { expected, found }) => {
            panic!("boot wants schema {expected}, found {found}")
        }
        Err(BootFailure::PreBaseline { found }) => panic!("schema {found} is before the baseline"),
        Err(BootFailure::Retry { reason }) => panic!("boot failed: {reason}"),
    };
    Booted {
        handles,
        sinks,
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

fn missing_steps(handles: &RuntimeHandles, kind_ids: &[&'static str]) -> Vec<&'static str> {
    kind_ids
        .iter()
        .copied()
        .filter(|kind_id| handles.sub_action_registry.get(kind_id).is_none())
        .collect()
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
            &booted.handles,
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

        let missing_gated = missing_steps(&booted.handles, IDENTITY_GATED_STEPS);
        let missing_wired = missing_steps(&booted.handles, ALWAYS_WIRED_STEPS);

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

        let states = booted.handles.integrations.watch().current();

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
        let handles = &booted.handles;

        let queues = guarded("the scheduler", handles.scheduler.queue_states())
            .await
            .unwrap();
        let server = handles.server.as_ref().unwrap();
        guarded("the server", server.snapshot()).await;

        assert!(queues.contains_key(&DEFAULT_QUEUE_ID));
        assert!(!*server.run_state().borrow());
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
        let handles = &booted.handles;
        let mut subscription = handles.bus.subscribe();
        let event = Event::new(
            EventSource::Server,
            format!("custom.{CUSTOM_EVENT_NAME}"),
            serde_json::json!({}),
        );
        let event_id = event.id;

        handles.bus.publish(event);
        guarded("the action run", until_action_done(&mut subscription)).await;
        handles.bus.shutdown();
        guarded("the event and history flush", handles.bus.await_flush()).await;

        let runs = handles
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
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&handles.backend) as Arc<dyn GlobalsRepo>;
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
        let handles = &booted.handles;

        let applied = guarded("the route apply", handles.audio_router.apply()).await;
        let builds_after_apply = booted.sinks.builds();
        handles
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
                booted.sinks.sink.plays()
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
        let handles = &booted.handles;

        guarded("the graceful stop", async {
            tokio::join!(
                stop_scheduled_backups(handles.scheduled_backups.clone()),
                ShutdownHandles::from_handles(handles).run_graceful(),
            )
        })
        .await;
        guarded("the flush tasks", handles.bus.await_flush()).await;
        guarded("the storage close", handles.backend.shutdown()).await;

        let mut backups = handles.scheduled_backups.status();
        guarded(
            "the scheduled backup stop",
            backups.wait_for(|status| status.state != ScheduleState::Running),
        )
        .await
        .unwrap();
        let reopened = guarded("the reopen", open_store()).await;
        let server_enabled =
            read_setting_or_default(reopened.as_ref(), &forge_server::settings::ENABLED).await;
        assert!(!server_enabled);
    });
}
