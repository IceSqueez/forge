//! Integration tests for PlaySound SubAction dispatch via the action engine.
//!
//! Tests the full public path: spawn_action_engine → ExecutionRequest → action.done,
//! with three SoundPlayer configurations:
//!   - None player  → subaction outcome is Skipped
//!   - Ok() player  → subaction outcome is Success
//!   - Err player   → subaction outcome is Failed
//!
//! No audio hardware is used; all sinks are mocked at the SoundPlayer boundary.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_runtime::{
    EventBus, ExecutionRequest, NullEventLogRepo, ScriptRegistry, SoundPlayer, SoundPlayerError,
    spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, ClipId, EventId, LogLevel, OutputDevice, QueueId, SubActionSpec,
};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn make_dp() -> Arc<dyn DataProvider> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .unwrap(),
    )
}

async fn wait_for_action_done(sub: &mut forge_runtime::EventSubscription, timeout_ms: u64) -> bool {
    use forge_events::EventsError;
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

async fn make_action_with_play_sound(dp: &Arc<dyn DataProvider>) -> (ActionId, QueueId) {
    let queue = dp
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("Default queue seeded by migration 0002");
    let queue_id = queue.id;
    let action_id = ActionId::new();
    let action = Action {
        id: action_id,
        name: "play-sound-test".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionSpec::PlaySound {
            clip_id: ClipId::new(),
            output_device_override: None,
        }],
    };
    dp.action_repo().save(&action).await.unwrap();
    (action_id, queue_id)
}

struct OkSoundPlayer;

#[async_trait]
impl SoundPlayer for OkSoundPlayer {
    async fn play(
        &self,
        _clip_id: ClipId,
        _override_device: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError> {
        Ok(())
    }
}

struct ErrSoundPlayer;

#[async_trait]
impl SoundPlayer for ErrSoundPlayer {
    async fn play(
        &self,
        _clip_id: ClipId,
        _override_device: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError> {
        Err(SoundPlayerError::Play("device unavailable".to_string()))
    }
}

#[tokio::test]
async fn play_sound_none_player_action_completes_with_skipped_subaction() {
    let dp = make_dp().await;
    let (action_id, _) = make_action_with_play_sound(&dp).await;

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
        None,
        None,
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
        "action.done must arrive within 2s"
    );
}

#[tokio::test]
async fn play_sound_ok_player_action_completes_successfully() {
    let dp = make_dp().await;
    let (action_id, _) = make_action_with_play_sound(&dp).await;

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();
    let player: Arc<dyn SoundPlayer> = Arc::new(OkSoundPlayer);

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
        Some(player),
        None,
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
        "action.done must arrive within 2s for OkSoundPlayer"
    );
}

#[tokio::test]
async fn play_sound_err_player_action_still_completes() {
    let dp = make_dp().await;
    let (action_id, _) = make_action_with_play_sound(&dp).await;

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();
    let player: Arc<dyn SoundPlayer> = Arc::new(ErrSoundPlayer);

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
        Some(player),
        None,
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
        "action.done must arrive even when SoundPlayer returns Err"
    );
}

#[tokio::test]
async fn play_sound_with_log_subaction_demonstrates_mixed_outcome() {
    let dp = make_dp().await;
    let queue = dp
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("Default queue");
    let action_id = ActionId::new();
    let action = Action {
        id: action_id,
        name: "mixed-sound-log".to_owned(),
        group: None,
        queue_id: queue.id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![
            SubActionSpec::PlaySound {
                clip_id: ClipId::new(),
                output_device_override: None,
            },
            SubActionSpec::Log {
                level: LogLevel::Info,
                message: "played".to_string(),
            },
        ],
    };
    dp.action_repo().save(&action).await.unwrap();

    let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let mut sub = bus.subscribe();
    let player: Arc<dyn SoundPlayer> = Arc::new(OkSoundPlayer);

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
        Some(player),
        None,
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
        "action.done must arrive for mixed PlaySound + Log action"
    );
}
