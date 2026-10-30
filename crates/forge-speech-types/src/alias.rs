use serde::{Deserialize, Serialize};

use crate::voice::{EngineId, TtsVoice, VoiceId};

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

fn normalized_locale(locale: &str) -> String {
    locale.trim().replace('_', "-").to_lowercase()
}

fn primary_subtag(normalized: &str) -> &str {
    normalized.split('-').next().unwrap_or(normalized)
}

fn locales_overlap(excluded: &str, voice: &str) -> bool {
    let excluded = normalized_locale(excluded);
    let voice = normalized_locale(voice);
    excluded == voice
        || ((!excluded.contains('-') || !voice.contains('-'))
            && primary_subtag(&excluded) == primary_subtag(&voice))
}

impl IgnoreProfile {
    pub fn is_eligible(&self, voice: &TtsVoice) -> bool {
        !self.excluded_voice_ids.contains(&voice.id)
            && !self
                .excluded_locales
                .iter()
                .any(|excluded| locales_overlap(excluded, &voice.locale))
    }
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

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::*;
    use crate::voice::VoiceGender;

    fn encode_and_reencode<T: Serialize + DeserializeOwned>(value: &T, wire: &str) -> [String; 2] {
        let decoded: T = serde_json::from_str(wire).unwrap();
        [
            serde_json::to_string(value).unwrap(),
            serde_json::to_string(&decoded).unwrap(),
        ]
    }

    #[test]
    fn voice_alias_keeps_its_stored_json_shape() {
        let alias = VoiceAlias {
            id: AliasId("01HWTEST".into()),
            viewer_id: "123456".into(),
            viewer_name: "testviewer".into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId("uk_UA-ukrainian-medium".into()),
            pitch_semitones: Some(2.0),
            rate_multiplier: None,
            state: AliasState::Blocked,
        };
        let wire = r#"{"id":"01HWTEST","viewer_id":"123456","viewer_name":"testviewer","engine_id":"piper","voice_id":"uk_UA-ukrainian-medium","pitch_semitones":2.0,"rate_multiplier":null,"state":"Blocked"}"#;
        assert_eq!(encode_and_reencode(&alias, wire), [wire, wire]);
    }

    #[test]
    fn assignment_strategy_keeps_its_stored_json_shape_for_each_variant() {
        for (strategy, wire) in [
            (
                AssignmentStrategy::DeterministicByName,
                r#""DeterministicByName""#,
            ),
            (AssignmentStrategy::Random, r#""Random""#),
            (
                AssignmentStrategy::Single {
                    voice_id: VoiceId("uk_UA-ukrainian-medium".into()),
                    engine_id: EngineId("piper".into()),
                },
                r#"{"Single":{"voice_id":"uk_UA-ukrainian-medium","engine_id":"piper"}}"#,
            ),
        ] {
            assert_eq!(encode_and_reencode(&strategy, wire), [wire, wire]);
        }
    }

    #[test]
    fn ignore_profile_and_synthesis_defaults_keep_their_stored_json_shape() {
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: vec!["de-DE".into()],
        };
        let profile_wire = r#"{"excluded_voice_ids":["boring"],"excluded_locales":["de-DE"]}"#;
        assert_eq!(
            encode_and_reencode(&profile, profile_wire),
            [profile_wire, profile_wire]
        );
        let defaults = SynthesisDefaults {
            pitch_semitones: -1.5,
            rate_multiplier: 1.25,
        };
        let defaults_wire = r#"{"pitch_semitones":-1.5,"rate_multiplier":1.25}"#;
        assert_eq!(
            encode_and_reencode(&defaults, defaults_wire),
            [defaults_wire, defaults_wire]
        );
    }

    fn voice(id: &str, locale: &str) -> TtsVoice {
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

    fn excluding_locale(locale: &str) -> IgnoreProfile {
        IgnoreProfile {
            excluded_voice_ids: Vec::new(),
            excluded_locales: vec![locale.into()],
        }
    }

    #[test]
    fn ignore_profile_excludes_a_voice_by_id_regardless_of_locale() {
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: Vec::new(),
        };
        assert!(!profile.is_eligible(&voice("boring", "en-US")));
        assert!(profile.is_eligible(&voice("lively", "en-US")));
    }

    #[test]
    fn a_region_exclusion_matches_the_same_region_in_any_spelling_and_case() {
        let profile = excluding_locale("de-DE");
        for locale in ["de-DE", "de_DE", "DE-de", "de-de", " de-DE "] {
            assert!(!profile.is_eligible(&voice("v", locale)), "{locale:?}");
        }
    }

    #[test]
    fn a_region_exclusion_also_matches_a_voice_reporting_only_the_language() {
        let profile = excluding_locale("de-DE");
        for locale in ["de", "DE"] {
            assert!(!profile.is_eligible(&voice("v", locale)), "{locale:?}");
        }
    }

    #[test]
    fn a_region_exclusion_spares_other_regions_and_other_languages() {
        let profile = excluding_locale("de-DE");
        for locale in ["de-AT", "de_CH", "uk-UA", "den-DE", "d", ""] {
            assert!(profile.is_eligible(&voice("v", locale)), "{locale:?}");
        }
    }

    #[test]
    fn a_language_only_exclusion_matches_every_region_of_that_language() {
        for excluded in ["de", "DE"] {
            let profile = excluding_locale(excluded);
            for locale in ["de", "de-DE", "de_AT", "DE-ch"] {
                assert!(
                    !profile.is_eligible(&voice("v", locale)),
                    "{excluded:?} vs {locale:?}"
                );
            }
            for locale in ["deu-DE", "dev", "en-US"] {
                assert!(
                    profile.is_eligible(&voice("v", locale)),
                    "{excluded:?} vs {locale:?}"
                );
            }
        }
    }

    #[test]
    fn an_empty_locale_exclusion_excludes_nothing() {
        assert!(excluding_locale("").is_eligible(&voice("v", "en-US")));
    }
}
