#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_runtime::{
    EventBus, NullEventLogRepo, OverlayConnectListener, OverlayFrameSink, OverlayReceivers,
    OverlayServiceHandle,
};
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo,
};
use forge_types::{ArgStack, Variant};
use time::OffsetDateTime;
use tokio::sync::Notify;

const ALERT_KIND: &str = "overlay.alert";
const GOAL_KIND: &str = "overlay.goal";
const VALUE_KEY: &str = "value";
const HEADLINE_KEY: &str = "headline";

const HELD: &str = "held";
const WAIT_BOUND: Duration = Duration::from_secs(5);

/// A frame whose value equals `held` stops inside the sink until `release` fires; a frame is
/// recorded only once it has actually been delivered.
struct GatedSink {
    held: String,
    entered: Notify,
    release: Notify,
    frames: Mutex<Vec<(OverlayId, String)>>,
}

impl GatedSink {
    fn delivered_to(&self, id: &OverlayId) -> Vec<String> {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .filter(|(identity, _)| identity == id)
            .map(|(_, value)| value.clone())
            .collect()
    }
}

#[async_trait]
impl OverlayFrameSink for GatedSink {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        let value = content[VALUE_KEY]
            .as_str()
            .or_else(|| content[HEADLINE_KEY].as_str())
            .unwrap()
            .to_owned();
        if value == self.held {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.frames.lock().unwrap().push((identity.clone(), value));
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

type RetainedStore = Arc<Mutex<HashMap<OverlayId, OverlayConfig>>>;

struct Harness {
    sink: Arc<GatedSink>,
    retained: RetainedStore,
    service: OverlayServiceHandle,
}

impl Harness {
    fn retained_value(&self, id: &OverlayId) -> Option<String> {
        self.retained
            .lock()
            .unwrap()
            .get(id)
            .and_then(|content| match content.get(VALUE_KEY) {
                Some(Variant::String(value)) => Some(value.clone()),
                _ => None,
            })
    }

    fn deliver(&self, id: &OverlayId, value: &str) -> tokio::task::JoinHandle<()> {
        let service = self.service.clone();
        let id = id.clone();
        let content = content(value);
        tokio::spawn(async move {
            service
                .send_to(&id, &content, &ArgStack::new(), None)
                .await
                .expect("a bound overlay accepts content");
        })
    }

    async fn wait_until_held(&self) {
        tokio::time::timeout(WAIT_BOUND, self.sink.entered.notified())
            .await
            .expect("the held frame reached the sink");
    }
}

fn content(value: &str) -> OverlayConfig {
    OverlayConfig::from([
        (VALUE_KEY.to_owned(), Variant::String(value.to_owned())),
        (HEADLINE_KEY.to_owned(), Variant::String(value.to_owned())),
    ])
}

fn definition(id: &str, kind_id: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: id.to_owned(),
        kind_id: kind_id.to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::new(),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn harness(overlays: &[(&str, &str)]) -> Harness {
    let definitions: Vec<OverlayDefinition> = overlays
        .iter()
        .map(|(id, kind_id)| definition(id, kind_id))
        .collect();
    let retained: RetainedStore = Arc::default();

    let mut repo = MockOverlayRepo::new();
    repo.expect_get()
        .returning(move |id| Ok(definitions.iter().find(|d| &d.id == id).cloned()));
    let writes = Arc::clone(&retained);
    repo.expect_set_retained_content()
        .returning(move |id, content| {
            writes.lock().unwrap().insert(id.clone(), content.clone());
            Ok(())
        });
    let reads = Arc::clone(&retained);
    repo.expect_get_retained_content()
        .returning(move |id| Ok(reads.lock().unwrap().get(id).cloned()));

    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");

    let sink = Arc::new(GatedSink {
        held: HELD.to_owned(),
        entered: Notify::new(),
        release: Notify::new(),
        frames: Mutex::default(),
    });
    let service = OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(MockSettingsRepo::new()) as Arc<dyn SettingsRepo>,
        Arc::new(kinds),
        EventBus::new(Arc::new(NullEventLogRepo)),
        Some(Arc::clone(&sink) as Arc<dyn OverlayFrameSink>),
    );

    Harness {
        sink,
        retained,
        service,
    }
}

/// Under paused time the clock only advances once every task is idle, so this returns after
/// every spawned delivery has run as far as it can.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

#[tokio::test(start_paused = true)]
async fn concurrent_pushes_to_a_replace_overlay_leave_the_retained_content_equal_to_the_last_frame_shown()
 {
    let harness = harness(&[("goal-box", GOAL_KIND)]);
    let goal = OverlayId::new("goal-box");

    let first = harness.deliver(&goal, HELD);
    harness.wait_until_held().await;
    let second = harness.deliver(&goal, "newer");
    settle().await;
    harness.sink.release.notify_one();
    first.await.unwrap();
    second.await.unwrap();

    let shown = harness.sink.delivered_to(&goal);
    assert_eq!(
        harness.retained_value(&goal).as_ref(),
        shown.last(),
        "the page ends on {shown:?} but a reconnect would replay something else"
    );
}

#[tokio::test(start_paused = true)]
async fn a_replay_racing_a_push_never_leaves_the_page_on_the_older_content() {
    let harness = harness(&[("goal-box", GOAL_KIND)]);
    let goal = OverlayId::new("goal-box");
    harness
        .retained
        .lock()
        .unwrap()
        .insert(goal.clone(), content(HELD));

    let replay = tokio::spawn({
        let service = harness.service.clone();
        let goal = goal.clone();
        async move { service.overlay_connected(&goal).await }
    });
    harness.wait_until_held().await;
    let push = harness.deliver(&goal, "newer");
    settle().await;
    harness.sink.release.notify_one();
    replay.await.unwrap();
    push.await.unwrap();

    assert_eq!(
        harness.sink.delivered_to(&goal).last().map(String::as_str),
        Some("newer"),
        "the replayed older content landed after the newer push: {:?}",
        harness.sink.delivered_to(&goal)
    );
}

#[tokio::test(start_paused = true)]
async fn a_push_held_on_one_overlay_does_not_hold_up_a_push_to_another() {
    let harness = harness(&[("goal-a", GOAL_KIND), ("goal-b", GOAL_KIND)]);
    let (held, other) = (OverlayId::new("goal-a"), OverlayId::new("goal-b"));

    let first = harness.deliver(&held, HELD);
    harness.wait_until_held().await;
    let delivered = tokio::time::timeout(
        WAIT_BOUND,
        harness
            .service
            .send_to(&other, &content("free"), &ArgStack::new(), None),
    )
    .await;
    harness.sink.release.notify_one();
    first.await.unwrap();

    assert!(
        delivered.is_ok(),
        "a push to one overlay waited on a push to a different overlay"
    );
}

#[tokio::test(start_paused = true)]
async fn pushes_to_an_overlay_that_retains_nothing_are_not_serialised() {
    let harness = harness(&[("sub-alert", ALERT_KIND)]);
    let alert = OverlayId::new("sub-alert");

    let first = harness.deliver(&alert, HELD);
    harness.wait_until_held().await;
    let delivered = tokio::time::timeout(
        WAIT_BOUND,
        harness
            .service
            .send_to(&alert, &content("free"), &ArgStack::new(), None),
    )
    .await;
    harness.sink.release.notify_one();
    first.await.unwrap();

    assert!(
        delivered.is_ok(),
        "a second alert waited behind the first although alerts retain nothing to keep in step"
    );
}
