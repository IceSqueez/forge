#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_events::{Event, EventSource};
use forge_registry::{ActorBlock, ActorIdentity, LoginSlot, TriggerVariables};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId,
    SynthesisHint, Variant, VariantKind,
};

const CANONICAL_ROSTER: &[(&str, VariantKind)] = &[
    ("user_id", VariantKind::String),
    ("user_name", VariantKind::String),
    ("user_login", VariantKind::String),
    ("user_platform", VariantKind::String),
    ("moderator_id", VariantKind::String),
    ("moderator_name", VariantKind::String),
    ("moderator_login", VariantKind::String),
    ("moderator_platform", VariantKind::String),
    ("gifter_id", VariantKind::String),
    ("gifter_name", VariantKind::String),
    ("gifter_login", VariantKind::String),
    ("gifter_platform", VariantKind::String),
    ("recipient_id", VariantKind::String),
    ("recipient_name", VariantKind::String),
    ("recipient_login", VariantKind::String),
    ("recipient_platform", VariantKind::String),
    ("message_text", VariantKind::String),
    ("viewer_count", VariantKind::Int),
    ("bits_amount", VariantKind::Int),
    ("gift_count", VariantKind::Int),
    ("sub_cumulative_months", VariantKind::Int),
    ("sub_streak_months", VariantKind::Int),
    ("sub_tier", VariantKind::String),
];

fn every_canonical_variable_in_declared_order() -> Vec<CanonicalVariable> {
    let mut all: Vec<CanonicalVariable> = ActorRole::ALL
        .into_iter()
        .flat_map(|role| {
            ActorSlot::ALL
                .into_iter()
                .map(move |slot| CanonicalVariable::actor(role, slot))
        })
        .chain(std::iter::once(CanonicalVariable::MessageText))
        .chain(
            CanonicalCount::ALL
                .into_iter()
                .map(CanonicalVariable::Count),
        )
        .chain(std::iter::once(CanonicalVariable::SubTier))
        .collect();
    all.sort_by_key(|canonical| canonical.order());
    all
}

fn no_payload() -> Event {
    Event::new(EventSource::Core, "test.event", serde_json::Value::Null)
}

fn twitch_block(role: ActorRole) -> ActorBlock {
    ActorBlock {
        role,
        platform: PlatformId::Twitch,
        login: LoginSlot::Declared,
    }
}

fn identity(id: &str, display_name: &str, login: Option<&str>) -> ActorIdentity {
    ActorIdentity {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        login: login.map(str::to_owned),
    }
}

fn text(value: &'static str) -> DeclaredVariable {
    DeclaredVariable {
        name: value.to_owned(),
        kind: VariantKind::String,
        label: value.to_owned(),
        synthesis: None,
    }
}

fn flag(value: &'static str) -> DeclaredVariable {
    DeclaredVariable {
        name: value.to_owned(),
        kind: VariantKind::Bool,
        label: value.to_owned(),
        synthesis: None,
    }
}

fn message(name: &'static str) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind: VariantKind::String,
        label: name.to_owned(),
        synthesis: Some(SynthesisHint::Message),
    }
}

fn one_of_every_entry_kind() -> TriggerVariables {
    TriggerVariables::new()
        .legacy(
            message("sub_message"),
            CanonicalVariable::MessageText,
            |_| Variant::String("thanks for the stream".to_owned()),
        )
        .event_specific(flag("share_streak"), |_| Variant::Bool(true))
        .sub_tier(|_| "1000".to_owned())
        .message_text(|_| "thanks for the stream".to_owned())
        .actor(twitch_block(ActorRole::Principal), |_| {
            identity("222", "LoyalFan", Some("loyalfan"))
        })
        .count(CanonicalCount::SubStreakMonths, |_| 6)
        .event_specific(text("followed_at"), |_| {
            Variant::String("2026-09-21T00:00:00Z".to_owned())
        })
}

fn declared_names(variables: &TriggerVariables) -> Vec<String> {
    variables
        .declarations()
        .into_iter()
        .map(|variable| variable.declared.name.clone())
        .collect()
}

