#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, SubActionRegistry, TriggerCategory,
    TriggerKindDescriptor, TriggerRegistry,
};
use forge_runtime::{
    ActionCancelRegistry, Catalog, EventBus, ExecutionRequest, spawn_action_engine,
};
use forge_storage::action::MockActionRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{CatalogRevision, EventLogRepo, StorageError};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMetadata, ExecutionMode, QueueId, TriggerConfig,
};
use time::OffsetDateTime;

const CHAT_KIND: &str = "twitch.channel.chat.message";
const RUN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

struct ChatDescriptor;

impl TriggerKindDescriptor for ChatDescriptor {
    fn id(&self) -> &str {
        CHAT_KIND
    }
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }
    fn label(&self) -> &str {
        ""
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }
    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn condition_display(&self, _: &TriggerConfig) -> String {
        String::new()
    }
    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(CHAT_KIND.to_owned()),
        }
    }
    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }
    fn delivery_lane(&self) -> DeliveryLane {
        DeliveryLane::Bulk
    }
}

#[derive(Default)]
struct StoredEvents(Mutex<Vec<Event>>);

#[async_trait]
impl EventLogRepo for StoredEvents {
    async fn insert(&self, event: &Event) -> Result<(), StorageError> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
    async fn get(&self, _: EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }
    async fn recent(&self, _: usize) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }
    async fn recent_since(&self, _: usize, _: Option<EventId>) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }
    async fn prune_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

fn empty_action() -> Action {
    Action {
        id: ActionId::new(),
        name: "greet".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: Vec::new(),
    }
}

struct Persisted {
    event_log: Vec<(String, Option<EventId>)>,
    run_triggers: Vec<EventId>,
}

/// Runs one action for a chat message and one for a follow, then drains every persisting consumer.
async fn run_for_chat_and_follow(chat: &Event, follow: &Event) -> Persisted {
    let action = empty_action();
    let listed = vec![action.clone()];
    let mut actions = MockActionRepo::new();
    actions.expect_list().returning(move || Ok(listed.clone()));
    let served = action.clone();
    actions
        .expect_get()
        .returning(move |_| Ok(Some(served.clone())));
    actions.expect_record_executions().returning(|_| Ok(()));

    let run_triggers = Arc::new(Mutex::new(Vec::new()));
    let saved = Arc::clone(&run_triggers);
    let mut history = MockHistoryRepo::new();
    history.expect_save_batch().returning(move |contexts| {
        saved
            .lock()
            .unwrap()
            .extend(contexts.iter().filter_map(|ctx| match ctx.metadata {
                ExecutionMetadata::Trigger { event_id, .. } => Some(event_id),
                ExecutionMetadata::QuickAction { .. } => None,
            }));
        Ok(())
    });

    let mut instances = MockTriggerInstanceRepo::new();
    instances.expect_list_for_action().returning(|_| Ok(vec![]));
    let actions = Arc::new(actions);
    let catalog = Catalog::new(
        Arc::clone(&actions) as _,
        Arc::new(instances),
        CatalogRevision::new(),
    );

    let event_log = Arc::new(StoredEvents::default());
    let bus = EventBus::new(Arc::clone(&event_log) as Arc<dyn EventLogRepo>);
    let mut triggers = TriggerRegistry::new();
    triggers.register(Box::new(ChatDescriptor)).unwrap();
    bus.declare_lanes(&triggers);
    EventBus::spawn_flush_task(Arc::clone(&bus));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        catalog,
        actions,
        Arc::new(history),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    );

    let mut observer = bus.subscribe();
    for root in [chat, follow] {
        bus.publish(root.clone());
        engine
            .dispatch(ExecutionRequest {
                action_id: action.id,
                trigger_event_id: root.id,
                trigger_kind: None,
                initial_args: ArgStack::new(),
            })
            .await
            .unwrap();
    }
    let mut done = 0;
    while done < 2 {
        let event = tokio::time::timeout(RUN_DEADLINE, observer.recv())
            .await
            .unwrap()
            .unwrap();
        done += usize::from(event.kind == "action.done");
    }
    bus.shutdown();
    bus.await_flush().await;

    let event_log = event_log
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|event| (event.kind.clone(), event.caused_by))
        .collect();
    let mut run_triggers = run_triggers.lock().unwrap().clone();
    run_triggers.sort();
    Persisted {
        event_log,
        run_triggers,
    }
}

fn chat_and_follow() -> (Event, Event) {
    (
        Event::new(EventSource::Twitch, CHAT_KIND, serde_json::Value::Null),
        Event::new(
            EventSource::Twitch,
            "twitch.channel.follow",
            serde_json::Value::Null,
        ),
    )
}

#[tokio::test]
async fn a_chat_triggered_run_leaves_no_telemetry_rows_while_a_follow_triggered_run_does() {
    let (chat, follow) = chat_and_follow();

    let persisted = run_for_chat_and_follow(&chat, &follow).await;

    let row_causes = |wanted: &str| -> Vec<Option<EventId>> {
        persisted
            .event_log
            .iter()
            .filter(|(kind, _)| kind == wanted)
            .map(|(_, cause)| *cause)
            .collect()
    };
    assert_eq!(
        (row_causes("action.start"), row_causes("action.done").len()),
        (vec![Some(follow.id)], 1),
        "only the follow-triggered run's start and done are stored"
    );
}

#[tokio::test]
async fn a_chat_triggered_run_still_stores_its_chat_message_and_its_run_history() {
    let (chat, follow) = chat_and_follow();

    let persisted = run_for_chat_and_follow(&chat, &follow).await;

    let chat_rows = persisted
        .event_log
        .iter()
        .filter(|(kind, _)| kind == CHAT_KIND)
        .count();
    assert_eq!(
        (chat_rows, persisted.run_triggers),
        (1, {
            let mut both = vec![chat.id, follow.id];
            both.sort();
            both
        })
    );
}
