use std::collections::HashMap;

use forge_speech_types::{
    AliasId, AliasState, AssignmentStrategy, EngineId, IgnoreProfile, LanguageCode,
    SynthesisDefaults, TtsVoice, VoiceAlias, VoiceId,
};
use forge_types::PlatformId;
use sha2::{Digest, Sha256};

use super::alias_key::split_stored;
use super::language::voice_speaks_language;
use crate::SpeakRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceSkip {
    BlockedByAlias,
    NoVoicesAvailable,
    VoiceOverrideNotFound,
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
        reason: VoiceSkip,
    },
}

pub struct VoiceAliasResolver {
    aliases: Vec<VoiceAlias>,
    strategy: AssignmentStrategy,
    profile: IgnoreProfile,
    defaults: SynthesisDefaults,
    engine_defaults: HashMap<EngineId, SynthesisDefaults>,
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

    pub fn aliases(&self) -> &[VoiceAlias] {
        &self.aliases
    }

    pub fn set_engine_defaults(&mut self, engine_defaults: HashMap<EngineId, SynthesisDefaults>) {
        self.engine_defaults = engine_defaults;
    }

    pub fn set_engine_default(&mut self, engine_id: EngineId, defaults: SynthesisDefaults) {
        self.engine_defaults.insert(engine_id, defaults);
    }

    pub fn strategy(&self) -> &AssignmentStrategy {
        &self.strategy
    }

    pub fn set_strategy(&mut self, strategy: AssignmentStrategy) {
        self.strategy = strategy;
    }

    pub fn set_alias(&mut self, alias: VoiceAlias) {
        match self
            .aliases
            .iter_mut()
            .find(|existing| existing.viewer_id == alias.viewer_id)
        {
            Some(existing) => *existing = alias,
            None => self.aliases.push(alias),
        }
    }

    pub fn remove_alias(&mut self, id: &AliasId) {
        self.aliases.retain(|alias| &alias.id != id);
    }

    pub fn switch_alias(&mut self, key: &str, engine_id: EngineId, voice_id: VoiceId) {
        let lowered = key.to_lowercase();
        let (_, lowered_handle) = split_stored(&lowered);
        let target = self
            .aliases
            .iter()
            .position(|alias| alias.viewer_id == key)
            .or_else(|| {
                [lowered.as_str(), lowered_handle]
                    .into_iter()
                    .find_map(|candidate| {
                        self.aliases
                            .iter()
                            .position(|alias| alias.viewer_id.to_lowercase() == candidate)
                    })
            });
        if let Some(existing) = target.map(|index| &mut self.aliases[index]) {
            existing.engine_id = engine_id;
            existing.voice_id = voice_id;
            existing.state = AliasState::Active;
        }
    }

    pub fn defaults_for(&self, engine_id: &EngineId) -> SynthesisDefaults {
        self.engine_defaults
            .get(engine_id)
            .copied()
            .unwrap_or(self.defaults)
    }

    pub fn active_alias_engine(&self, viewer_id: &str, viewer_name: &str) -> Option<&EngineId> {
        self.alias_for(viewer_id, viewer_name)
            .filter(|alias| alias.state == AliasState::Active)
            .map(|alias| &alias.engine_id)
    }

    pub fn resolve(
        &self,
        request: &SpeakRequest,
        language: Option<LanguageCode>,
        catalog: &[TtsVoice],
    ) -> ResolveResult {
        if let Some(voice_id) = &request.voice_override {
            return self.overridden_voice(voice_id, request.engine_override.as_ref(), catalog);
        }

        if let Some(alias) = self.alias_for(&request.viewer_id, &request.viewer_name) {
            return match alias.state {
                AliasState::Blocked => ResolveResult::Skip {
                    reason: VoiceSkip::BlockedByAlias,
                },
                AliasState::Active => self.speak(
                    alias.voice_id.clone(),
                    alias.engine_id.clone(),
                    alias.pitch_semitones,
                    alias.rate_multiplier,
                ),
            };
        }

        match &self.strategy {
            AssignmentStrategy::Single {
                voice_id,
                engine_id,
            } => self.speak(voice_id.clone(), engine_id.clone(), None, None),
            AssignmentStrategy::DeterministicByName => {
                let pool = scoped_pool(catalog, language, request.engine_override.as_ref());
                self.pick_from_pool(&pool, || stable_name_hash(&request.viewer_name))
            }
            AssignmentStrategy::Random => {
                let pool = scoped_pool(catalog, language, request.engine_override.as_ref());
                self.pick_from_pool(&pool, rand::random::<u64>)
            }
        }
    }

