use serde::{Deserialize, Serialize};

pub use forge_tts_core::{EngineId, TtsVoice, VoiceId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AliasId(pub String);

impl AliasId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}

impl Default for AliasId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AliasState {
    Active,
    /// Message is accepted but no audio is synthesized.
    Blocked,
}

/// A manually pinned voice for one viewer.
///
/// `viewer_id` is the platform-specific user ID (Twitch user ID string), not username.
/// Username changes must not break the alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAlias {
    pub id: AliasId,
    pub viewer_id: String,
    pub viewer_name: String,
    pub engine_id: EngineId,
    pub voice_id: VoiceId,
    /// Semitone override relative to engine default. `None` = use engine default.
    pub pitch_semitones: Option<f32>,
    /// Rate multiplier override. `None` = use engine default.
    pub rate_multiplier: Option<f32>,
    pub state: AliasState,
}

/// How a voice is chosen for viewers without a manual alias.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AssignmentStrategy {
    /// `hash(username) % eligible_voices.len()` — stable, deterministic.
    #[default]
    DeterministicByName,
    /// Random pick from eligible voices each message.
    Random,
    /// Every message uses the same voice.
    Single {
        voice_id: VoiceId,
        engine_id: EngineId,
    },
}

/// Voices and locales excluded from random/deterministic picks.
///
/// Excluded voices are still usable by explicit manual aliases.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IgnoreProfile {
    pub excluded_voice_ids: Vec<VoiceId>,
    pub excluded_locales: Vec<String>,
}

impl IgnoreProfile {
    pub fn is_eligible(&self, voice: &TtsVoice) -> bool {
        !self.excluded_voice_ids.contains(&voice.id)
            && !self.excluded_locales.contains(&voice.locale)
    }
}

/// Pitch and rate fallback values when a voice has no per-alias override.
#[derive(Debug, Clone, Copy)]
pub struct SynthesisDefaults {
    pub pitch_semitones: f32,
    pub rate_multiplier: f32,
}

impl Default for SynthesisDefaults {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
        }
    }
}

/// Resolution result for one incoming message.
#[derive(Debug, Clone)]
pub enum ResolveResult {
    Speak {
        voice_id: VoiceId,
        engine_id: EngineId,
        pitch: f32,
        rate: f32,
    },
    Skip {
        reason: &'static str,
    },
}

pub struct VoiceAliasResolver {
    pub aliases: Vec<VoiceAlias>,
    pub strategy: AssignmentStrategy,
    pub profile: IgnoreProfile,
    pub defaults: SynthesisDefaults,
}

impl VoiceAliasResolver {
    pub fn new(
        aliases: Vec<VoiceAlias>,
        strategy: AssignmentStrategy,
        profile: IgnoreProfile,
        defaults: SynthesisDefaults,
    ) -> Self {
        Self {
            aliases,
            strategy,
            profile,
            defaults,
        }
    }

    /// Resolution chain:
    /// 1. Explicit alias by viewer_id → honours AliasState::Blocked as Skip.
    /// 2. Strategy fallback using eligible voices from the provided catalog.
    /// 3. If catalog is empty → Skip("no voices available").
    pub fn resolve(&self, _viewer_id: &str, _voice_catalog: &[TtsVoice]) -> ResolveResult {
        ResolveResult::Skip {
            reason: "resolver not yet wired",
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn voice_alias_serde_roundtrip() {
        let alias = VoiceAlias {
            id: AliasId("01HWTEST".into()),
            viewer_id: "123456".into(),
            viewer_name: "testviewer".into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId("uk_UA-ukrainian-medium".into()),
            pitch_semitones: Some(2.0),
            rate_multiplier: None,
            state: AliasState::Active,
        };
        let json = serde_json::to_string(&alias).unwrap();
        let back: VoiceAlias = serde_json::from_str(&json).unwrap();
        assert_eq!(back.viewer_id, alias.viewer_id);
        assert_eq!(back.state, AliasState::Active);
        assert_eq!(back.pitch_semitones, Some(2.0));
        assert!(back.rate_multiplier.is_none());
    }

    #[test]
    fn assignment_strategy_serde_roundtrip() {
        let strategy = AssignmentStrategy::Single {
            voice_id: VoiceId("uk_UA-ukrainian-medium".into()),
            engine_id: EngineId("piper".into()),
        };
        let json = serde_json::to_string(&strategy).unwrap();
        let back: AssignmentStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, strategy);

        let det = AssignmentStrategy::DeterministicByName;
        let det_json = serde_json::to_string(&det).unwrap();
        let det_back: AssignmentStrategy = serde_json::from_str(&det_json).unwrap();
        assert_eq!(det_back, AssignmentStrategy::DeterministicByName);
    }

    #[test]
    fn ignore_profile_serde_roundtrip() {
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: vec!["de-DE".into()],
        };
        let json = serde_json::to_string(&profile).unwrap();
        let back: IgnoreProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.excluded_locales, vec!["de-DE".to_string()]);
        assert_eq!(back.excluded_voice_ids.len(), 1);
    }

    #[test]
    fn ignore_profile_is_eligible() {
        use forge_tts_core::VoiceGender;
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: vec!["de-DE".into()],
        };
        let eligible = TtsVoice {
            id: VoiceId("uk_UA-medium".into()),
            name: "Ukrainian".into(),
            locale: "uk-UA".into(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId("piper".into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        };
        let excluded_by_id = TtsVoice {
            id: VoiceId("boring".into()),
            name: "Boring".into(),
            locale: "en-US".into(),
            gender: VoiceGender::Male,
            engine_id: EngineId("piper".into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        };
        let excluded_by_locale = TtsVoice {
            id: VoiceId("german".into()),
            name: "German".into(),
            locale: "de-DE".into(),
            gender: VoiceGender::Female,
            engine_id: EngineId("piper".into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        };
        assert!(profile.is_eligible(&eligible));
        assert!(!profile.is_eligible(&excluded_by_id));
        assert!(!profile.is_eligible(&excluded_by_locale));
    }

    #[test]
    fn resolver_stub_returns_skip() {
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let result = resolver.resolve("viewer123", &[]);
        assert!(matches!(result, ResolveResult::Skip { .. }));
    }
}
