//! Regression: AliasState::Blocked must return Skip regardless of catalog.
//!
//! A blocked viewer alias must never result in audio synthesis even when
//! the voice catalog is fully populated. The explicit alias check runs
//! before any strategy fallback.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_tts_core::{EngineId, VoiceGender, VoiceId};
use forge_voice::{
    AliasId, AliasState, AssignmentStrategy, IgnoreProfile, ResolveResult, SynthesisDefaults,
    TtsVoice, VoiceAlias, VoiceAliasResolver,
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

fn blocked_alias(viewer_id: &str) -> VoiceAlias {
    VoiceAlias {
        id: AliasId::new(),
        viewer_id: viewer_id.into(),
        viewer_name: "BlockedViewer".into(),
        engine_id: EngineId("piper".into()),
        voice_id: VoiceId("some-voice".into()),
        pitch_semitones: None,
        rate_multiplier: None,
        state: AliasState::Blocked,
    }
}

#[test]
fn blocked_alias_returns_skip_with_populated_catalog() {
    let catalog = vec![make_voice("voice-a"), make_voice("voice-b")];
    let resolver = VoiceAliasResolver::new(
        vec![blocked_alias("blocked-uid")],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );

    match resolver.resolve("blocked-uid", "BlockedViewer", &catalog) {
        ResolveResult::Skip { reason } => {
            assert_eq!(reason, "blocked by alias");
        }
        ResolveResult::Speak { .. } => {
            panic!("blocked viewer must never resolve to Speak");
        }
    }
}

#[test]
fn blocked_alias_does_not_affect_other_viewers() {
    let catalog = vec![make_voice("voice-a")];
    let resolver = VoiceAliasResolver::new(
        vec![blocked_alias("blocked-uid")],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );

    match resolver.resolve("other-uid", "OtherViewer", &catalog) {
        ResolveResult::Speak { .. } => {}
        ResolveResult::Skip { reason } => {
            panic!("unblocked viewer should resolve to Speak, got Skip: {reason}");
        }
    }
}

#[test]
fn block_matched_by_viewer_id_not_name() {
    // Block is keyed on viewer_id. A different uid with the same name must pass through.
    let catalog = vec![make_voice("voice-a")];
    let resolver = VoiceAliasResolver::new(
        vec![blocked_alias("uid-blocked")],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );

    // Same viewer_name but different viewer_id — must NOT be blocked.
    match resolver.resolve("uid-other", "BlockedViewer", &catalog) {
        ResolveResult::Speak { .. } => {}
        ResolveResult::Skip { reason } => {
            panic!("viewer with different uid must not be blocked, got Skip: {reason}");
        }
    }
}
