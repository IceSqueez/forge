#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    FakeObs, RELOOKED_ITEM_ID, Recorder, Reply, SCENE, SCENE_UUID, SNAPSHOT_ITEM_ID, SOURCE, Seen,
    completes_every_handshake, connect_to, connect_with_timing, connected, event_frame,
    finishes_promptly, is_hidden, listed_scenes, listed_sources, page_scenes, page_sources,
    settle_until_async, spawn_fake_obs, standard_reply, the_first_two_handshakes_complete,
    version_data,
};
use forge_events::{Event, EventPublisher};
use forge_obs::{EVENT_BUFFER_CAPACITY, ObsClient, ObsError, ObsSink, ObsTiming};
use forge_platform_core::BuiltinControl;
use forge_test_support::settle_until;
use serde_json::{Value, json};
use tokio::sync::Notify;

const BEYOND_ANY_GUARD: Duration = Duration::from_secs(100 * 24 * 3600);
const BEYOND_THE_BUFFER: usize = 76;
const EVENT_BURST: usize = EVENT_BUFFER_CAPACITY + BEYOND_THE_BUFFER;

fn scene_item_enable_state_changed(enabled: bool) -> Value {
    event_frame(
        "SceneItemEnableStateChanged",
        json!({
            "sceneName": SCENE,
            "sceneUuid": SCENE_UUID,
            "sceneItemId": SNAPSHOT_ITEM_ID,
            "sceneItemEnabled": enabled,
        }),
    )
}

#[tokio::test]
async fn disconnecting_while_the_catalog_load_waits_on_obs_returns_promptly() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match (seen.request_type, seen.nth) {
            ("GetVersion", 0) => Reply::Answer(version_data()),
            _ => Reply::Silent,
        }),
    )
    .await;
    let recorder = Recorder::default();
    let timing = ObsTiming {
        request_timeout: BEYOND_ANY_GUARD,
        ..ObsTiming::default()
    };
    let client = connect_with_timing(&fake, &recorder, timing).await;
    settle_until("the catalog load asked OBS for its version", || {
        fake.log.count(0, "GetVersion") == 2
    })
    .await;

    let outcome = finishes_promptly(client.disconnect()).await;

    assert!(
        outcome.is_some(),
        "disconnect waited on the stalled catalog load"
    );
}

async fn drive_a_drop_during_the_catalog_load() -> (Recorder, Arc<ObsClient>, FakeObs) {
    let fake = spawn_fake_obs(
        the_first_two_handshakes_complete,
        Arc::new(|seen: Seen<'_>| match (seen.conn, seen.request_type) {
            (0, "SetCurrentProgramScene") | (1, "GetSceneList") => Reply::DropSocket,
            (_, "GetCurrentProgramScene") => {
                Reply::Answer(json!({ "sceneName": SCENE, "sceneUuid": SCENE_UUID }))
            }
            (_, other) => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;
    assert_eq!(listed_scenes(&client).await, [SCENE], "precondition");

    let lost = client.set_scene(SCENE).await;
    assert!(matches!(lost, Err(ObsError::NotConnected)), "got {lost:?}");
    settle_until("the supervisor dialled again after the drop", || {
        fake.log.accepted() >= 2
    })
    .await;
    settle_until("connection 1 reached its catalog load", || {
        fake.log.count(1, "GetSceneList") == 1
    })
    .await;
    settle_until("the supervisor dialled again after the failed load", || {
        fake.log.accepted() >= 3
    })
    .await;
    assert_eq!(
        fake.log.count(1, "GetSceneList"),
        1,
        "precondition: connection 1 died during its catalog load"
    );
    (recorder, client, fake)
}

#[tokio::test]
async fn a_connection_lost_during_the_catalog_load_is_retried_without_announcing_connected() {
    let (recorder, _client, _fake) = drive_a_drop_during_the_catalog_load().await;

    assert_eq!(recorder.states(), ["connected", "reconnecting"]);
}

#[tokio::test]
async fn a_connection_lost_during_the_catalog_load_keeps_the_last_good_catalog() {
    let (_recorder, client, _fake) = drive_a_drop_during_the_catalog_load().await;

    assert_eq!(page_scenes(&client), [SCENE]);
    assert_eq!(page_sources(&client).len(), 1);
}

#[tokio::test]
async fn a_scene_item_toggled_during_the_catalog_load_lands_in_the_catalog_and_on_the_bus() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(
            |seen: Seen<'_>| match (seen.conn, seen.request_type, seen.nth) {
                (0, "GetSceneItemEnabled", 0) => Reply::EventsFirst(
                    vec![scene_item_enable_state_changed(false)],
                    Box::new(Reply::Answer(json!({ "sceneItemEnabled": true }))),
                ),
                (_, other, _) => standard_reply(other),
            },
        ),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    settle_until("the toggle reached the bus", || {
        recorder
            .kinds()
            .iter()
            .any(|k| k == "obs.source.visibility_changed")
    })
    .await;

    let rows = listed_sources(&client, SCENE).await;
    assert!(
        rows.iter().any(|row| row.value == SOURCE && is_hidden(row)),
        "the catalog kept the snapshot's stale visibility: {rows:?}"
    );
}