#[test]
fn the_canonical_vocabulary_is_this_roster_of_names_and_kinds_in_this_order() {
    let actual: Vec<(&str, VariantKind)> = every_canonical_variable_in_declared_order()
        .into_iter()
        .map(|canonical| (canonical.name(), canonical.kind()))
        .collect();
    assert_eq!(actual, CANONICAL_ROSTER);
}

#[test]
fn a_platform_without_logins_omits_the_login_from_the_schema_and_from_the_arg_stack() {
    let variables = TriggerVariables::new().actor(
        ActorBlock {
            role: ActorRole::Principal,
            platform: PlatformId::YouTube,
            login: LoginSlot::PlatformHasNone,
        },
        |_| identity("UC123", "Chat Watcher", None),
    );

    assert_eq!(
        declared_names(&variables),
        ["user_id", "user_name", "user_platform"]
    );
    assert_eq!(
        variables
            .arg_stack(&no_payload())
            .snapshot()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ["user_id", "user_name", "user_platform"]
    );
}

#[test]
fn every_declared_name_carries_a_value_and_every_value_has_a_declared_name() {
    let variables = one_of_every_entry_kind();
    let schema = variables.schema();
    let stack = variables.arg_stack(&no_payload()).snapshot();

    let declared: BTreeSet<String> = schema
        .variables
        .iter()
        .map(|variable| variable.name.clone())
        .collect();
    let published: BTreeSet<String> = stack.keys().cloned().collect();
    assert_eq!(declared, published);

    for variable in &schema.variables {
        assert_eq!(
            VariantKind::from_variant(&stack[&variable.name]),
            variable.kind,
            "'{}' is declared {:?} but published {:?}",
            variable.name,
            variable.kind,
            stack[&variable.name],
        );
    }
}

#[test]
fn canonical_entries_are_declared_ahead_of_event_specific_and_legacy_entries() {
    assert_eq!(
        declared_names(&one_of_every_entry_kind()),
        [
            "user_id",
            "user_name",
            "user_login",
            "user_platform",
            "message_text",
            "sub_streak_months",
            "sub_tier",
            "share_streak",
            "followed_at",
            "sub_message",
        ]
    );
}

#[test]
fn the_shown_name_falls_back_to_the_login_and_is_empty_only_when_neither_arrives() {
    let cases = [
        (
            "display name and login",
            "LoyalFan",
            Some("loyalfan"),
            "LoyalFan",
            "loyalfan",
        ),
        ("display name only", "LoyalFan", None, "LoyalFan", ""),
        ("login only", "", Some("loyalfan"), "loyalfan", "loyalfan"),
        ("neither", "", None, "", ""),
    ];

    for (case, display_name, login, expected_name, expected_login) in cases {
        let variables = TriggerVariables::new()
            .actor(twitch_block(ActorRole::Principal), move |_| {
                identity("222", display_name, login)
            });
        let stack = variables.arg_stack(&no_payload());
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String(expected_name.to_owned())),
            "case: {case}"
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String(expected_login.to_owned())),
            "case: {case}"
        );
    }
}

#[test]
fn a_legacy_entry_carries_its_canonical_twins_value_and_names_the_twin_it_defers_to() {
    let variables = one_of_every_entry_kind();
    let stack = variables.arg_stack(&no_payload());
    let line = Variant::String("thanks for the stream".to_owned());
    assert_eq!(stack.get("message_text"), Some(&line));
    assert_eq!(stack.get("sub_message"), Some(&line));

    let standings: Vec<(String, Option<&'static str>)> = variables
        .declarations()
        .into_iter()
        .map(|variable| {
            (
                variable.declared.name.clone(),
                variable
                    .standing
                    .superseded_by()
                    .map(CanonicalVariable::name),
            )
        })
        .collect();
    assert_eq!(
        standings
            .iter()
            .find(|(name, _)| name == "sub_message")
            .map(|(_, superseded_by)| *superseded_by),
        Some(Some("message_text"))
    );
    assert!(
        standings
            .iter()
            .filter(|(name, _)| name != "sub_message")
            .all(|(_, superseded_by)| superseded_by.is_none()),
        "only a legacy entry names a canonical replacement: {standings:?}"
    );
}
