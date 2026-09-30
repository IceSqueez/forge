use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use forge_tts_core::{EngineId, TtsVoice, VoiceId};
pub use forge_tts_pipeline::LanguageCode;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AliasId(pub String);

impl AliasId {
    pub fn new() -> Self {
        Self(ulid::Ulid::generate().to_string())
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
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAlias {
    pub id: AliasId,
    pub viewer_id: String,
    pub viewer_name: String,
    pub engine_id: EngineId,
    pub voice_id: VoiceId,
    pub pitch_semitones: Option<f32>,
    pub rate_multiplier: Option<f32>,
    pub state: AliasState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AssignmentStrategy {
    #[default]
    DeterministicByName,
    Random,
    Single {
        voice_id: VoiceId,
        engine_id: EngineId,
    },
}

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

pub fn voice_speaks_language(voice: &TtsVoice, language: LanguageCode) -> bool {
    LanguageCode::from_locale(&voice.locale) == Some(language)
}

pub fn candidate_languages(catalog: &[TtsVoice]) -> Vec<LanguageCode> {
    let mut languages = Vec::new();
    for voice in catalog {
        if let Some(code) = LanguageCode::from_locale(&voice.locale)
            && !languages.contains(&code)
        {
            languages.push(code);
        }
    }
    languages
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    pub engine_defaults: HashMap<EngineId, SynthesisDefaults>,
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
            engine_defaults: HashMap::new(),
        }
    }

    pub fn defaults_for(&self, engine_id: &EngineId) -> SynthesisDefaults {
        self.engine_defaults
            .get(engine_id)
            .copied()
            .unwrap_or(self.defaults)
    }

    fn alias_for(&self, viewer_id: &str, viewer_name: &str) -> Option<&VoiceAlias> {
        let by_name = viewer_name.to_lowercase();
        self.aliases
            .iter()
            .find(|a| a.viewer_id == viewer_id)
            .or_else(|| {
                self.aliases
                    .iter()
                    .find(|a| a.viewer_id.to_lowercase() == by_name)
            })
    }

    pub fn resolve(
        &self,
        viewer_id: &str,
        viewer_name: &str,
        voice_catalog: &[TtsVoice],
    ) -> ResolveResult {
        if let Some(alias) = self.alias_for(viewer_id, viewer_name) {
            return match alias.state {
                AliasState::Blocked => ResolveResult::Skip {
                    reason: "blocked by alias",
                },
                AliasState::Active => {
                    let engine_defaults = self.defaults_for(&alias.engine_id);
                    ResolveResult::Speak {
                        voice_id: alias.voice_id.clone(),
                        engine_id: alias.engine_id.clone(),
                        pitch: alias
                            .pitch_semitones
                            .unwrap_or(engine_defaults.pitch_semitones),
                        rate: alias
                            .rate_multiplier
                            .unwrap_or(engine_defaults.rate_multiplier),
                    }
                }
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
                let engine_defaults = self.defaults_for(engine_id);
                return ResolveResult::Speak {
                    voice_id: voice_id.clone(),
                    engine_id: engine_id.clone(),
                    pitch: engine_defaults.pitch_semitones,
                    rate: engine_defaults.rate_multiplier,
                };
            }
        };

        let voice = eligible[idx];
        let engine_defaults = self.defaults_for(&voice.engine_id);
        ResolveResult::Speak {
            voice_id: voice.id.clone(),
            engine_id: voice.engine_id.clone(),
            pitch: engine_defaults.pitch_semitones,
            rate: engine_defaults.rate_multiplier,
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

    fn language(code: &str) -> LanguageCode {
        LanguageCode::from_locale(code).unwrap()
    }

    #[test]
    fn voice_speaks_language_compares_primary_subtags_across_engine_locale_shapes() {
        let uk = language("uk");
        for locale in ["uk-UA", "uk_ua", "UK", "uk"] {
            assert!(
                voice_speaks_language(&make_voice("v", locale), uk),
                "locale {locale} serves uk"
            );
        }
        assert!(!voice_speaks_language(&make_voice("v", "en-US"), uk));
    }

    #[test]
    fn voice_with_an_unreadable_locale_serves_no_language_at_all() {
        for locale in ["", "und", "0409", "fil-PH"] {
            let voice = make_voice("mystery", locale);
            for code in ["uk", "en", "ru"] {
                assert!(
                    !voice_speaks_language(&voice, language(code)),
                    "locale {locale:?} must not claim {code}"
                );
            }
        }
    }

    #[test]
    fn candidate_languages_lists_each_language_once_in_catalog_order() {
        let catalog = vec![
            make_voice("a", "en-US"),
            make_voice("b", "uk-UA"),
            make_voice("c", "en-GB"),
            make_voice("d", ""),
            make_voice("e", "und"),
            make_voice("f", "ru_RU"),
        ];
        let codes: Vec<String> = candidate_languages(&catalog)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(codes, vec!["en", "uk", "ru"]);
    }

    fn keyed_alias(key: &str, voice: &str) -> VoiceAlias {
        VoiceAlias {
            id: AliasId::new(),
            viewer_id: key.into(),
            viewer_name: key.into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId(voice.into()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        }
    }

    fn resolved_voice(aliases: Vec<VoiceAlias>, viewer_id: &str, viewer_name: &str) -> String {
        let resolver = VoiceAliasResolver::new(
            aliases,
            AssignmentStrategy::Single {
                voice_id: VoiceId("default-voice".into()),
                engine_id: EngineId("piper".into()),
            },
            IgnoreProfile::default(),
            SynthesisDefaults::default(),
        );
        match resolver.resolve(
            viewer_id,
            viewer_name,
            &[make_voice("default-voice", "en-US")],
        ) {
            ResolveResult::Speak { voice_id, .. } => voice_id.0,
            ResolveResult::Skip { reason } => panic!("expected Speak, skipped: {reason}"),
        }
    }

    #[test]
    fn an_alias_keyed_by_the_exact_viewer_id_wins_over_one_keyed_by_the_name() {
        let voice = resolved_voice(
            vec![
                keyed_alias("aurora", "name-voice"),
                keyed_alias("twitch:77", "id-voice"),
            ],
            "twitch:77",
            "Aurora",
        );

        assert_eq!(voice, "id-voice");
    }

    #[test]
    fn an_alias_keyed_by_the_display_name_matches_ignoring_case() {
        for (key, name) in [
            ("NovaFox", "novafox"),
            ("novafox", "NOVAFOX"),
            ("Зірка", "зІРКА"),
        ] {
            let voice = resolved_voice(vec![keyed_alias(key, "alias-voice")], "kick:9", name);

            assert_eq!(voice, "alias-voice", "alias {key:?} vs viewer {name:?}");
        }
    }

    #[test]
    fn a_viewer_matching_no_alias_gets_the_strategy_voice() {
        for (viewer_id, name) in [
            ("twitch:2", "aurora2"),
            ("twitch:2", "auror"),
            ("twitch:2", ""),
            ("system", "Forge"),
        ] {
            let voice = resolved_voice(vec![keyed_alias("aurora", "alias-voice")], viewer_id, name);

            assert_eq!(voice, "default-voice", "{viewer_id:?} / {name:?}");
        }
    }
}
