use serde::{Deserialize, Serialize};

pub use forge_tts_core::{EngineId, TtsVoice, VoiceId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AliasId(pub String);

impl AliasId {
    pub fn new() -> Self {
        Self(ulid::Ulid::r#gen().to_string())
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
    /// `sha256(viewer_name) % eligible_voices.len()` — stable, deterministic per username.
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
    pub fn resolve(
        &self,
        viewer_id: &str,
        viewer_name: &str,
        voice_catalog: &[TtsVoice],
    ) -> ResolveResult {
        if let Some(alias) = self.aliases.iter().find(|a| a.viewer_id == viewer_id) {
            return match alias.state {
                AliasState::Blocked => ResolveResult::Skip {
                    reason: "blocked by alias",
                },
                AliasState::Active => ResolveResult::Speak {
                    voice_id: alias.voice_id.clone(),
                    engine_id: alias.engine_id.clone(),
                    pitch: alias
                        .pitch_semitones
                        .unwrap_or(self.defaults.pitch_semitones),
                    rate: alias
                        .rate_multiplier
                        .unwrap_or(self.defaults.rate_multiplier),
                },
            };
        }

        let mut eligible: Vec<&TtsVoice> = voice_catalog
            .iter()
            .filter(|v| self.profile.is_eligible(v))
            .collect();

        if eligible.is_empty() {
            return ResolveResult::Skip {
                reason: "no voices available",
            };
        }

        eligible.sort_by(|a, b| a.id.0.cmp(&b.id.0));

        let idx = match &self.strategy {
            AssignmentStrategy::DeterministicByName => {
                use sha2::{Digest, Sha256};
                let digest = Sha256::digest(viewer_name.as_bytes());
                let hash_u64 = u64::from_le_bytes([
                    digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6],
                    digest[7],
                ]);
                (hash_u64 as usize) % eligible.len()
            }
            AssignmentStrategy::Random => (rand::random::<u64>() as usize) % eligible.len(),
            AssignmentStrategy::Single {
                voice_id,
                engine_id,
            } => {
                return ResolveResult::Speak {
                    voice_id: voice_id.clone(),
                    engine_id: engine_id.clone(),
                    pitch: self.defaults.pitch_semitones,
                    rate: self.defaults.rate_multiplier,
                };
            }
        };

        let voice = eligible[idx];
        ResolveResult::Speak {
            voice_id: voice.id.clone(),
            engine_id: voice.engine_id.clone(),
            pitch: self.defaults.pitch_semitones,
            rate: self.defaults.rate_multiplier,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_voice(id: &str, locale: &str) -> TtsVoice {
        use forge_tts_core::VoiceGender;
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
    fn resolver_empty_catalog_returns_skip() {
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let result = resolver.resolve("viewer123", "testviewer", &[]);
        assert!(matches!(
            result,
            ResolveResult::Skip {
                reason: "no voices available"
            }
        ));
    }

    #[test]
    fn resolver_blocked_alias_returns_skip() {
        let alias = VoiceAlias {
            id: AliasId::new(),
            viewer_id: "blocked_user".into(),
            viewer_name: "BlockedUser".into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId("uk_UA-medium".into()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Blocked,
        };
        let resolver = VoiceAliasResolver::new(
            vec![alias],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let catalog = vec![make_voice("uk_UA-medium", "uk-UA")];
        let result = resolver.resolve("blocked_user", "BlockedUser", &catalog);
        assert!(matches!(
            result,
            ResolveResult::Skip {
                reason: "blocked by alias"
            }
        ));
    }

    #[test]
    fn resolver_active_alias_overrides_strategy() {
        let alias = VoiceAlias {
            id: AliasId::new(),
            viewer_id: "pinned_user".into(),
            viewer_name: "PinnedUser".into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId("pinned-voice".into()),
            pitch_semitones: Some(2.0),
            rate_multiplier: Some(1.5),
            state: AliasState::Active,
        };
        let resolver = VoiceAliasResolver::new(
            vec![alias],
            AssignmentStrategy::Random,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let catalog = vec![make_voice("other-voice", "en-US")];
        let result = resolver.resolve("pinned_user", "PinnedUser", &catalog);
        match result {
            ResolveResult::Speak {
                voice_id,
                pitch,
                rate,
                ..
            } => {
                assert_eq!(voice_id.0, "pinned-voice");
                assert!((pitch - 2.0).abs() < 0.001);
                assert!((rate - 1.5).abs() < 0.001);
            }
            ResolveResult::Skip { .. } => panic!("expected Speak"),
        }
    }

    #[test]
    fn resolver_deterministic_same_name_same_voice() {
        let catalog = vec![
            make_voice("voice-a", "en-US"),
            make_voice("voice-b", "en-US"),
            make_voice("voice-c", "en-US"),
        ];
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        let r1 = resolver.resolve("user1", "alice", &catalog);
        let r2 = resolver.resolve("user1", "alice", &catalog);
        match (r1, r2) {
            (
                ResolveResult::Speak { voice_id: v1, .. },
                ResolveResult::Speak { voice_id: v2, .. },
            ) => {
                assert_eq!(v1, v2, "same name must always resolve to same voice");
            }
            _ => panic!("expected Speak for both"),
        }
    }

    #[test]
    fn resolver_ignore_profile_excludes_from_pool() {
        let catalog = vec![
            make_voice("boring", "en-US"),
            make_voice("exciting", "en-US"),
        ];
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: vec![],
        };
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            profile,
            SynthesisDefaults::default(),
        );
        let result = resolver.resolve("any", "any", &catalog);
        match result {
            ResolveResult::Speak { voice_id, .. } => {
                assert_eq!(voice_id.0, "exciting", "excluded voice must not be picked");
            }
            ResolveResult::Skip { .. } => panic!("expected Speak"),
        }
    }

    #[test]
    fn resolver_all_excluded_returns_skip() {
        let catalog = vec![make_voice("boring", "en-US")];
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: vec![],
        };
        let resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            profile,
            SynthesisDefaults::default(),
        );
        let result = resolver.resolve("any", "any", &catalog);
        assert!(matches!(
            result,
            ResolveResult::Skip {
                reason: "no voices available"
            }
        ));
    }
}
