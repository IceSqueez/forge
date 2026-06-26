//! `resolve_with_overrides`: `voice_override` / `engine_override` bypass and
//! precedence. Driven end-to-end by enqueuing a `SpeakRequest` with the override
//! fields set and asserting which voice/engine the actor resolved (reported on the
//! post-synthesis `Started` event).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent};
use forge_voice::AssignmentStrategy;

use common::{
    make_deps, recording_sink, request_with_overrides, standard_registry, wait_for,
    wait_for_resolved_voice,
};

/// Precedence matrix across the override fields. Each row exercises a distinct
/// branch of `resolve_with_overrides`, not the underlying resolver strategy.
#[tokio::test]
async fn override_fields_decide_resolved_voice_and_engine() {
    // (engine_override, voice_override, expected_voice, expected_engine, why)
    let cases = [
        // voice_override only → engine inferred from the catalog entry for that voice.
        (None, Some("alpha-2"), "alpha-2", "alpha"),
        // both set → voice from voice_override, engine forced by engine_override
        // (cross-engine: a beta voice forced through the alpha engine).
        (Some("alpha"), Some("beta-1"), "beta-1", "alpha"),
        // engine_override only → strategy picks within that engine's scoped voices;
        // beta has exactly one, so the pick is determinate.
        (Some("beta"), None, "beta-1", "beta"),
    ];

    for (engine_override, voice_override, expected_voice, expected_engine) in cases {
        let (sink, _plays) = recording_sink();
        let deps = make_deps(
            standard_registry(),
            sink,
            AssignmentStrategy::DeterministicByName,
            vec![],
        );
        let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

        let req = request_with_overrides("viewer", engine_override, voice_override);
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

        let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
        assert_eq!(
            (voice.as_str(), engine.as_str()),
            (expected_voice, expected_engine),
            "engine_override={engine_override:?} voice_override={voice_override:?}",
        );
    }
}

#[tokio::test]
async fn voice_override_absent_from_catalog_with_no_engine_override_skips() {
    let (sink, plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        Arc::clone(&sink),
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    let req = request_with_overrides("viewer", None, Some("ghost-voice"));
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    wait_for(
        &mut stream,
        |e| {
            matches!(
                e,
                SpeakEvent::Skipped { reason, .. } if reason.contains("voice override not found")
            )
        },
        2_000,
    )
    .await;
    assert_eq!(*plays.lock().unwrap(), 0, "skipped item must not play");
}

#[tokio::test]
async fn engine_override_forces_synthesis_for_voice_absent_from_catalog() {
    // An unknown voice would Skip on its own, but a set engine_override short-circuits
    // the catalog lookup and forces synthesis through that engine.
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    let req = request_with_overrides("viewer", Some("beta"), Some("ghost-voice"));
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    let (voice, engine) = wait_for_resolved_voice(&mut stream, 2_000).await;
    assert_eq!((voice.as_str(), engine.as_str()), ("ghost-voice", "beta"));
}
