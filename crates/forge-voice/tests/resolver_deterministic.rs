//! Regression: DeterministicByName strategy must be stable across calls.
//!
//! Invariant: `sha256(viewer_name) % eligible_voices.len()` produces a
//! deterministic index, so the same viewer always gets the same voice even
//! after process restart. Any change to the hash input format or modulo
//! logic breaks this contract and causes viewer voice reassignment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_core::{EngineId, VoiceGender, VoiceId};
use forge_voice::{
    AssignmentStrategy, IgnoreProfile, ResolveResult, SynthesisDefaults, TtsVoice,
    VoiceAliasResolver,
};

fn make_voice(id: &str) -> TtsVoice {
    TtsVoice {
        id: VoiceId(id.into()),
        name: id.into(),
        locale: "en-US".into(),
        gender: VoiceGender::Neutral,
        engine_id: EngineId("piper".into()),
        is_neural: false,
        sample_rate_hint: 22_050,
    }
}

fn make_resolver(catalog_size: usize) -> (VoiceAliasResolver, Vec<TtsVoice>) {
    let catalog: Vec<TtsVoice> = (0..catalog_size)
        .map(|i| make_voice(&format!("voice-{i:02}")))
        .collect();
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );
    (resolver, catalog)
}

#[test]
fn same_name_same_voice_across_repeated_calls() {
    let (resolver, catalog) = make_resolver(5);

    let first = resolver.resolve("uid-alice", "alice", &catalog);
    let voice_id = match first {
        ResolveResult::Speak { voice_id, .. } => voice_id,
        ResolveResult::Skip { reason } => panic!("unexpected Skip: {reason}"),
    };

    for _ in 0..50 {
        match resolver.resolve("uid-alice", "alice", &catalog) {
            ResolveResult::Speak { voice_id: v, .. } => {
                assert_eq!(
                    v, voice_id,
                    "voice must be stable across calls for same name"
                );
            }
            ResolveResult::Skip { reason } => panic!("unexpected Skip: {reason}"),
        }
    }
}

#[test]
fn same_viewer_id_different_name_still_deterministic_on_name() {
    // Resolution uses viewer_name for the hash, not viewer_id.
    // Renaming a viewer changes their voice assignment.
    let (resolver, catalog) = make_resolver(10);

    let r_alice = resolver.resolve("uid-1", "alice", &catalog);
    let r_bob = resolver.resolve("uid-1", "bob", &catalog);

    let v_alice = match r_alice {
        ResolveResult::Speak { voice_id, .. } => voice_id,
        _ => panic!("expected Speak"),
    };
    let v_bob = match r_bob {
        ResolveResult::Speak { voice_id, .. } => voice_id,
        _ => panic!("expected Speak"),
    };

    // With 10 voices, sha256("alice") and sha256("bob") map to different
    // indices. Assert that the two names differ (probabilistic near-certainty).
    assert_ne!(
        v_alice, v_bob,
        "alice and bob should resolve to different voices with 10-voice catalog"
    );
}

#[test]
fn catalog_sorted_before_hash_modulo() {
    // The resolver sorts the eligible list by voice_id before applying modulo.
    // Catalog insertion order must not affect resolution.
    let catalog_asc: Vec<TtsVoice> = (0..4).map(|i| make_voice(&format!("voice-{i}"))).collect();
    let mut catalog_desc = catalog_asc.clone();
    catalog_desc.reverse();

    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );

    let r_asc = resolver.resolve("uid-x", "constname", &catalog_asc);
    let r_desc = resolver.resolve("uid-x", "constname", &catalog_desc);

    let v_asc = match r_asc {
        ResolveResult::Speak { voice_id, .. } => voice_id,
        _ => panic!("expected Speak"),
    };
    let v_desc = match r_desc {
        ResolveResult::Speak { voice_id, .. } => voice_id,
        _ => panic!("expected Speak"),
    };

    assert_eq!(
        v_asc, v_desc,
        "catalog insertion order must not affect deterministic resolution"
    );
}

#[test]
fn empty_catalog_returns_skip() {
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );
    match resolver.resolve("uid-any", "anyone", &[]) {
        ResolveResult::Skip { reason } => {
            assert_eq!(reason, "no voices available");
        }
        ResolveResult::Speak { .. } => panic!("expected Skip for empty catalog"),
    }
}
