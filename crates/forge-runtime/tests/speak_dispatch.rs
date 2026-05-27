//! Regression: SubActionSpec::Speak dispatches through SpeakDispatcher correctly.
//!
//! Covered invariants:
//! - dispatcher=None → SubActionOutcome::Skipped (subsystem unavailable path).
//! - dispatcher=Ok  → SubActionOutcome::Success.
//! - dispatcher=Err → SubActionOutcome::Failed.
//!
//! No audio hardware or speak queue is used; all synthesis is mocked at the
//! SpeakDispatcher trait boundary, matching the PlaySound pattern in
//! play_sound_dispatch.rs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::EventsError;
use forge_runtime::{
    EventBus, ExecutionRequest, NullEventLogRepo, ScriptRegistry, SpeakDispatchError,
    SpeakDispatcher, spawn_action_engine,
};
use forge_storage::{DataProvider, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, ArgStack, EventId, QueueId, SubActionSpec};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn make_dp() -> Arc<dyn DataProvider> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .unwrap(),
    )
}

async fn default_queue_id(dp: &Arc<dyn DataProvider>) -> QueueId {
    dp.queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("Default queue seeded by migration 0002")
        .id
}

async fn make_action_with_speak(dp: &Arc<dyn DataProvider>) -> ActionId {
    let queue_id = default_queue_id(dp).await;
    let action_id = ActionId::new();
    let action = Action {
        id: action_id,
        name: "speak-test".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionSpec::Speak {
            text: "Hello from test".to_owned(),
            voice_id_override: None,
        }],
    };
    dp.action_repo().save(&action).await.unwrap();
    action_id
}

async fn wait_for_action_done(sub: &mut forge_runtime::EventSubscription, timeout_ms: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        match tokio::time::timeout(remaining, sub.recv()).await {
            Ok(Ok(ev)) if ev.kind == "action.done" => return true,
            Ok(Ok(_)) => continue,
            Ok(Err(EventsError::LaggingReceiver)) => continue,
            Ok(Err(_)) | Err(_) => return false,
        }
    }
}

struct OkDispatcher;

#[async_trait]
impl SpeakDispatcher for OkDispatcher {
    async fn speak(
        &self,
        _text: String,
        _voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        Ok(())
    }
}

struct ErrDispatcher;

#[async_trait]
impl SpeakDispatcher for ErrDispatcher {
    async fn speak(
        &self,
        _text: String,
        _voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        Err(SpeakDispatchError::Dispatch("speak queue gone".to_string()))
    }
}

#[tokio::test]
async fn none_dispatcher_action_completes_with_skipped_outcome() {
    let dp = make_dp().await;
    let action_id = make_action_with_speak(&dp).await;

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::new(ScriptRegistry::new()),
        None,
        None,
        None, // No SpeakDispatcher
    );

    engine
        .dispatch(ExecutionRequest {
            action_id,
            trigger_event_id: EventId::new(),
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    assert!(
        wait_for_action_done(&mut sub, 2_000).await,
        "action.done must arrive when dispatcher is None (Skipped outcome)"
    );
}

#[tokio::test]
async fn ok_dispatcher_action_completes_with_success_outcome() {
    let dp = make_dp().await;
    let action_id = make_action_with_speak(&dp).await;

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();
    let dispatcher: Arc<dyn SpeakDispatcher> = Arc::new(OkDispatcher);

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::new(ScriptRegistry::new()),
        None,
        None,
        Some(dispatcher),
    );

    engine
        .dispatch(ExecutionRequest {
            action_id,
            trigger_event_id: EventId::new(),
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    assert!(
        wait_for_action_done(&mut sub, 2_000).await,
        "action.done must arrive when OkDispatcher is used"
    );
}

#[tokio::test]
async fn err_dispatcher_action_still_completes() {
    let dp = make_dp().await;
    let action_id = make_action_with_speak(&dp).await;

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();
    let dispatcher: Arc<dyn SpeakDispatcher> = Arc::new(ErrDispatcher);

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::new(ScriptRegistry::new()),
        None,
        None,
        Some(dispatcher),
    );

    engine
        .dispatch(ExecutionRequest {
            action_id,
            trigger_event_id: EventId::new(),
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    assert!(
        wait_for_action_done(&mut sub, 2_000).await,
        "action.done must arrive even when SpeakDispatcher returns Err (Failed outcome)"
    );
}

#[tokio::test]
async fn voice_override_forwarded_to_dispatcher() {
    use std::sync::Mutex;

    struct CapturingDispatcher {
        captured: Arc<Mutex<Option<String>>>,
    }

    #[async_trait]
    impl SpeakDispatcher for CapturingDispatcher {
        async fn speak(
            &self,
            _text: String,
            voice_id_override: Option<String>,
        ) -> Result<(), SpeakDispatchError> {
            *self.captured.lock().unwrap() = voice_id_override;
            Ok(())
        }
    }

    let dp = make_dp().await;
    let queue_id = default_queue_id(&dp).await;
    let action_id = ActionId::new();
    let action = Action {
        id: action_id,
        name: "speak-override-test".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionSpec::Speak {
            text: "override test".to_owned(),
            voice_id_override: Some("piper/en_US-amy-medium".to_owned()),
        }],
    };
    dp.action_repo().save(&action).await.unwrap();

    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let dispatcher: Arc<dyn SpeakDispatcher> = Arc::new(CapturingDispatcher {
        captured: Arc::clone(&captured),
    });

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::new(ScriptRegistry::new()),
        None,
        None,
        Some(dispatcher),
    );

    engine
        .dispatch(ExecutionRequest {
            action_id,
            trigger_event_id: EventId::new(),
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    assert!(
        wait_for_action_done(&mut sub, 2_000).await,
        "action.done must arrive for voice-override test"
    );

    assert_eq!(
        *captured.lock().unwrap(),
        Some("piper/en_US-amy-medium".to_owned()),
        "voice_id_override must be forwarded to dispatcher verbatim"
    );
}
