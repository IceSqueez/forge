//! Regression: IgnoreProfile must exclude voices from the auto-assignment pool.
//!
//! Excluded voices remain accessible via explicit manual aliases but must
//! never appear in the random/deterministic selection pool. When all voices
//! are excluded the resolver must return Skip rather than panicking.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_core::{EngineId, VoiceGender, VoiceId};
use forge_voice::{
    AssignmentStrategy, IgnoreProfile, ResolveResult, SynthesisDefaults, TtsVoice,
    VoiceAliasResolver,
};

fn make_voice(id: &str, locale: &str) -> TtsVoice {
    TtsVoice {
        id: VoiceId(id.into()),
        name: id.into(),
        locale: locale.into(),
        gender: VoiceGender::Neutral,
        engine_id: EngineId("piper".into()),
        is_neural: false,
        sample_rate_hint: 22_050,
    }
}

#[test]
fn excluded_voice_id_never_selected_by_deterministic_strategy() {
    let catalog = vec![
        make_voice("excluded-voice", "en-US"),
        make_voice("allowed-voice", "en-US"),
    ];
    let profile = IgnoreProfile {
        excluded_voice_ids: vec![VoiceId("excluded-voice".into())],
        excluded_locales: vec![],
    };
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        profile,
        SynthesisDefaults::default(),
    );

    // With two voices and one excluded, every resolution must use "allowed-voice".
    for name in ["alice", "bob", "charlie", "delta", "echo"] {
        match resolver.resolve(name, name, &catalog) {
            ResolveResult::Speak { voice_id, .. } => {
                assert_ne!(
                    voice_id.0, "excluded-voice",
                    "excluded voice must never be picked (viewer: {name})"
                );
                assert_eq!(voice_id.0, "allowed-voice");
            }
            ResolveResult::Skip { reason } => {
                panic!("expected Speak for viewer {name}, got Skip: {reason}");
            }
        }
    }
}

#[test]
fn excluded_locale_removes_all_voices_of_that_locale() {
    let catalog = vec![
        make_voice("de-voice-1", "de-DE"),
        make_voice("de-voice-2", "de-DE"),
        make_voice("en-voice", "en-US"),
    ];
    let profile = IgnoreProfile {
        excluded_voice_ids: vec![],
        excluded_locales: vec!["de-DE".into()],
    };
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        profile,
        SynthesisDefaults::default(),
    );

    match resolver.resolve("uid-x", "someviewer", &catalog) {
        ResolveResult::Speak { voice_id, .. } => {
            assert_eq!(
                voice_id.0, "en-voice",
                "only the non-excluded locale voice should be eligible"
            );
        }
        ResolveResult::Skip { reason } => panic!("expected Speak, got Skip: {reason}"),
    }
}

#[test]
fn all_voices_excluded_returns_skip_not_panic() {
    let catalog = vec![make_voice("only-voice", "en-US")];
    let profile = IgnoreProfile {
        excluded_voice_ids: vec![VoiceId("only-voice".into())],
        excluded_locales: vec![],
    };
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        profile,
        SynthesisDefaults::default(),
    );

    match resolver.resolve("uid-y", "viewer", &catalog) {
        ResolveResult::Skip { reason } => {
            assert_eq!(reason, "no voices available");
        }
        ResolveResult::Speak { .. } => panic!("expected Skip when all voices are excluded"),
    }
}

#[test]
fn random_strategy_never_picks_excluded_voice() {
    let catalog = vec![
        make_voice("excluded", "en-US"),
        make_voice("valid-1", "en-US"),
        make_voice("valid-2", "en-US"),
    ];
    let profile = IgnoreProfile {
        excluded_voice_ids: vec![VoiceId("excluded".into())],
        excluded_locales: vec![],
    };
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::Random,
        profile,
        SynthesisDefaults::default(),
    );

    // Run enough iterations to make a false-positive statistically negligible.
    for i in 0..200 {
        match resolver.resolve(&format!("uid-{i}"), &format!("viewer{i}"), &catalog) {
            ResolveResult::Speak { voice_id, .. } => {
                assert_ne!(
                    voice_id.0, "excluded",
                    "random strategy must never pick excluded voice (iteration {i})"
                );
            }
            ResolveResult::Skip { reason } => panic!("unexpected Skip: {reason}"),
        }
    }
}
