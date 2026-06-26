//! `SetAlias` (insert / replace by viewer_id) and `SwitchAlias` (repoint existing /
//! no-op when absent) against the live `VoiceAliasResolver`. Each test enqueues a
//! speak for the affected viewer and asserts the voice/engine the actor resolved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use forge_speak_queue::{QueueConfig, SpeakCommand};
use forge_tts_core::{EngineId, VoiceId};
use forge_voice::AssignmentStrategy;

use common::{
    alias, make_deps, recording_sink, request, standard_registry, wait_for_resolved_voice,
};

#[tokio::test]
async fn set_alias_inserts_alias_that_subsequent_speak_resolves_through() {
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::SetAlias(alias("v1", "beta", "beta-1")))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v1", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!((voice.as_str(), engine.as_str()), ("beta-1", "beta"));
}

#[tokio::test]
async fn set_alias_replaces_existing_alias_for_same_viewer_id() {
    let (sink, _plays) = recording_sink();
    // Seed an existing alias for v1 → alpha-1; SetAlias for v1 again must REPLACE it
    // (not append a duplicate), so resolution follows the second alias.
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![alias("v1", "alpha", "alpha-1")],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::SetAlias(alias("v1", "beta", "beta-1")))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v1", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!((voice.as_str(), engine.as_str()), ("beta-1", "beta"));
}

#[tokio::test]
async fn switch_alias_repoints_existing_viewer_alias() {
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![alias("v1", "alpha", "alpha-1")],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::SwitchAlias {
            viewer_id: "v1".into(),
            engine_id: EngineId("beta".into()),
            voice_id: VoiceId("beta-1".into()),
        })
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v1", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!((voice.as_str(), engine.as_str()), ("beta-1", "beta"));
}

#[tokio::test]
async fn switch_alias_is_noop_when_viewer_has_no_alias() {
    let (sink, _plays) = recording_sink();
    // `Single` strategy pins every alias-less viewer to alpha-1. If SwitchAlias wrongly
    // created an alias for v2, resolution would flip to beta-1 — so alpha-1 proves no-op.
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::Single {
            voice_id: VoiceId("alpha-1".into()),
            engine_id: EngineId("alpha".into()),
        },
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::SwitchAlias {
            viewer_id: "v2".into(),
            engine_id: EngineId("beta".into()),
            voice_id: VoiceId("beta-1".into()),
        })
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v2", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!(
        (voice.as_str(), engine.as_str()),
        ("alpha-1", "alpha"),
        "SwitchAlias must not create an alias for a viewer that had none",
    );
}
