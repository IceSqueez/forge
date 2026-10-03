#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::{ContentFeed, OverlayKindRegistry, register_builtin_kinds};
use forge_platform_twitch::register_twitch_triggers;
use forge_registry::TriggerRegistry;
use forge_types::{LATEST_DONATION_SLOT, Variant, latest_fields};

const DEFAULT_SOURCE_TRIGGERS: &[(&str, &str)] = &[
    ("overlay.alert", "twitch.support.resubscriber"),
    ("overlay.chat", "twitch.chat.message"),
    ("overlay.ticker", "twitch.support.cheer"),
];

const DONATION_RECORD_FIELDS: &[&str] = &[
    latest_fields::PLATFORM,
    latest_fields::OCCURRED_AT,
    latest_fields::USER_NAME,
    latest_fields::AMOUNT,
    latest_fields::AMOUNT_MICROS,
    latest_fields::AMOUNT_FORMATTED,
    latest_fields::CURRENCY,
    latest_fields::DONATION_KIND,
];

fn overlay_kinds() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn twitch_triggers() -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    register_twitch_triggers(&mut reg).expect("the twitch triggers register");
    reg
}

fn declared_variables(triggers: &TriggerRegistry, trigger_id: &str) -> BTreeSet<String> {
    triggers
        .get(trigger_id)
        .unwrap_or_else(|| panic!("{trigger_id} is not a registered trigger"))
        .output_schema()
        .unwrap_or_else(|| panic!("{trigger_id} declares no variables for a step to interpolate"))
        .variables
        .into_iter()
        .map(|variable| variable.name)
        .collect()
}

fn tokens(template: &str) -> Vec<String> {
    template
        .split('%')
        .skip(1)
        .step_by(2)
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
        .collect()
}

#[test]
fn every_builtin_overlay_default_names_a_variable_its_source_trigger_declares() {
    let overlays = overlay_kinds();
    let triggers = twitch_triggers();

    for (kind_id, _) in DEFAULT_SOURCE_TRIGGERS {
        assert!(
            overlays.get(kind_id).is_some(),
            "{kind_id} is paired with a trigger but is not a registered overlay kind"
        );
    }

    for descriptor in overlays.all() {
        let kind_id = descriptor.id();
        let source = DEFAULT_SOURCE_TRIGGERS
            .iter()
            .find(|(overlay_id, _)| *overlay_id == kind_id)
            .map(|(_, trigger_id)| *trigger_id);
        let slot_bound = descriptor.content_feed() == ContentFeed::LatestSlot;
        if slot_bound {
            assert_eq!(
                descriptor
                    .default_config()
                    .get("slot")
                    .and_then(Variant::as_str),
                Some(LATEST_DONATION_SLOT),
                "{kind_id} opens on a slot other than the donation slot whose record this check reads"
            );
        }
        let declared = if slot_bound {
            DONATION_RECORD_FIELDS
                .iter()
                .map(|field| (*field).to_owned())
                .collect()
        } else {
            source.map_or_else(BTreeSet::new, |trigger_id| {
                declared_variables(&triggers, trigger_id)
            })
        };
        let vocabulary = if slot_bound {
            "the donation record"
        } else {
            source.unwrap_or("any trigger paired with this kind")
        };

        for (key, held) in &descriptor.default_config() {
            let Some(template) = Variant::as_str(held) else {
                continue;
            };
            for token in tokens(template) {
                assert!(
                    declared.contains(&token),
                    "{kind_id} defaults {key} to %{token}%, which {vocabulary} never declares"
                );
            }
        }
    }
}
