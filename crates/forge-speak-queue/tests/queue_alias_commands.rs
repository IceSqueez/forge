//! Resolver-mutating commands (`SetAlias`, `SwitchAlias`, `SetStrategy`,
//! `RemoveAlias`) against the live `VoiceAliasResolver`. Each test enqueues a
//! speak for the affected viewer and asserts the voice/engine the actor resolved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use forge_speak_queue::{QueueConfig, SpeakCommand};
use forge_tts_core::{EngineId, VoiceId};
use forge_voice::{AliasId, AssignmentStrategy};

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
    // created an alias for v2, resolution would flip to beta-1 - so alpha-1 proves no-op.
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

#[tokio::test]
async fn set_strategy_repoints_live_fallback_for_aliasless_viewers() {
    let (sink, _plays) = recording_sink();
    // Fallback starts pinned to alpha-1 for every alias-less viewer; SetStrategy must
    // swap the live resolver's strategy so a later speak resolves through beta-1.
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
        .send(SpeakCommand::SetStrategy(AssignmentStrategy::Single {
            voice_id: VoiceId("beta-1".into()),
            engine_id: EngineId("beta".into()),
        }))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v1", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!(
        (voice.as_str(), engine.as_str()),
        ("beta-1", "beta"),
        "SetStrategy must repoint the live resolver's fallback strategy",
    );
}

#[tokio::test]
async fn remove_alias_drops_matching_alias_so_fallback_resolves() {
    let (sink, _plays) = recording_sink();
    let target = alias("v1", "beta", "beta-1");
    let target_id = target.id.clone();
    // v1 has an explicit beta-1 alias; fallback pins alias-less viewers to alpha-1.
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::Single {
            voice_id: VoiceId("alpha-1".into()),
            engine_id: EngineId("alpha".into()),
        },
        vec![target],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::RemoveAlias(target_id))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v1", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!(
        (voice.as_str(), engine.as_str()),
        ("alpha-1", "alpha"),
        "after RemoveAlias, v1 must fall back to the strategy voice",
    );
}

#[tokio::test]
async fn remove_alias_with_absent_id_leaves_existing_alias_intact() {
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::Single {
            voice_id: VoiceId("alpha-1".into()),
            engine_id: EngineId("alpha".into()),
        },
        vec![alias("v1", "beta", "beta-1")],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    // A fresh, unrelated id must not disturb v1's real alias.
    handle
        .send(SpeakCommand::RemoveAlias(AliasId::new()))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Enqueue(request("v1", "hello")))
        .await
        .unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!(
        (voice.as_str(), engine.as_str()),
        ("beta-1", "beta"),
        "RemoveAlias with an absent id must be a no-op",
    );
}