fn scene_list(names: &[&str]) -> Reply {
    let scenes: Vec<Value> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            json!({ "sceneName": name, "sceneUuid": SCENE_UUID, "sceneIndex": index })
        })
        .collect();
    Reply::Answer(json!({ "scenes": scenes }))
}

async fn wait_for_scene(client: &ObsClient, scene: &str) {
    settle_until_async("the created scene appeared", || async {
        listed_scenes(client).await.iter().any(|s| s == scene)
    })
    .await;
}

#[tokio::test]
async fn a_scene_created_during_a_held_topology_fetch_survives_the_resync() {
    const INTRO: &str = "Intro";
    const LOADED: usize = 1;
    let release = Arc::new(Notify::new());
    let held = Arc::clone(&release);
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(move |seen: Seen<'_>| match seen.request_type {
            "GetSceneList" if seen.nth <= LOADED => scene_list(&[SCENE]),
            "GetSceneList" => scene_list(&[SCENE, INTRO]),
            "GetSceneItemList" if seen.nth == LOADED => Reply::EventsFirst(
                vec![event_frame(
                    "SceneCreated",
                    json!({ "sceneName": INTRO, "sceneUuid": SCENE_UUID, "isGroup": false }),
                )],
                Box::new(Reply::HeldUntil(
                    Arc::clone(&held),
                    Box::new(standard_reply("GetSceneItemList")),
                )),
            ),
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    forge_platform_core::BuiltinContent::request_refresh(&*client);
    wait_for_scene(&client, INTRO).await;
    release.notify_one();

    settle_until_async("a refetch replaced the stale fetch", || async {
        !listed_sources(&client, INTRO).await.is_empty()
    })
    .await;
    assert_eq!(listed_scenes(&client).await, vec![SCENE, INTRO]);
}

#[tokio::test]
async fn switching_the_scene_collection_makes_the_next_toggle_look_the_item_id_up_again() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| standard_reply(seen.request_type)),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    client
        .set_source_visible(SCENE, SOURCE, false)
        .await
        .unwrap();
    client.set_current_scene_collection("Collab").await.unwrap();
    client
        .set_source_visible(SCENE, SOURCE, true)
        .await
        .unwrap();

    let toggles: Vec<(String, Option<i64>)> = fake
        .log
        .requests_of(0, &["GetSceneItemId", "SetSceneItemEnabled"])
        .into_iter()
        .map(|(t, data)| (t, data["sceneItemId"].as_i64()))
        .collect();
    assert_eq!(
        toggles,
        [
            ("SetSceneItemEnabled".to_owned(), Some(SNAPSHOT_ITEM_ID)),
            ("GetSceneItemId".to_owned(), None),
            ("SetSceneItemEnabled".to_owned(), Some(RELOOKED_ITEM_ID)),
        ]
    );
}

struct StallingPublisher {
    recorder: Recorder,
    gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl EventPublisher for StallingPublisher {
    fn publish(&self, event: Event) {
        if event.kind == "obs.scene.changed" {
            let gate = self.gate.lock().unwrap().take();
            if let Some(gate) = gate {
                tokio::task::block_in_place(|| {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "recv returns once the sender is dropped"
                    )]
                    let _ = gate.recv();
                });
            }
        }
        self.recorder.publish(event);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_burst_that_overflows_the_buffer_resyncs_the_catalog_on_the_same_session() {
    let scene_changed = event_frame(
        "CurrentProgramSceneChanged",
        json!({ "sceneName": SCENE, "sceneUuid": SCENE_UUID }),
    );
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(move |seen: Seen<'_>| match seen.request_type {
            "GetSceneTransitionList" => Reply::EventsFirst(
                vec![scene_changed.clone(); EVENT_BURST],
                Box::new(Reply::Reject),
            ),
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let (release, gate) = std::sync::mpsc::channel();
    let publisher = Arc::new(StallingPublisher {
        recorder: recorder.clone(),
        gate: Mutex::new(Some(gate)),
    });
    let client = ObsClient::connect(
        &forge_obs::ObsEndpoint::parse(&format!("127.0.0.1:{}", fake.port), "").unwrap(),
        publisher,
    )
    .await
    .unwrap();
    connected(&client).await;
    assert_eq!(fake.log.count(0, "GetSceneItemEnabled"), 1, "precondition");

    let probe = forge_platform_core::BuiltinOptionSource::resolve(
        &client,
        &forge_platform_core::OptionListId::new("obs.transitions"),
        &std::collections::BTreeMap::new(),
    )
    .await;
    assert!(probe.is_err(), "precondition: the fake rejects the probe");
    drop(release);

    settle_until("the catalog was reloaded on the same session", || {
        fake.log.count(0, "GetSceneItemEnabled") >= 2
    })
    .await;
    assert!(
        !recorder
            .kinds()
            .iter()
            .any(|k| k == "obs.connection.disconnected"),
        "the overflow was announced as a lost connection"
    );
    assert_eq!(fake.log.accepted(), 1, "the overflow redialled OBS");
}
