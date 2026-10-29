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

#[cfg(test)]
mod tests {
    use forge_speech_types::VoiceGender;

    use super::*;
    use crate::{Priority, RequestId};

    fn voice(id: &str, engine: &str, locale: &str) -> TtsVoice {
        TtsVoice {
            id: VoiceId(id.into()),
            name: id.into(),
            locale: locale.into(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId(engine.into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    fn piper_voices(ids: &[&str]) -> Vec<TtsVoice> {
        ids.iter().map(|id| voice(id, "piper", "en-US")).collect()
    }

    fn bilingual_catalog() -> Vec<TtsVoice> {
        vec![
            voice("alpha-en", "alpha", "en-US"),
            voice("alpha-uk", "alpha", "uk-UA"),
            voice("beta-uk", "beta", "uk_UA"),
        ]
    }

    fn request(viewer_id: &str, viewer_name: &str) -> SpeakRequest {
        SpeakRequest {
            request_id: RequestId::new(),
            viewer_id: viewer_id.into(),
            viewer_name: viewer_name.into(),
            text: "hello".into(),
            priority: Priority::Normal,
            engine_override: None,
            voice_override: None,
            source_event_id: None,
            is_reward: false,
            target: None,
            message_emotes: Default::default(),
        }
    }

    fn alias(key: &str, voice: &str, state: AliasState) -> VoiceAlias {
        VoiceAlias {
            id: AliasId::new(),
            viewer_id: key.into(),
            viewer_name: key.into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId(voice.into()),
            pitch_semitones: None,
            rate_multiplier: None,
            state,
        }
    }

    fn active(key: &str, voice: &str) -> VoiceAlias {
        alias(key, voice, AliasState::Active)
    }

    fn single(voice: &str) -> AssignmentStrategy {
        AssignmentStrategy::Single {
            voice_id: VoiceId(voice.into()),
            engine_id: EngineId("piper".into()),
        }
    }

    fn resolver_with(
        aliases: Vec<VoiceAlias>,
        strategy: AssignmentStrategy,
        profile: IgnoreProfile,
    ) -> VoiceAliasResolver {
        VoiceAliasResolver::new(aliases, strategy, profile, SynthesisDefaults::default())
    }

    fn deterministic() -> VoiceAliasResolver {
        resolver_with(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
        )
    }

    fn excluding_voice(id: &str) -> IgnoreProfile {
        IgnoreProfile {
            excluded_voice_ids: vec![VoiceId(id.into())],
            excluded_locales: vec![],
        }
    }

    fn language(code: &str) -> LanguageCode {
        LanguageCode::from_locale(code).unwrap()
    }

    fn outcome(result: ResolveResult) -> Result<String, VoiceSkip> {
        match result {
            ResolveResult::Speak { voice_id, .. } => Ok(voice_id.0),
            ResolveResult::Skip { reason } => Err(reason),
        }
    }

    fn spoken(result: ResolveResult) -> String {
        outcome(result).unwrap_or_else(|reason| panic!("expected Speak, skipped: {reason:?}"))
    }

    fn voice_for(aliases: Vec<VoiceAlias>, viewer_id: &str, viewer_name: &str) -> String {
        let resolver = resolver_with(aliases, single("default-voice"), IgnoreProfile::default());
        spoken(resolver.resolve(&request(viewer_id, viewer_name), None, &[]))
    }

    fn pitch_and_rate(result: ResolveResult) -> (f32, f32) {
        match result {
            ResolveResult::Speak { pitch, rate, .. } => (pitch, rate),
            ResolveResult::Skip { reason } => panic!("expected Speak, skipped: {reason:?}"),
        }
    }

    #[test]
    fn a_pooled_strategy_skips_with_no_voices_available_on_an_empty_catalog() {
        for strategy in [
            AssignmentStrategy::DeterministicByName,
            AssignmentStrategy::Random,
        ] {
            let resolver = resolver_with(vec![], strategy.clone(), IgnoreProfile::default());

            assert_eq!(
                outcome(resolver.resolve(&request("uid", "anyone"), None, &[])),
                Err(VoiceSkip::NoVoicesAvailable),
                "{strategy:?}"
            );
        }
    }

    #[test]
    fn the_single_strategy_speaks_its_voice_with_an_empty_catalog() {
        let resolver = resolver_with(vec![], single("only-voice"), IgnoreProfile::default());

        assert_eq!(
            outcome(resolver.resolve(&request("uid", "anyone"), None, &[])),
            Ok("only-voice".to_owned())
        );
    }

    #[test]
    fn the_single_strategy_speaks_its_voice_even_when_the_ignore_profile_excludes_it() {
        let resolver = resolver_with(vec![], single("only-voice"), excluding_voice("only-voice"));

        assert_eq!(
            outcome(resolver.resolve(
                &request("uid", "anyone"),
                None,
                &piper_voices(&["only-voice"])
            )),
            Ok("only-voice".to_owned())
        );
    }

    #[test]
    fn a_blocked_alias_skips_its_viewer_despite_a_populated_catalog() {
        let resolver = resolver_with(
            vec![alias("blocked-uid", "some-voice", AliasState::Blocked)],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
        );

        assert_eq!(
            outcome(resolver.resolve(
                &request("blocked-uid", "BlockedViewer"),
                None,
                &piper_voices(&["voice-a", "voice-b"])
            )),
            Err(VoiceSkip::BlockedByAlias)
        );
    }

    #[test]
    fn a_blocked_alias_leaves_viewers_with_another_key_speaking() {
        let mut blocked = alias("blocked-uid", "some-voice", AliasState::Blocked);
        blocked.viewer_name = "BlockedViewer".into();
        let resolver = resolver_with(
            vec![blocked],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
        );
        for (viewer_id, name) in [("other-uid", "OtherViewer"), ("other-uid", "BlockedViewer")] {
            assert_eq!(
                outcome(resolver.resolve(
                    &request(viewer_id, name),
                    None,
                    &piper_voices(&["voice-a"])
                )),
                Ok("voice-a".to_owned()),
                "{viewer_id} / {name}"
            );
        }
    }

    #[test]
    fn an_active_alias_outranks_the_strategy_and_carries_its_pitch_and_rate() {
        let mut pinned = active("pinned_user", "pinned-voice");
        pinned.pitch_semitones = Some(2.0);
        pinned.rate_multiplier = Some(1.5);
        let resolver = resolver_with(
            vec![pinned],
            AssignmentStrategy::Random,
            IgnoreProfile::default(),
        );

        let result = resolver.resolve(
            &request("pinned_user", "PinnedUser"),
            None,
            &piper_voices(&["other-voice"]),
        );

        assert!(matches!(
            result,
            ResolveResult::Speak { ref voice_id, pitch, rate, .. }
                if voice_id.0 == "pinned-voice" && pitch == 2.0 && rate == 1.5
        ));
    }

    #[test]
    fn an_alias_without_pitch_or_rate_takes_its_engine_defaults() {
        let mut resolver = resolver_with(
            vec![active("nova", "pinned-voice")],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
        );
        resolver.set_engine_default(
            EngineId("piper".into()),
            SynthesisDefaults {
                pitch_semitones: -3.0,
                rate_multiplier: 0.75,
            },
        );

        assert_eq!(
            pitch_and_rate(resolver.resolve(&request("nova", "nova"), None, &[])),
            (-3.0, 0.75)
        );
    }

    #[test]
    fn a_strategy_voice_uses_its_engine_defaults_and_otherwise_the_global_defaults() {
        let mut resolver = VoiceAliasResolver::new(
            vec![],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
            SynthesisDefaults {
                pitch_semitones: 1.0,
                rate_multiplier: 1.25,
            },
        );
        resolver.set_engine_defaults(HashMap::from([(
            EngineId("alpha".into()),
            SynthesisDefaults {
                pitch_semitones: 4.0,
                rate_multiplier: 0.5,
            },
        )]));
        for (catalog_voice, expected) in [
            (voice("a", "alpha", "en-US"), (4.0, 0.5)),
            (voice("b", "beta", "en-US"), (1.0, 1.25)),
        ] {
            assert_eq!(
                pitch_and_rate(resolver.resolve(&request("uid", "nova"), None, &[catalog_voice])),
                expected
            );
        }
    }

    #[test]
    fn deterministic_resolution_gives_a_name_the_same_voice_on_every_call() {
        let catalog = piper_voices(&["voice-00", "voice-01", "voice-02", "voice-03", "voice-04"]);
        let resolver = deterministic();
        let first = spoken(resolver.resolve(&request("uid-alice", "alice"), None, &catalog));

        for _ in 0..50 {
            assert_eq!(
                spoken(resolver.resolve(&request("uid-alice", "alice"), None, &catalog)),
                first
            );
        }
    }

    #[test]
    fn deterministic_resolution_follows_the_name_not_the_viewer_id() {
        let catalog: Vec<TtsVoice> = (0..10)
            .map(|i| voice(&format!("voice-{i:02}"), "piper", "en-US"))
            .collect();
        let resolver = deterministic();

        assert_ne!(
            spoken(resolver.resolve(&request("uid-1", "alice"), None, &catalog)),
            spoken(resolver.resolve(&request("uid-1", "bob"), None, &catalog))
        );
    }

    #[test]
    fn deterministic_resolution_ignores_catalog_order() {
        let ascending = piper_voices(&["voice-0", "voice-1", "voice-2", "voice-3"]);
        let mut descending = ascending.clone();
        descending.reverse();
        let resolver = deterministic();
        let req = request("uid-x", "constname");

        assert_eq!(
            spoken(resolver.resolve(&req, None, &ascending)),
            spoken(resolver.resolve(&req, None, &descending))
        );
    }

    #[test]
    fn a_pooled_strategy_never_picks_a_voice_the_ignore_profile_excludes() {
        let catalog = piper_voices(&["excluded", "valid-1", "valid-2"]);
        for strategy in [
            AssignmentStrategy::DeterministicByName,
            AssignmentStrategy::Random,
        ] {
            let resolver = resolver_with(vec![], strategy.clone(), excluding_voice("excluded"));
            for i in 0..100 {
                let voice = spoken(resolver.resolve(
                    &request(&format!("uid-{i}"), &format!("viewer{i}")),
                    None,
                    &catalog,
                ));

                assert_ne!(voice, "excluded", "{strategy:?} viewer{i}");
            }
        }
    }

    #[test]
    fn an_excluded_locale_removes_every_voice_of_that_locale_from_the_pool() {
        let catalog = vec![
            voice("de-voice-1", "piper", "de-DE"),
            voice("de-voice-2", "piper", "de-DE"),
            voice("en-voice", "piper", "en-US"),
        ];
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![],
            excluded_locales: vec!["de-DE".into()],
        };
        let resolver = resolver_with(vec![], AssignmentStrategy::DeterministicByName, profile);

        assert_eq!(
            spoken(resolver.resolve(&request("uid-x", "someviewer"), None, &catalog)),
            "en-voice"
        );
    }

    #[test]
    fn a_pool_with_every_voice_excluded_skips_with_no_voices_available() {
        let resolver = resolver_with(
            vec![],
            AssignmentStrategy::DeterministicByName,
            excluding_voice("only-voice"),
        );

        assert_eq!(
            outcome(resolver.resolve(
                &request("uid-y", "viewer"),
                None,
                &piper_voices(&["only-voice"])
            )),
            Err(VoiceSkip::NoVoicesAvailable)
        );
    }

    #[test]
    fn a_voice_override_takes_its_engine_from_the_catalog_and_skips_when_absent() {
        let catalog = bilingual_catalog();
        for (override_id, expected) in [
            ("beta-uk", Ok(("beta-uk".to_owned(), "beta".to_owned()))),
            ("ghost", Err(VoiceSkip::VoiceOverrideNotFound)),
        ] {
            let mut req = request("nova", "nova");
            req.voice_override = Some(VoiceId(override_id.into()));
            let resolved = match deterministic().resolve(&req, None, &catalog) {
                ResolveResult::Speak {
                    voice_id,
                    engine_id,
                    ..
                } => Ok((voice_id.0, engine_id.0)),
                ResolveResult::Skip { reason } => Err(reason),
            };

            assert_eq!(resolved, expected, "{override_id}");
        }
    }

    #[test]
    fn a_detected_language_narrows_resolution_to_voices_that_speak_it() {
        let catalog = bilingual_catalog();
        for (viewer, code, suffix) in [("zoryana", "uk", "-uk"), ("nova", "en", "-en")] {
            let voice = spoken(deterministic().resolve(
                &request(viewer, viewer),
                Some(language(code)),
                &catalog,
            ));

            assert!(voice.ends_with(suffix), "{viewer} in {code} got {voice}");
        }
    }

    #[test]
    fn a_detected_language_no_voice_speaks_leaves_the_full_catalog_rather_than_skipping() {
        let catalog = vec![voice("alpha-en", "alpha", "en-US")];

        assert_eq!(
            spoken(deterministic().resolve(
                &request("nova", "nova"),
                Some(language("uk")),
                &catalog
            )),
            "alpha-en"
        );
    }

    #[test]
    fn an_explicit_alias_outranks_the_detected_language() {
        let resolver = resolver_with(
            vec![active("nova", "alpha-en")],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
        );

        assert_eq!(
            spoken(resolver.resolve(
                &request("nova", "nova"),
                Some(language("uk")),
                &bilingual_catalog()
            )),
            "alpha-en"
        );
    }

    #[test]
    fn an_explicit_voice_override_outranks_the_detected_language() {
        let mut req = request("nova", "nova");
        req.voice_override = Some(VoiceId("alpha-en".into()));

        assert_eq!(
            spoken(deterministic().resolve(&req, Some(language("uk")), &bilingual_catalog())),
            "alpha-en"
        );
    }

    #[test]
    fn an_engine_override_intersects_with_the_detected_language() {
        let mut req = request("mira", "mira");
        req.engine_override = Some(EngineId("alpha".into()));

        assert_eq!(
            spoken(deterministic().resolve(&req, Some(language("uk")), &bilingual_catalog())),
            "alpha-uk"
        );
    }

    #[test]
    fn an_empty_language_and_engine_intersection_falls_back_to_the_engine_alone() {
        let mut req = request("nova", "nova");
        req.engine_override = Some(EngineId("beta".into()));

        assert_eq!(
            spoken(deterministic().resolve(&req, Some(language("en")), &bilingual_catalog())),
            "beta-uk"
        );
    }

    #[test]
    fn an_engine_override_with_no_voices_on_that_engine_skips_with_no_voices_available() {
        let mut req = request("nova", "nova");
        req.engine_override = Some(EngineId("gamma".into()));

        assert_eq!(
            outcome(deterministic().resolve(&req, None, &bilingual_catalog())),
            Err(VoiceSkip::NoVoicesAvailable)
        );
    }

    #[test]
    fn an_alias_keyed_by_the_exact_viewer_id_wins_over_one_keyed_by_the_name() {
        let voice = voice_for(
            vec![
                active("aurora", "name-voice"),
                active("twitch:77", "id-voice"),
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
            let voice = voice_for(vec![active(key, "alias-voice")], "kick:9", name);

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
            let voice = voice_for(vec![active("aurora", "alias-voice")], viewer_id, name);

            assert_eq!(voice, "default-voice", "{viewer_id:?} / {name:?}");
        }
    }

    #[test]
    fn a_platform_scoped_alias_applies_only_to_viewers_on_that_platform() {
        for (key, viewer_id, name, expected) in [
            ("twitch:Alice", "twitch:1", "alice", "alias-voice"),
            ("twitch:Alice", "twitch:1", "ALICE", "alias-voice"),
            ("twitch:Alice", "kick:1", "alice", "default-voice"),
            ("twitch:Alice", "youtube:1", "alice", "default-voice"),
            ("twitch:Alice", "1", "alice", "default-voice"),
            ("Alice", "kick:1", "alice", "alias-voice"),
            ("Alice", "youtube:1", "alice", "alias-voice"),
            ("Alice", "1", "alice", "alias-voice"),
        ] {
            let voice = voice_for(vec![active(key, "alias-voice")], viewer_id, name);

            assert_eq!(voice, expected, "alias {key:?} vs {viewer_id:?} / {name:?}");
        }
    }

    #[test]
    fn an_alias_with_an_unknown_prefix_is_a_bare_name_matched_ignoring_case() {
        for (key, viewer_id, name, expected) in [
            ("discord:Alice", "twitch:1", "DISCORD:alice", "alias-voice"),
            ("discord:Alice", "kick:1", "discord:alice", "alias-voice"),
            ("discord:Alice", "twitch:1", "alice", "default-voice"),
            ("Twitch:Alice", "twitch:1", "twitch:alice", "alias-voice"),
            ("Twitch:Alice", "twitch:1", "alice", "default-voice"),
            ("Mr: X", "youtube:1", "mr: x", "alias-voice"),
            ("Mr: X", "youtube:1", " x", "default-voice"),
        ] {
            let voice = voice_for(vec![active(key, "alias-voice")], viewer_id, name);

            assert_eq!(voice, expected, "alias {key:?} vs {viewer_id:?} / {name:?}");
        }
    }

    #[test]
    fn a_scoped_alias_outranks_a_bare_one_on_its_platform_regardless_of_order() {
        for scoped_first in [true, false] {
            let mut aliases = vec![
                active("Alice", "bare-voice"),
                active("twitch:Alice", "scoped-voice"),
            ];
            if scoped_first {
                aliases.reverse();
            }
            for (viewer_id, expected) in [
                ("twitch:1", "scoped-voice"),
                ("kick:1", "bare-voice"),
                ("1", "bare-voice"),
            ] {
                let voice = voice_for(aliases.clone(), viewer_id, "alice");

                assert_eq!(voice, expected, "{viewer_id:?} scoped_first={scoped_first}");
            }
        }
    }

    #[test]
    fn an_exact_viewer_id_alias_outranks_a_platform_scoped_name_alias() {
        let voice = voice_for(
            vec![
                active("twitch:Alice", "scoped-voice"),
                active("twitch:1", "id-voice"),
            ],
            "twitch:1",
            "alice",
        );

        assert_eq!(voice, "id-voice");
    }

    #[test]
    fn active_alias_engine_follows_alias_lookup_and_ignores_blocked_aliases() {
        let engine_alias = |key: &str, engine: &str, state: AliasState| VoiceAlias {
            engine_id: EngineId(engine.into()),
            ..alias(key, "v", state)
        };
        let resolver = resolver_with(
            vec![
                engine_alias("twitch:123", "piper", AliasState::Active),
                engine_alias("twitch:alice", "elevenlabs", AliasState::Active),
                engine_alias("dave", "azure", AliasState::Active),
                engine_alias("twitch:bob", "openai", AliasState::Blocked),
            ],
            AssignmentStrategy::DeterministicByName,
            IgnoreProfile::default(),
        );
        for (viewer_id, viewer_name, expected) in [
            ("twitch:123", "anyone", Some("piper")),
            ("twitch:555", "Alice", Some("elevenlabs")),
            ("youtube:555", "Alice", None),
            ("kick:9", "DAVE", Some("azure")),
            ("dave", "someone-else", Some("azure")),
            ("twitch:bob", "bob", None),
            ("twitch:777", "nobody", None),
            ("", "", None),
        ] {
            assert_eq!(
                resolver
                    .active_alias_engine(viewer_id, viewer_name)
                    .map(|engine| engine.0.as_str()),
                expected,
                "{viewer_id} {viewer_name}"
            );
        }
    }

    #[test]
    fn switch_alias_with_an_unknown_prefix_does_not_fall_back_to_the_bare_handle() {
        let mut resolver = resolver_with(
            vec![active("Alice", "old-voice")],
            single("default-voice"),
            IgnoreProfile::default(),
        );

        resolver.switch_alias(
            "discord:Alice",
            EngineId("piper".into()),
            VoiceId("new-voice".into()),
        );

        assert_eq!(resolver.aliases()[0].voice_id.0, "old-voice");
    }
}
