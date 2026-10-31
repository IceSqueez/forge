#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use common::{
    FakeObs, Recorder, Reply, SCENE, SCENE_UUID, SNAPSHOT_ITEM_ID, SOURCE, Seen,
    completes_every_handshake, connect_to, connected, spawn_fake_obs, standard_reply,
};
use forge_obs::{ObsClient, ObsError, ObsSink, ObsSource};
use forge_types::Variant;
use serde_json::{Map, Value, json};

const INPUT: &str = "Mic";
const FILTER: &str = "Color";
const TRANSITION: &str = "Fade";
const PROFILE: &str = "Streaming";
const COLLECTION: &str = "Collab";
const CAPTION: &str = "hello chat";
const SCREENSHOT_PATH: &str = "/captures/shot.png";
const SCREENSHOT_FORMAT: &str = "png";
const RECORD_DIRECTORY: &str = "/captures";
const REFRESH_BUTTON: &str = "refreshnocache";
const MEDIA_RESTART: &str = "OBS_WEBSOCKET_MEDIA_INPUT_ACTION_RESTART";
const VOLUME_DB: f64 = -6.5;
const RECORDED_MS: i64 = 1500;
const STREAMED_MS: i64 = 2500;
const OBS_STUDIO_MODE_NOT_ACTIVE: u16 = 506;

type Sent<'a, T> = Pin<Box<dyn Future<Output = Result<T, ObsError>> + Send + 'a>>;
type Command = for<'a> fn(&'a ObsClient) -> Sent<'a, ()>;
type Query = for<'a> fn(&'a ObsClient) -> Sent<'a, Variant>;

fn zero_output_counters() -> Value {
    json!({ "outputBytes": 0, "outputTimecode": "00:00:00.000" })
}

fn with_output_counters(mut fields: Value) -> Value {
    let counters = zero_output_counters();
    for (key, value) in counters.as_object().unwrap() {
        fields[key] = value.clone();
    }
    fields
}

fn answers(request_type: &str) -> Reply {
    match request_type {
        "StopRecord" => Reply::Answer(json!({ "outputPath": "/captures/run.mkv" })),
        "ToggleRecordPause" => Reply::Answer(json!({ "outputPaused": true })),
        "GetSceneList" => Reply::Answer(json!({
            "currentProgramSceneName": SCENE,
            "currentProgramSceneUuid": SCENE_UUID,
            "scenes": [{ "sceneName": SCENE, "sceneUuid": SCENE_UUID, "sceneIndex": 0 }],
        })),
        "GetCurrentProgramScene" => Reply::Answer(json!({
            "sceneName": SCENE,
            "sceneUuid": SCENE_UUID,
        })),
        "GetInputList" => Reply::Answer(json!({
            "inputs": [{
                "inputName": INPUT,
                "inputUuid": SCENE_UUID,
                "inputKind": "pulse_input_capture",
                "unversionedInputKind": "pulse_input_capture",
            }],
        })),
        "GetRecordStatus" => Reply::Answer(with_output_counters(json!({
            "outputActive": true,
            "outputPaused": false,
            "outputDuration": RECORDED_MS,
        }))),
        "GetStreamStatus" => Reply::Answer(with_output_counters(json!({
            "outputActive": true,
            "outputReconnecting": false,
            "outputDuration": STREAMED_MS,
            "outputCongestion": 0.0,
            "outputSkippedFrames": 0,
            "outputTotalFrames": 0,
        }))),
        "GetInputSettings" => Reply::Answer(json!({
            "inputKind": "browser_source",
            "inputSettings": {
                "url": null,
                "width": 1920,
                "tagged": { "type": "Int", "value": 7 },
            },
        })),
        other if other.starts_with("Get") => standard_reply(other),
        _ => Reply::Answer(Value::Null),
    }
}

