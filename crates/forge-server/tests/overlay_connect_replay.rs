#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test fixtures"
)]

use std::sync::Arc;
use std::time::Duration;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_runtime::{
    EventBus, NullEventLogRepo, OverlayFrameSink, OverlayServiceHandle, OverlayServiceParts,
};
use forge_server::test_support::ServerDeps;
use forge_server::{DeliveryHub, ServerHandle, start_server};
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo,
};
use forge_types::{OverlayCredential, Variant};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use time::OffsetDateTime;
use tokio_tungstenite::tungstenite::Message;

const GOAL_ID: &str = "goal-box";
const GOAL_CREDENTIAL: &str = "2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d";
const RETAINED_VALUE: &str = "42 of 100";
const HANG_GUARD: Duration = Duration::from_secs(30);

type Page =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn goal_overlay() -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(GOAL_ID),
        display_name: "Sub goal".to_owned(),
        kind_id: "overlay.goal".to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::new(),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new(GOAL_CREDENTIAL),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn retained_content() -> OverlayConfig {
    OverlayConfig::from([(
        "value".to_owned(),
        Variant::String(RETAINED_VALUE.to_owned()),
    )])
}

fn overlay_service(hub: &DeliveryHub, tasks: &forge_net::TaskOwner) -> OverlayServiceHandle {
    let mut repo = MockOverlayRepo::new();
    repo.expect_get()
        .returning(|id| Ok((id.as_str() == GOAL_ID).then(goal_overlay)));
    repo.expect_get_retained_content()
        .returning(|id| Ok((id.as_str() == GOAL_ID).then(retained_content)));
    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).unwrap();
    OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: tasks.clone(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: Arc::new(kinds),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: Arc::new(hub.clone()) as Arc<dyn OverlayFrameSink>,
        media: None,
        speaker: None,
        latest: None,
    })
}

struct Running {
    handle: ServerHandle,
    hub: DeliveryHub,
    url: String,
    _tasks: forge_net::TaskOwner,
    _root: tempfile::TempDir,
}

async fn server_replaying_through_the_runtime() -> Running {
    let root = tempfile::tempdir().unwrap();
    let hub = DeliveryHub::new(EventBus::new(Arc::new(NullEventLogRepo)));
    let tasks = forge_net::TaskOwner::new();
    hub.set_overlay_connect_listener(Arc::new(overlay_service(&hub, &tasks)));

    let mut pages = MockOverlayRepo::new();
    pages
        .expect_get_by_credential()
        .returning(|credential| Ok((credential.expose() == GOAL_CREDENTIAL).then(goal_overlay)));
    let mut config = ServerDeps::new(root.path())
        .with_overlays(Arc::new(pages))
        .build();
    config.delivery_hub = Some(hub.clone());
    let handle = start_server(config).await.expect("the server starts");
    let url = format!("ws://{}/ws/v1/", handle.bind_addr().await);
    Running {
        handle,
        hub,
        url,
        _tasks: tasks,
        _root: root,
    }
}

async fn next_json(page: &mut Page) -> Value {
    let message = tokio::time::timeout(HANG_GUARD, page.next())
        .await
        .expect("a frame arrived within the hang guard")
        .expect("the socket stayed open")
        .expect("no websocket error");
    match message {
        Message::Text(text) => serde_json::from_str(&text).expect("a json frame"),
        other => panic!("the page received a non-text frame: {other:?}"),
    }
}

async fn connect_page(url: &str) -> (Page, Value) {
    let (mut page, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("ws connect");
    page.send(Message::Text(
        serde_json::json!({
            "id": "1",
            "request": "auth",
            "overlayCredential": GOAL_CREDENTIAL,
        })
        .to_string()
        .into(),
    ))
    .await
    .expect("the page sends its credential");
    let mut replay = None;
    let mut acknowledged = false;
    while !acknowledged || replay.is_none() {
        let frame = next_json(&mut page).await;
        if frame.get("frame").is_some() {
            replay = Some(frame);
        } else {
            assert_eq!(frame["status"], "ok", "the page was refused: {frame}");
            acknowledged = true;
        }
    }
    (
        page,
        replay.expect("the loop exits only once the replay arrived"),
    )
}

#[tokio::test]
async fn a_connecting_page_receives_its_replay_as_a_plain_content_frame() {
    let running = server_replaying_through_the_runtime().await;

    let (_page, replay) = connect_page(&running.url).await;

    assert_eq!(
        replay,
        serde_json::json!({
            "frame": "content",
            "content": { "value": RETAINED_VALUE },
        })
    );
    running.handle.stop().await.expect("stop");
}

#[tokio::test]
async fn a_second_page_connecting_to_the_same_overlay_does_not_replay_to_the_first_page() {
    let running = server_replaying_through_the_runtime().await;
    let (mut first, _first_replay) = connect_page(&running.url).await;

    let (_second, _second_replay) = connect_page(&running.url).await;
    running
        .hub
        .deliver_content(
            &OverlayId::new(GOAL_ID),
            serde_json::json!({ "value": "after both connected" }),
            None,
        )
        .await;

    assert_eq!(
        next_json(&mut first).await["content"]["value"],
        "after both connected",
        "the first page was re-animated by the second page's connect replay"
    );
    running.handle.stop().await.expect("stop");
}
