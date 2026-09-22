use std::collections::BTreeMap;

use forge_types::{
    ActorRole, ActorSlot, ArgStack, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    VariableStanding, Variant, VariantKind,
};
use rand::RngExt;
use time::OffsetDateTime;

use crate::kind_platform_contract::KindPlatformContract;
use crate::variables::TriggerVariable;

const DISPLAY_NAME_POOL: &[&str] = &[
    "TestUser",
    "StreamFan42",
    "NightOwl",
    "PixelPal",
    "CoolViewer",
];
const ACTOR_ID_POOL: &[&str] = &["37402112", "51938264", "60114873", "72840591", "84025617"];
const MESSAGE_POOL: &[&str] = &[
    "hey, great stream!",
    "this is a test message",
    "love the content",
    "what game is this?",
    "first time here, hi!",
];
const SUB_TIER_POOL: &[&str] = &["1000", "2000", "3000"];
const NO_PLATFORM: &str = "";
const FALLBACK_TOKEN: &str = "sample";
const GENERIC_INT_MIN: i64 = 1;
const GENERIC_INT_MAX: i64 = 100;
const GENERIC_FLOAT_MIN: f64 = 0.01;
const GENERIC_FLOAT_MAX: f64 = 99.99;
const RATIO_RESOLUTION: i64 = 1_000;
const FIRST_IN_POOL: usize = 0;
const MID_RATIO_STEP: i64 = RATIO_RESOLUTION / 2;

pub struct SynthesisSample {
    platform: Option<PlatformId>,
    name_index: usize,
    message_index: usize,
    tier_index: usize,
    ratio_step: i64,
    flag: bool,
    now: OffsetDateTime,
}

impl SynthesisSample {
    pub fn random(contract: KindPlatformContract) -> Self {
        let mut rng = rand::rng();
        SynthesisSample {
            platform: platform_of(contract),
            name_index: rng.random_range(0..DISPLAY_NAME_POOL.len()),
            message_index: rng.random_range(0..MESSAGE_POOL.len()),
            tier_index: rng.random_range(0..SUB_TIER_POOL.len()),
            ratio_step: rng.random_range(0..=RATIO_RESOLUTION),
            flag: rng.random_range(0..=1) == 1,
            now: OffsetDateTime::now_utc(),
        }
    }

    /// Yields the same values on every call, so a regenerated preview is byte-identical to the last.
    pub fn stable(contract: KindPlatformContract) -> Self {
        SynthesisSample {
            platform: platform_of(contract),
            name_index: FIRST_IN_POOL,
            message_index: FIRST_IN_POOL,
            tier_index: FIRST_IN_POOL,
            ratio_step: MID_RATIO_STEP,
            flag: true,
            now: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn display_name(&self, role: ActorRole) -> &'static str {
        rotated(DISPLAY_NAME_POOL, self.name_index, role as usize)
    }

    fn actor_id(&self, role: ActorRole) -> &'static str {
        rotated(ACTOR_ID_POOL, self.name_index, role as usize)
    }

    fn message(&self) -> &'static str {
        rotated(MESSAGE_POOL, self.message_index, 0)
    }

    fn sub_tier(&self) -> &'static str {
        rotated(SUB_TIER_POOL, self.tier_index, 0)
    }

    fn platform_name(&self) -> &'static str {
        self.platform.map_or(NO_PLATFORM, PlatformId::as_str)
    }

    fn ratio(&self) -> f64 {
        self.ratio_step as f64 / RATIO_RESOLUTION as f64
    }

    fn bounded_int(&self, min: i64, max: i64) -> i64 {
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        lo + ((hi - lo) as f64 * self.ratio()).round() as i64
    }

    fn bounded_float(&self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.ratio()
    }
}

pub fn synthesize_args(variables: &[TriggerVariable], sample: &SynthesisSample) -> ArgStack {
    let mut minted: BTreeMap<CanonicalVariable, Variant> = BTreeMap::new();
    let mut stack = ArgStack::new();
    for variable in variables {
        let value = match canonical_slot(variable.standing) {
            Some(canonical) => minted
                .entry(canonical)
                .or_insert_with(|| canonical_value(canonical, sample))
                .clone(),
            None => event_specific_value(&variable.declared, sample),
        };
        stack = stack.set(variable.declared.name.clone(), value);
    }
    stack
}

fn platform_of(contract: KindPlatformContract) -> Option<PlatformId> {
    match contract {
        KindPlatformContract::PlatformSpecific(platform) => Some(platform),
        KindPlatformContract::Universal => None,
    }
}