    fn overridden_voice(
        &self,
        voice_id: &VoiceId,
        engine_override: Option<&EngineId>,
        catalog: &[TtsVoice],
    ) -> ResolveResult {
        let engine_id = engine_override.cloned().or_else(|| {
            catalog
                .iter()
                .find(|voice| &voice.id == voice_id)
                .map(|voice| voice.engine_id.clone())
        });
        match engine_id {
            Some(engine_id) => self.speak(voice_id.clone(), engine_id, None, None),
            None => ResolveResult::Skip {
                reason: VoiceSkip::VoiceOverrideNotFound,
            },
        }
    }

    fn pick_from_pool(&self, pool: &[&TtsVoice], draw: impl FnOnce() -> u64) -> ResolveResult {
        let mut eligible: Vec<&TtsVoice> = pool
            .iter()
            .copied()
            .filter(|voice| self.profile.is_eligible(voice))
            .collect();
        if eligible.is_empty() {
            return ResolveResult::Skip {
                reason: VoiceSkip::NoVoicesAvailable,
            };
        }
        eligible.sort_by(|a, b| a.id.0.cmp(&b.id.0));

        let voice = eligible[(draw() as usize) % eligible.len()];
        self.speak(voice.id.clone(), voice.engine_id.clone(), None, None)
    }

    fn speak(
        &self,
        voice_id: VoiceId,
        engine_id: EngineId,
        pitch: Option<f32>,
        rate: Option<f32>,
    ) -> ResolveResult {
        let engine_defaults = self.defaults_for(&engine_id);
        ResolveResult::Speak {
            voice_id,
            engine_id,
            pitch: pitch.unwrap_or(engine_defaults.pitch_semitones),
            rate: rate.unwrap_or(engine_defaults.rate_multiplier),
        }
    }

    fn alias_for(&self, viewer_id: &str, viewer_name: &str) -> Option<&VoiceAlias> {
        let lowered_name = viewer_name.to_lowercase();
        let (viewer_platform, _) = split_stored(viewer_id);
        let named_on = |platform: Option<PlatformId>| {
            self.aliases.iter().find(|alias| {
                let (alias_platform, handle) = split_stored(&alias.viewer_id);
                alias_platform == platform && handle.to_lowercase() == lowered_name
            })
        };
        self.aliases
            .iter()
            .find(|alias| alias.viewer_id == viewer_id)
            .or_else(|| viewer_platform.and_then(|platform| named_on(Some(platform))))
            .or_else(|| named_on(None))
    }
}

fn scoped_pool<'a>(
    catalog: &'a [TtsVoice],
    language: Option<LanguageCode>,
    engine_override: Option<&EngineId>,
) -> Vec<&'a TtsVoice> {
    let in_language: Vec<&TtsVoice> = language
        .map(|code| {
            catalog
                .iter()
                .filter(|voice| voice_speaks_language(voice, code))
                .collect()
        })
        .unwrap_or_default();
    let candidates: Vec<&TtsVoice> = if in_language.is_empty() {
        catalog.iter().collect()
    } else {
        in_language
    };

    let Some(engine_id) = engine_override else {
        return candidates;
    };
    let on_engine: Vec<&TtsVoice> = candidates
        .into_iter()
        .filter(|voice| &voice.engine_id == engine_id)
        .collect();
    if on_engine.is_empty() {
        catalog
            .iter()
            .filter(|voice| &voice.engine_id == engine_id)
            .collect()
    } else {
        on_engine
    }
}

fn stable_name_hash(viewer_name: &str) -> u64 {
    let digest = Sha256::digest(viewer_name.as_bytes());
    let mut prefix = [0u8; size_of::<u64>()];
    prefix.copy_from_slice(&digest[..size_of::<u64>()]);
    u64::from_le_bytes(prefix)
}
