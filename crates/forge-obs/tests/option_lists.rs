mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use common::{
    FakeObs, Recorder, Reply, SCENE_UUID, Seen, completes_every_handshake, connect_to, event_frame,
    settle_until, settle_until_async, spawn_fake_obs, standard_reply,
};
use forge_obs::vocabulary::event_kinds::AUDIO_SOURCE_MUTE_CHANGED;
use forge_obs::{ObsClient, ObsSink};
use forge_platform_core::{
    BuiltinOptionSource, CollectionOutcome, CollectionRevisions, OptionEntry, OptionListId,
    RevisionWait,
};
use futures_util::FutureExt as _;
use serde_json::{Value, json};

const SCENE_NAMES: &str = "obs.scene_names";
const SOURCE_NAMES: &str = "obs.source_names";
const INPUT_NAMES: &str = "obs.input_names";
const AUDIO_INPUTS: &str = "obs.audio_inputs";
const AUDIO_INPUT_NAMES: &str = "obs.audio_input_names";
const HANG_GUARD: Duration = Duration::from_secs(90);

const STUDIO_SCENES: [&str; 3] = ["Main", "Broken", "Outro"];

fn listed_scene(seen: Seen<'_>) -> &'static str {
    STUDIO_SCENES[seen.nth % STUDIO_SCENES.len()]
}

fn scene_list() -> Reply {
    let scenes: Vec<Value> = STUDIO_SCENES
        .iter()
        .enumerate()
        .map(|(index, name)| {
            json!({ "sceneName": name, "sceneUuid": SCENE_UUID, "sceneIndex": index })
        })
        .collect();
    Reply::Answer(json!({ "scenes": scenes }))
}

fn camera_of(scene: &str) -> Reply {
    Reply::Answer(json!({
        "sceneItems": [{
            "sceneItemId": 1,
            "sceneItemIndex": 0,
            "sourceName": format!("{scene} Camera"),
            "sourceType": "OBS_SOURCE_TYPE_INPUT",
            "inputKind": "browser_source",
            "isGroup": null,
        }],
    }))
}

fn audio_inputs() -> Reply {
    let inputs: Vec<Value> = ["Mic", "Desktop Audio"]
        .iter()
        .map(|name| {
            json!({
                "inputName": name,
                "inputUuid": SCENE_UUID,
                "inputKind": "pulse_input_capture",
                "unversionedInputKind": "pulse_input_capture",
            })
        })
        .collect();
    Reply::Answer(json!({ "inputs": inputs }))
}

fn healthy_studio(seen: Seen<'_>) -> Reply {
    match seen.request_type {
        "GetSceneList" => scene_list(),
        "GetInputList" => audio_inputs(),
        "GetSceneItemList" => camera_of(listed_scene(seen)),
        other => standard_reply(other),
    }
}

async fn resolved(client: &ObsClient, list: &str) -> CollectionOutcome<Vec<OptionEntry>> {
    client
        .resolve(&OptionListId::new(list), &BTreeMap::new())
        .await
}

async fn names(client: &ObsClient, list: &str) -> Vec<String> {
    resolved(client, list)
        .await
        .map(|entries| entries.into_iter().map(|entry| entry.value).collect())
        .unwrap_or_default()
}

async fn catalog_reached(client: &ObsClient, ready_list: &str) {
    settle_until_async("the catalog reached the option source", || async {
        !names(client, ready_list).await.is_empty()
    })
    .await;
}

async fn loaded_studio(
    script: fn(Seen<'_>) -> Reply,
    ready_list: &str,
) -> (FakeObs, Arc<ObsClient>) {
    let fake = spawn_fake_obs(completes_every_handshake, Arc::new(script)).await;
    let client = connect_to(&fake, &Recorder::default()).await;
    catalog_reached(&client, ready_list).await;
    (fake, client)
}

#[tokio::test]
async fn every_editor_list_of_a_connected_client_resolves_to_its_own_names() {
    let (_fake, client) = loaded_studio(healthy_studio, SCENE_NAMES).await;

    for (list, expected) in [
        (SCENE_NAMES, vec!["Main", "Broken", "Outro"]),
        (
            SOURCE_NAMES,
            vec!["Main Camera", "Broken Camera", "Outro Camera"],
        ),
        (
            INPUT_NAMES,
            vec![
                "Main Camera",
                "Broken Camera",
                "Outro Camera",
                "Mic",
                "Desktop Audio",
            ],
        ),
        (AUDIO_INPUTS, vec!["Mic", "Desktop Audio"]),
        (AUDIO_INPUT_NAMES, vec!["Mic", "Desktop Audio"]),
    ] {
        assert_eq!(names(&client, list).await, expected, "{list}");
    }
}

#[tokio::test]
async fn a_rejected_scene_list_still_offers_the_audio_inputs() {
    let (_fake, client) = loaded_studio(
        |seen| match seen.request_type {
            "GetSceneList" => Reply::Reject,
            _ => healthy_studio(seen),
        },
        AUDIO_INPUT_NAMES,
    )
    .await;

    assert_eq!(
        (
            resolved(&client, SCENE_NAMES).await,
            names(&client, AUDIO_INPUT_NAMES).await,
        ),
        (
            Ok(Vec::new()),
            vec!["Mic".to_owned(), "Desktop Audio".to_owned()]
        )
    );
}

#[tokio::test]
async fn a_rejected_audio_input_list_still_offers_the_scenes() {
    let (_fake, client) = loaded_studio(
        |seen| match seen.request_type {
            "GetInputList" => Reply::Reject,
            _ => healthy_studio(seen),
        },
        SCENE_NAMES,
    )
    .await;

    assert_eq!(
        (
            names(&client, SCENE_NAMES).await,
            resolved(&client, AUDIO_INPUT_NAMES).await,
        ),
        (
            vec!["Main".to_owned(), "Broken".to_owned(), "Outro".to_owned()],
            Ok(Vec::new()),
        )
    );
}

#[tokio::test]
async fn a_scene_whose_items_are_rejected_still_leaves_the_other_scenes_items() {
    let (_fake, client) = loaded_studio(
        |seen| match seen.request_type {
            "GetSceneItemList" if listed_scene(seen) == "Broken" => Reply::Reject,
            _ => healthy_studio(seen),
        },
        SOURCE_NAMES,
    )
    .await;

    assert_eq!(
        names(&client, SOURCE_NAMES).await,
        ["Main Camera", "Outro Camera"]
    );
}

fn moved(revisions: &mut CollectionRevisions) -> bool {
    revisions.changed().now_or_never() == Some(RevisionWait::Changed)
}

fn mute_changed() -> Value {
    event_frame(
        "InputMuteStateChanged",
        json!({ "inputName": "Mic", "inputUuid": SCENE_UUID, "inputMuted": true }),
    )
}

fn scene_created() -> Value {
    event_frame(
        "SceneCreated",
        json!({ "sceneName": "Created", "sceneUuid": SCENE_UUID, "isGroup": false }),
    )
}

const SCENE_SWITCH_THAT_CREATES_A_SCENE: usize = 2;

fn events_on_each_scene_switch(seen: Seen<'_>) -> Reply {
    if seen.request_type != "SetCurrentProgramScene" {
        return healthy_studio(seen);
    }
    let event = if seen.nth == SCENE_SWITCH_THAT_CREATES_A_SCENE {
        scene_created()
    } else {
        mute_changed()
    };
    Reply::EventsFirst(vec![event], Box::new(Reply::Answer(Value::Null)))
}

async fn wait_for_mutes(recorder: &Recorder, count: usize) {
    settle_until("the mute events reached the bus", || {
        recorder
            .kinds()
            .iter()
            .filter(|kind| *kind == AUDIO_SOURCE_MUTE_CHANGED)
            .count()
            >= count
    })
    .await;
}

#[tokio::test]
async fn a_scene_created_moves_the_option_revision_and_a_mute_toggle_does_not() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(events_on_each_scene_switch),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    catalog_reached(&client, SCENE_NAMES).await;
    client.set_scene("Main").await.unwrap();
    wait_for_mutes(&recorder, 1).await;
    let mut revisions = client.revisions();

    client.set_scene("Main").await.unwrap();
    wait_for_mutes(&recorder, 2).await;
    let moved_on_mute = moved(&mut revisions);
    client.set_scene("Main").await.unwrap();
    let on_scene_created = tokio::time::timeout(HANG_GUARD, revisions.changed()).await;

    assert_eq!(
        (
            moved_on_mute,
            on_scene_created.ok(),
            names(&client, SCENE_NAMES).await
        ),
        (
            false,
            Some(RevisionWait::Changed),
            vec![
                "Main".to_owned(),
                "Broken".to_owned(),
                "Outro".to_owned(),
                "Created".to_owned(),
            ],
        )
    );
}