fn canonical_slot(standing: VariableStanding) -> Option<CanonicalVariable> {
    match standing {
        VariableStanding::Canonical(canonical) | VariableStanding::Legacy(canonical) => {
            Some(canonical)
        }
        VariableStanding::EventSpecific => None,
    }
}

fn canonical_value(canonical: CanonicalVariable, sample: &SynthesisSample) -> Variant {
    match canonical {
        CanonicalVariable::Actor { role, slot } => Variant::String(actor_value(role, slot, sample)),
        CanonicalVariable::SubTier => Variant::String(sample.sub_tier().to_owned()),
        CanonicalVariable::MessageText | CanonicalVariable::Count(_) => hinted_value(
            canonical.synthesis().as_ref(),
            canonical.kind(),
            canonical.name(),
            sample,
        ),
    }
}

fn actor_value(role: ActorRole, slot: ActorSlot, sample: &SynthesisSample) -> String {
    match slot {
        ActorSlot::Id => sample.actor_id(role).to_owned(),
        ActorSlot::Name => sample.display_name(role).to_owned(),
        ActorSlot::Login => login_of(sample.display_name(role)),
        ActorSlot::Platform => sample.platform_name().to_owned(),
    }
}

fn login_of(display_name: &str) -> String {
    display_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn event_specific_value(declared: &DeclaredVariable, sample: &SynthesisSample) -> Variant {
    let token_base = if declared.label.is_empty() {
        &declared.name
    } else {
        &declared.label
    };
    hinted_value(
        declared.synthesis.as_ref(),
        declared.kind,
        token_base,
        sample,
    )
}

fn hinted_value(
    hint: Option<&SynthesisHint>,
    kind: VariantKind,
    token_base: &str,
    sample: &SynthesisSample,
) -> Variant {
    match hint {
        Some(SynthesisHint::Username) => {
            Variant::String(login_of(sample.display_name(ActorRole::Principal)))
        }
        Some(SynthesisHint::DisplayName) => {
            Variant::String(sample.display_name(ActorRole::Principal).to_owned())
        }
        Some(SynthesisHint::Message) => Variant::String(sample.message().to_owned()),
        Some(SynthesisHint::BoundedInt { min, max }) => {
            Variant::Int(sample.bounded_int(*min, *max))
        }
        None => untyped_value(kind, token_base, sample),
    }
}

fn untyped_value(kind: VariantKind, token_base: &str, sample: &SynthesisSample) -> Variant {
    match kind {
        VariantKind::String => Variant::String(sample_token(token_base)),
        VariantKind::Int => Variant::Int(sample.bounded_int(GENERIC_INT_MIN, GENERIC_INT_MAX)),
        VariantKind::Float => {
            Variant::Float(sample.bounded_float(GENERIC_FLOAT_MIN, GENERIC_FLOAT_MAX))
        }
        VariantKind::Bool => Variant::Bool(sample.flag),
        VariantKind::Datetime => Variant::Datetime(sample.now),
        VariantKind::Array => Variant::Array(vec![
            Variant::String(sample_token(token_base)),
            Variant::String(login_of(sample.display_name(ActorRole::Principal))),
        ]),
        VariantKind::Object => Variant::Object(BTreeMap::new()),
    }
}

fn rotated(pool: &[&'static str], index: usize, offset: usize) -> &'static str {
    pool[(index + offset) % pool.len()]
}

fn sample_token(base: &str) -> String {
    let slug: String = base
        .split_whitespace()
        .next()
        .unwrap_or(FALLBACK_TOKEN)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if slug.is_empty() {
        FALLBACK_TOKEN.to_owned()
    } else {
        format!("{slug}_sample")
    }
}

#[cfg(test)]
mod tests {
    use forge_types::CanonicalCount;

    use super::*;

    const PRINCIPAL_NAME: &str = "StreamFan42";
    const PRINCIPAL_LOGIN: &str = "streamfan42";
    const PRINCIPAL_ID: &str = "51938264";
    const MODERATOR_NAME: &str = "NightOwl";
    const MODERATOR_LOGIN: &str = "nightowl";
    const MODERATOR_ID: &str = "60114873";
    const CHOSEN_MESSAGE: &str = "love the content";

    fn sample() -> SynthesisSample {
        SynthesisSample {
            platform: Some(PlatformId::Kick),
            name_index: 1,
            message_index: 2,
            tier_index: 0,
            ratio_step: 250,
            flag: true,
            now: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn canonical_entry(canonical: CanonicalVariable) -> TriggerVariable {
        TriggerVariable::canonical(canonical)
    }

    fn legacy_entry(
        name: &str,
        kind: VariantKind,
        synthesis: Option<SynthesisHint>,
        superseded_by: CanonicalVariable,
    ) -> TriggerVariable {
        TriggerVariable {
            declared: DeclaredVariable {
                name: name.to_owned(),
                kind,
                label: name.to_owned(),
                synthesis,
            },
            standing: VariableStanding::Legacy(superseded_by),
        }
    }

    fn event_specific_entry(
        name: &str,
        label: &str,
        kind: VariantKind,
        synthesis: Option<SynthesisHint>,
    ) -> TriggerVariable {
        TriggerVariable {
            declared: DeclaredVariable {
                name: name.to_owned(),
                kind,
                label: label.to_owned(),
                synthesis,
            },
            standing: VariableStanding::EventSpecific,
        }
    }

    fn text(value: &str) -> Option<Variant> {
        Some(Variant::String(value.to_owned()))
    }

    #[test]
    fn every_legacy_alias_carries_the_value_minted_for_the_slot_it_supersedes() {
        let aliases = [
            (
                "display_name",
                VariantKind::String,
                Some(SynthesisHint::DisplayName),
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
            ),
            (
                "username",
                VariantKind::String,
                Some(SynthesisHint::Username),
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            ),
            (
                "content",
                VariantKind::String,
                Some(SynthesisHint::Message),
                CanonicalVariable::MessageText,
            ),
            (
                "months",
                VariantKind::Int,
                Some(SynthesisHint::BoundedInt { min: 1, max: 24 }),
                CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths),
            ),
            (
                "gifter_username",
                VariantKind::String,
                Some(SynthesisHint::Username),
                CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Login),
            ),
        ];
        let mut variables: Vec<TriggerVariable> = aliases
            .iter()
            .map(|(name, kind, synthesis, canonical)| {
                legacy_entry(name, *kind, synthesis.clone(), *canonical)
            })
            .collect();
        variables.extend(
            aliases
                .iter()
                .map(|(_, _, _, canonical)| canonical_entry(*canonical)),
        );

        let stack = synthesize_args(&variables, &sample());

        for (name, _, _, canonical) in aliases {
            let minted = stack.get(canonical.name());
            assert!(minted.is_some(), "{} was never minted", canonical.name());
            assert_eq!(stack.get(name), minted, "{name}");
        }
    }

    #[test]
    fn a_legacy_count_alias_takes_the_canonical_range_past_its_own_declared_maximum() {
        let viewer_count = CanonicalVariable::Count(CanonicalCount::ViewerCount);
        let stack = synthesize_args(
            &[
                legacy_entry(
                    "raid_viewer_count",
                    VariantKind::Int,
                    Some(SynthesisHint::BoundedInt { min: 0, max: 500 }),
                    viewer_count,
                ),
                canonical_entry(viewer_count),
            ],
            &sample(),
        );

        assert_eq!(stack.get("raid_viewer_count"), Some(&Variant::Int(12_500)));
        assert_eq!(stack.get("viewer_count"), Some(&Variant::Int(12_500)));
    }

    #[test]
    fn an_actor_login_is_that_same_actors_display_name_lowercased() {
        let stack = synthesize_args(
            &[
                canonical_entry(CanonicalVariable::actor(
                    ActorRole::Principal,
                    ActorSlot::Name,
                )),
                canonical_entry(CanonicalVariable::actor(
                    ActorRole::Principal,
                    ActorSlot::Login,
                )),
                canonical_entry(CanonicalVariable::actor(
                    ActorRole::Moderator,
                    ActorSlot::Name,
                )),
                canonical_entry(CanonicalVariable::actor(
                    ActorRole::Moderator,
                    ActorSlot::Login,
                )),
            ],
            &sample(),
        );

        assert_eq!(stack.get("user_name"), text(PRINCIPAL_NAME).as_ref());
        assert_eq!(stack.get("user_login"), text(PRINCIPAL_LOGIN).as_ref());
        assert_eq!(stack.get("moderator_name"), text(MODERATOR_NAME).as_ref());
        assert_eq!(stack.get("moderator_login"), text(MODERATOR_LOGIN).as_ref());
    }

    #[test]
    fn each_actor_role_holds_one_id_shared_by_its_aliases_and_distinct_from_other_roles() {
        let stack = synthesize_args(
            &[
                canonical_entry(CanonicalVariable::actor(
                    ActorRole::Principal,
                    ActorSlot::Id,
                )),
                canonical_entry(CanonicalVariable::actor(
                    ActorRole::Moderator,
                    ActorSlot::Id,
                )),
                legacy_entry(
                    "chatter_id",
                    VariantKind::String,
                    None,
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                ),
                legacy_entry(
                    "deleted_by_id",
                    VariantKind::String,
                    None,
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Id),
                ),
            ],
            &sample(),
        );

        assert_eq!(stack.get("user_id"), text(PRINCIPAL_ID).as_ref());
        assert_eq!(stack.get("moderator_id"), text(MODERATOR_ID).as_ref());
        assert_eq!(stack.get("chatter_id"), text(PRINCIPAL_ID).as_ref());
        assert_eq!(stack.get("deleted_by_id"), text(MODERATOR_ID).as_ref());
    }

    #[test]
    fn every_actor_platform_slot_reports_the_descriptors_own_platform() {
        for (platform, expected) in [(Some(PlatformId::Kick), "kick"), (None, "")] {
            let stack = synthesize_args(
                &[
                    canonical_entry(CanonicalVariable::actor(
                        ActorRole::Principal,
                        ActorSlot::Platform,
                    )),
                    canonical_entry(CanonicalVariable::actor(
                        ActorRole::Gifter,
                        ActorSlot::Platform,
                    )),
                ],
                &SynthesisSample {
                    platform,
                    ..sample()
                },
            );

            assert_eq!(stack.get("user_platform"), text(expected).as_ref());
            assert_eq!(stack.get("gifter_platform"), text(expected).as_ref());
        }
    }

    #[test]
    fn an_event_specific_variable_follows_its_own_hint() {
        let stack = synthesize_args(
            &[
                event_specific_entry(
                    "reward_cost",
                    "Reward cost",
                    VariantKind::Int,
                    Some(SynthesisHint::BoundedInt { min: 10, max: 20 }),
                ),
                event_specific_entry(
                    "raider_name",
                    "Raider",
                    VariantKind::String,
                    Some(SynthesisHint::DisplayName),
                ),
                event_specific_entry(
                    "reply_body",
                    "Reply",
                    VariantKind::String,
                    Some(SynthesisHint::Message),
                ),
            ],
            &sample(),
        );

        assert_eq!(stack.get("reward_cost"), Some(&Variant::Int(13)));
        assert_eq!(stack.get("raider_name"), text(PRINCIPAL_NAME).as_ref());
        assert_eq!(stack.get("reply_body"), text(CHOSEN_MESSAGE).as_ref());
    }

    #[test]
    fn an_unhinted_event_specific_variable_falls_back_to_its_declared_kind() {
        for (kind, expected) in [
            (
                VariantKind::String,
                Variant::String("reward_sample".to_owned()),
            ),
            (VariantKind::Int, Variant::Int(26)),
            (VariantKind::Bool, Variant::Bool(true)),
            (
                VariantKind::Datetime,
                Variant::Datetime(OffsetDateTime::UNIX_EPOCH),
            ),
            (VariantKind::Object, Variant::Object(BTreeMap::new())),
            (
                VariantKind::Array,
                Variant::Array(vec![
                    Variant::String("reward_sample".to_owned()),
                    Variant::String(PRINCIPAL_LOGIN.to_owned()),
                ]),
            ),
        ] {
            let stack = synthesize_args(
                &[event_specific_entry("payload", "Reward title", kind, None)],
                &sample(),
            );

            assert_eq!(stack.get("payload"), Some(&expected), "{kind:?}");
        }
    }

    #[test]
    fn an_unhinted_float_variable_lands_inside_the_generic_float_span() {
        let stack = synthesize_args(
            &[event_specific_entry(
                "payload",
                "Reward title",
                VariantKind::Float,
                None,
            )],
            &sample(),
        );

        assert!(matches!(
            stack.get("payload"),
            Some(Variant::Float(value)) if (value - 25.005).abs() < 1e-9
        ));
    }

    #[test]
    fn a_string_sample_is_named_after_the_label_and_falls_back_to_the_variable_name() {
        for (label, name, expected) in [
            ("Reward title", "reward_id", "reward_sample"),
            ("", "gift_id", "giftid_sample"),
            ("!!! ???", "mystery", "sample"),
        ] {
            let stack = synthesize_args(
                &[event_specific_entry(name, label, VariantKind::String, None)],
                &sample(),
            );

            assert_eq!(stack.get(name), text(expected).as_ref(), "{label:?}");
        }
    }

    #[test]
    fn a_bounded_int_hint_reaches_both_ends_of_its_declared_range() {
        let months = CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths);
        for (ratio_step, expected) in [
            (0, 1),
            (RATIO_RESOLUTION, 120),
            (RATIO_RESOLUTION / 2, 61),
            (250, 31),
        ] {
            let stack = synthesize_args(
                &[canonical_entry(months)],
                &SynthesisSample {
                    ratio_step,
                    ..sample()
                },
            );

            assert_eq!(
                stack.get("sub_cumulative_months"),
                Some(&Variant::Int(expected)),
                "{ratio_step}"
            );
        }
    }
}