async fn session_answering(script: fn(Seen<'_>) -> Reply) -> (FakeObs, Arc<ObsClient>) {
    let fake = spawn_fake_obs(completes_every_handshake, Arc::new(script)).await;
    let client = connect_to(&fake, &Recorder::default()).await;
    connected(&client).await;
    (fake, client)
}

async fn answering_session() -> (FakeObs, Arc<ObsClient>) {
    session_answering(|seen| answers(seen.request_type)).await
}

async fn sole_request_data<T>(
    fake: &FakeObs,
    request_type: &str,
    work: impl Future<Output = Result<T, ObsError>>,
) -> (T, Value) {
    let before = fake.log.count(0, request_type);
    let outcome = work
        .await
        .unwrap_or_else(|error| panic!("{request_type} failed: {error:?}"));
    let requests = fake.log.requests_of(0, &[request_type]);
    let [(_, data)] = &requests[before..] else {
        panic!("{request_type} went out {} times", requests.len() - before);
    };
    (outcome, data.clone())
}

fn settings_with_null_and_a_type_value_object() -> Map<String, Value> {
    let Value::Object(settings) = json!({
        "url": null,
        "tagged": { "type": "Int", "value": 7 },
        "nested": { "keep": null, "items": [1, null] },
    }) else {
        unreachable!("the literal is an object");
    };
    settings
}

fn command_table() -> Vec<(Command, &'static str, Value)> {
    vec![
        (
            |c| c.set_scene(SCENE),
            "SetCurrentProgramScene",
            json!({ "sceneName": SCENE }),
        ),
        (
            |c| c.set_preview_scene(SCENE),
            "SetCurrentPreviewScene",
            json!({ "sceneName": SCENE }),
        ),
        (
            |c| c.set_source_visible(SCENE, SOURCE, false),
            "SetSceneItemEnabled",
            json!({
                "sceneName": SCENE,
                "sceneItemId": SNAPSHOT_ITEM_ID,
                "sceneItemEnabled": false,
            }),
        ),
        (
            |c| c.set_source_locked(SCENE, SOURCE, true),
            "SetSceneItemLocked",
            json!({
                "sceneName": SCENE,
                "sceneItemId": SNAPSHOT_ITEM_ID,
                "sceneItemLocked": true,
            }),
        ),
        (
            |c| c.set_input_mute(INPUT, true),
            "SetInputMute",
            json!({ "inputName": INPUT, "inputMuted": true }),
        ),
        (
            |c| c.set_input_volume_db(INPUT, VOLUME_DB),
            "SetInputVolume",
            json!({ "inputName": INPUT, "inputVolumeDb": VOLUME_DB }),
        ),
        (
            |c| c.set_current_scene_transition(TRANSITION),
            "SetCurrentSceneTransition",
            json!({ "transitionName": TRANSITION }),
        ),
        (
            |c| c.send_stream_caption(CAPTION),
            "SendStreamCaption",
            json!({ "captionText": CAPTION }),
        ),
        (
            |c| c.set_studio_mode(true),
            "SetStudioModeEnabled",
            json!({ "studioModeEnabled": true }),
        ),
        (
            |c| c.set_source_filter_enabled(SOURCE, FILTER, true),
            "SetSourceFilterEnabled",
            json!({ "sourceName": SOURCE, "filterName": FILTER, "filterEnabled": true }),
        ),
        (
            |c| c.refresh_browser_source(INPUT),
            "PressInputPropertiesButton",
            json!({ "inputName": INPUT, "propertyName": REFRESH_BUTTON }),
        ),
        (
            |c| c.restart_media_input(INPUT),
            "TriggerMediaInputAction",
            json!({ "inputName": INPUT, "mediaAction": MEDIA_RESTART }),
        ),
        (
            |c| c.save_source_screenshot(SOURCE, SCREENSHOT_PATH, SCREENSHOT_FORMAT),
            "SaveSourceScreenshot",
            json!({
                "sourceName": SOURCE,
                "imageFormat": SCREENSHOT_FORMAT,
                "imageFilePath": SCREENSHOT_PATH,
            }),
        ),
        (
            |c| c.set_record_directory(RECORD_DIRECTORY),
            "SetRecordDirectory",
            json!({ "recordDirectory": RECORD_DIRECTORY }),
        ),
        (
            |c| c.set_current_profile(PROFILE),
            "SetCurrentProfile",
            json!({ "profileName": PROFILE }),
        ),
        (
            |c| c.set_current_scene_collection(COLLECTION),
            "SetCurrentSceneCollection",
            json!({ "sceneCollectionName": COLLECTION }),
        ),
        (|c| c.start_record(), "StartRecord", Value::Null),
        (|c| c.stop_record(), "StopRecord", Value::Null),
        (|c| c.pause_record(), "PauseRecord", Value::Null),
        (|c| c.resume_record(), "ResumeRecord", Value::Null),
        (
            |c| c.toggle_record_pause(),
            "ToggleRecordPause",
            Value::Null,
        ),
        (|c| c.start_stream(), "StartStream", Value::Null),
        (|c| c.stop_stream(), "StopStream", Value::Null),
        (
            |c| c.start_replay_buffer(),
            "StartReplayBuffer",
            Value::Null,
        ),
        (|c| c.stop_replay_buffer(), "StopReplayBuffer", Value::Null),
        (|c| c.save_replay_buffer(), "SaveReplayBuffer", Value::Null),
        (
            |c| c.trigger_studio_transition(),
            "TriggerStudioModeTransition",
            Value::Null,
        ),
        (|c| c.start_virtual_cam(), "StartVirtualCam", Value::Null),
        (|c| c.stop_virtual_cam(), "StopVirtualCam", Value::Null),
    ]
}

#[tokio::test]
async fn each_command_sends_its_request_type_with_exactly_the_documented_data() {
    let (fake, client) = answering_session().await;

    for (send, request_type, expected_data) in command_table() {
        let ((), sent) = sole_request_data(&fake, request_type, send(&client)).await;
        assert_eq!(sent, expected_data, "{request_type}");
    }
}

fn object(fields: impl IntoIterator<Item = (&'static str, Variant)>) -> Variant {
    Variant::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn names(values: Vec<String>) -> Variant {
    Variant::Array(values.into_iter().map(Variant::String).collect())
}

fn query_table() -> Vec<(Query, &'static str, Value, Variant)> {
    vec![
        (
            |c| {
                Box::pin(async move {
                    let scenes = c.get_scene_list().await?;
                    Ok(object([
                        ("all_names", names(scenes.names)),
                        ("current", Variant::String(scenes.current)),
                    ]))
                })
            },
            "GetSceneList",
            Value::Null,
            object([
                ("all_names", Variant::Array(vec![text(SCENE)])),
                ("current", text(SCENE)),
            ]),
        ),
        (
            |c| {
                Box::pin(
                    async move { Ok(object([("all_names", names(c.get_input_list().await?))])) },
                )
            },
            "GetInputList",
            json!({}),
            object([("all_names", Variant::Array(vec![text(INPUT)]))]),
        ),
        (
            |c| {
                Box::pin(async move {
                    let status = c.get_record_status().await?;
                    Ok(object([
                        ("is_active", Variant::Bool(status.active)),
                        ("is_paused", Variant::Bool(status.paused)),
                        ("duration_ms", Variant::Int(status.duration_ms)),
                    ]))
                })
            },
            "GetRecordStatus",
            Value::Null,
            object([
                ("is_active", Variant::Bool(true)),
                ("is_paused", Variant::Bool(false)),
                ("duration_ms", Variant::Int(RECORDED_MS)),
            ]),
        ),
        (
            |c| {
                Box::pin(async move {
                    let status = c.get_stream_status().await?;
                    Ok(object([
                        ("is_active", Variant::Bool(status.active)),
                        ("duration_ms", Variant::Int(status.duration_ms)),
                    ]))
                })
            },
            "GetStreamStatus",
            Value::Null,
            object([
                ("is_active", Variant::Bool(true)),
                ("duration_ms", Variant::Int(STREAMED_MS)),
            ]),
        ),
        (
            |c| {
                Box::pin(async move {
                    let scene = c.get_current_scene().await?;
                    Ok(Variant::String(scene.unwrap_or_default()))
                })
            },
            "GetCurrentProgramScene",
            Value::Null,
            text(SCENE),
        ),
        (
            |c| {
                Box::pin(async move {
                    let input = c.get_input_settings(INPUT).await?;
                    Ok(object([
                        ("settings", input.settings),
                        ("kind", Variant::String(input.kind)),
                    ]))
                })
            },
            "GetInputSettings",
            json!({ "inputName": INPUT }),
            object([
                (
                    "settings",
                    object([
                        ("width", Variant::Int(1920)),
                        (
                            "tagged",
                            object([("type", text("Int")), ("value", Variant::Int(7))]),
                        ),
                    ]),
                ),
                ("kind", text("browser_source")),
            ]),
        ),
    ]
}

#[tokio::test]
async fn each_query_sends_its_request_and_reads_the_answer_into_a_variant() {
    let (fake, client) = answering_session().await;

    for (send, request_type, expected_data, expected_result) in query_table() {
        let (result, sent) = sole_request_data(&fake, request_type, send(&client)).await;
        assert_eq!(sent, expected_data, "{request_type}");
        assert_eq!(result, expected_result, "{request_type}");
    }
}

#[tokio::test]
async fn input_settings_reach_obs_verbatim_including_nulls_and_type_value_objects() {
    let settings = settings_with_null_and_a_type_value_object();
    let (fake, client) = answering_session().await;

    for overlay in [true, false] {
        let work = client.set_input_settings(INPUT, &settings, overlay);
        let ((), sent) = sole_request_data(&fake, "SetInputSettings", work).await;
        assert_eq!(
            sent,
            json!({ "inputName": INPUT, "inputSettings": settings, "overlay": overlay }),
        );
    }
}

#[tokio::test]
async fn a_toggle_of_an_item_missing_from_the_catalog_looks_its_id_up_once() {
    const UNCACHED_SOURCE: &str = "Overlay";
    let (fake, client) = answering_session().await;

    for visible in [true, false] {
        client
            .set_source_visible(SCENE, UNCACHED_SOURCE, visible)
            .await
            .unwrap();
    }

    let lookups = fake.log.requests_of(0, &["GetSceneItemId"]);
    assert_eq!(
        lookups,
        [(
            "GetSceneItemId".to_owned(),
            json!({ "sceneName": SCENE, "sourceName": UNCACHED_SOURCE }),
        )]
    );
}

#[tokio::test]
async fn a_volume_change_obs_accepted_lands_in_the_catalog() {
    let script = |seen: Seen<'_>| match seen.request_type {
        "GetSceneItemList" => standard_reply("GetSceneItemList"),
        other => answers(other),
    };
    let (_fake, client) = session_answering(script).await;

    client.set_input_volume_db(SOURCE, VOLUME_DB).await.unwrap();

    let rows = client.sources(SCENE).await.unwrap();
    assert_eq!(rows[0].audio_db, Some(VOLUME_DB as f32));
}

fn rejecting(seen: Seen<'_>) -> Reply {
    match seen.request_type {
        "SetCurrentProgramScene"
        | "SetSceneItemEnabled"
        | "GetSceneItemId"
        | "SetInputVolume"
        | "SetInputSettings"
        | "GetRecordStatus"
        | "GetInputSettings"
        | "SetCurrentProfile"
        | "SetCurrentSceneCollection" => Reply::Reject,
        "SetCurrentPreviewScene" | "TriggerStudioModeTransition" => {
            Reply::RejectWith(OBS_STUDIO_MODE_NOT_ACTIVE)
        }
        other => answers(other),
    }
}

fn rejected_table() -> Vec<(Command, &'static str)> {
    vec![
        (|c| c.set_scene(SCENE), "SetCurrentProgramScene"),
        (
            |c| c.set_source_visible(SCENE, SOURCE, true),
            "SetSceneItemEnabled",
        ),
        (
            |c| c.set_source_visible(SCENE, "Overlay", true),
            "GetSceneItemId",
        ),
        (
            |c| c.set_input_volume_db(INPUT, VOLUME_DB),
            "SetInputVolume",
        ),
        (|c| c.set_current_profile(PROFILE), "SetCurrentProfile"),
        (
            |c| c.set_current_scene_collection(COLLECTION),
            "SetCurrentSceneCollection",
        ),
        (
            |c| Box::pin(async move { c.get_record_status().await.map(|_| ()) }),
            "GetRecordStatus",
        ),
        (
            |c| Box::pin(async move { c.get_input_settings(INPUT).await.map(|_| ()) }),
            "GetInputSettings",
        ),
    ]
}

#[tokio::test]
async fn a_rejected_request_names_the_failing_request_in_each_family() {
    let (_fake, client) = session_answering(rejecting).await;

    for (send, expected_request) in rejected_table() {
        let outcome = send(&client).await;
        assert!(
            matches!(
                &outcome,
                Err(ObsError::Request { request_type, .. }) if request_type == expected_request
            ),
            "{expected_request} produced {outcome:?}"
        );
    }
}

#[tokio::test]
async fn studio_mode_rejections_map_to_the_studio_mode_error() {
    let (_fake, client) = session_answering(rejecting).await;

    let preview = client.set_preview_scene(SCENE).await;
    let transition = client.trigger_studio_transition().await;

    assert!(
        matches!(preview, Err(ObsError::StudioModeInactive)),
        "preview produced {preview:?}"
    );
    assert!(
        matches!(transition, Err(ObsError::StudioModeInactive)),
        "transition produced {transition:?}"
    );
}

#[tokio::test]
async fn a_rejected_volume_change_leaves_the_catalog_alone() {
    let (_fake, client) = session_answering(rejecting).await;
    let before = client.sources(SCENE).await.unwrap()[0].audio_db;

    let outcome = client.set_input_volume_db(SOURCE, VOLUME_DB).await;

    assert!(outcome.is_err());
    assert_eq!(client.sources(SCENE).await.unwrap()[0].audio_db, before);
}

#[tokio::test]
async fn a_rejected_scene_collection_switch_still_forgets_the_cached_item_ids() {
    let script = |seen: Seen<'_>| match seen.request_type {
        "SetCurrentSceneCollection" => Reply::Reject,
        other => answers(other),
    };
    let (fake, client) = session_answering(script).await;

    let switched = client.set_current_scene_collection(COLLECTION).await;
    assert!(switched.is_err());
    client
        .set_source_visible(SCENE, SOURCE, true)
        .await
        .unwrap();

    assert_eq!(fake.log.count(0, "GetSceneItemId"), 1);
}

#[tokio::test]
async fn input_settings_that_are_not_an_object_surface_as_a_protocol_error() {
    let script = |seen: Seen<'_>| match seen.request_type {
        "GetInputSettings" => Reply::Answer(json!({
            "inputKind": "browser_source",
            "inputSettings": [1, 2],
        })),
        other => answers(other),
    };
    let (_fake, client) = session_answering(script).await;

    let outcome = client.get_input_settings(INPUT).await;

    assert!(
        matches!(outcome, Err(ObsError::Protocol(_))),
        "got {outcome:?}"
    );
}
