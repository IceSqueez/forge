#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds, sample_payload};
use serde_json::Value;

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn keys_of(event_kind: &str) -> BTreeSet<String> {
    sample_payload(event_kind)
        .as_object()
        .unwrap_or_else(|| panic!("{event_kind} sampled something other than an object"))
        .keys()
        .cloned()
        .collect()
}

fn expected(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn placeholders(template: &str) -> Vec<String> {
    template
        .split('%')
        .skip(1)
        .step_by(2)
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
        .collect()
}

#[test]
fn a_sample_carries_exactly_the_fields_of_the_family_its_kind_names() {
    for (event_kind, fields) in [
        ("twitch.chat.message", &["channel", "message", "user"][..]),
        ("kick.chat.message", &["channel", "message", "user"]),
        ("twitch.channel.follow", &["followed_at", "user"]),
        ("twitch.channel.subscribe", &["is_gift", "tier", "user"]),
        (
            "twitch.channel.subscription.message",
            &[
                "cumulative_months",
                "message",
                "share_streak",
                "streak_months",
                "tier",
                "user",
            ],
        ),
        (
            "twitch.channel.subscription.gift",
            &["gifter", "is_anonymous", "recipient", "tier"],
        ),
        (
            "kick.channel.subscription.gifts",
            &["gifter", "is_anonymous", "recipient", "tier"],
        ),
        (
            "twitch.channel.cheer",
            &["bits", "is_anonymous", "message", "user"],
        ),
        (
            "twitch.channel.raid",
            &[
                "direction",
                "from_broadcaster",
                "to_broadcaster",
                "viewer_count",
            ],
        ),
        ("obs.scene.changed", &["message", "user"]),
    ] {
        assert_eq!(
            keys_of(event_kind),
            expected(fields),
            "{event_kind} was sampled as the wrong family"
        );
    }
}

/// Mirrors the runtime's resolution order: an exact top level key wins before dots walk nesting.
fn resolve<'a>(sample: &'a Value, token: &str) -> Option<&'a Value> {
    sample.get(token).or_else(|| {
        token
            .split('.')
            .try_fold(sample, |value, segment| value.get(segment))
    })
}

#[test]
fn every_placeholder_a_builtin_default_writes_names_a_field_its_sample_renders_as_text() {
    let targets = [
        ("overlay.alert", "twitch.channel.subscription.message"),
        ("overlay.ticker", "twitch.channel.cheer"),
        ("overlay.frame", "twitch.channel.subscribe"),
        ("overlay.chat", "overlay.chat"),
        ("overlay.goal", "overlay.goal"),
    ];
    let reg = registry();

    assert_eq!(
        targets
            .iter()
            .map(|(kind, _)| *kind)
            .collect::<BTreeSet<_>>(),
        reg.all().map(|d| d.id()).collect::<BTreeSet<_>>(),
        "a builtin overlay kind has no sample its default template was written against"
    );

    for (kind_id, event_kind) in targets {
        let descriptor = reg.get(kind_id).expect("a registered builtin kind");
        let defaults = descriptor.default_config();
        let sample = sample_payload(event_kind);

        for (key, held) in &defaults {
            let Some(template) = held.as_str() else {
                continue;
            };
            for token in placeholders(template) {
                let value = resolve(&sample, &token).unwrap_or_else(|| {
                    panic!(
                        "{kind_id} defaults {key} to %{token}%, a field {event_kind} never carries"
                    )
                });
                assert!(
                    value.is_string() || value.is_number() || value.is_boolean(),
                    "{kind_id} defaults {key} to %{token}%, which {event_kind} carries as {value}; \
                     the page renders that as a field count, not as text"
                );
            }
        }
    }
}
