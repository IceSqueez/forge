#![expect(clippy::panic, reason = "test fixtures assert directly")]
use forge_conformance::every_registered_trigger;
use forge_overlay::config::{HEADLINE, SUBLINE};
use forge_overlay::kinds::alert::AlertOverlayKind;
use forge_overlay::{EventWiringTrigger, OverlayConfig, is_curated, suggested_content};
use forge_registry::{CanonicalCount, CanonicalVariable, TriggerVariable, declared_variables};
use forge_types::{ArgStack, Variant};
use std::collections::BTreeSet;

const CURATED_IN_PICKER_ORDER: &[&str] = &[
    "twitch.support.subscriber",
    "twitch.support.resubscriber",
    "twitch.support.gift_sub",
    "twitch.support.cheer",
    "donation.received",
    "twitch.channel.raid_received",
    "twitch.channel.follow",
    "twitch.channel_points.redemption",
];

const UNCURATED: &str = "stub.uncurated";
const RESOLVED: &str = "resolved";

fn lines(kind_id: &str, label: &str, variables: &[TriggerVariable]) -> (String, String) {
    let content = suggested_content(
        &AlertOverlayKind,
        &EventWiringTrigger {
            kind_id,
            label,
            variables,
        },
    );
    (text_at(&content, HEADLINE), text_at(&content, SUBLINE))
}

fn text_at(content: &OverlayConfig, key: &str) -> String {
    content
        .get(key)
        .and_then(Variant::as_str)
        .unwrap_or_else(|| panic!("the suggestion left {key} unfilled"))
        .to_owned()
}

fn declared_names(variables: &[TriggerVariable]) -> BTreeSet<String> {
    variables
        .iter()
        .map(|variable| variable.declared.name.clone())
        .collect()
}

fn resolving_every(names: &BTreeSet<String>) -> ArgStack {
    names.iter().fold(ArgStack::new(), |stack, name| {
        stack.set(name.clone(), Variant::String(RESOLVED.to_owned()))
    })
}

#[test]
fn every_curated_row_is_admitted_for_the_real_trigger_it_names() {
    let triggers = every_registered_trigger();

    for kind_id in CURATED_IN_PICKER_ORDER {
        assert!(is_curated(kind_id), "{kind_id} left the launch set");
        let descriptor = triggers
            .get(kind_id)
            .unwrap_or_else(|| panic!("{kind_id} is no longer a registered trigger"));
        let variables = declared_variables(descriptor)
            .unwrap_or_else(|| panic!("{kind_id} declares no variables at all"));

        let curated = lines(kind_id, descriptor.label(), &variables);
        let derived = lines(UNCURATED, descriptor.label(), &variables);

        assert_ne!(
            curated, derived,
            "{kind_id} fell back to derived wording, so its curated row names an undeclared token",
        );
    }
}

#[test]
fn a_curated_row_missing_one_declared_token_falls_back_to_derived_wording_whole() {
    let triggers = every_registered_trigger();
    let kind_id = "twitch.support.resubscriber";
    let descriptor = triggers.get(kind_id).expect("the resubscriber ships");
    let label = descriptor.label();
    let full = declared_variables(descriptor).expect("the resubscriber declares variables");
    let pruned: Vec<TriggerVariable> = full
        .iter()
        .filter(|variable| {
            variable.declared.name
                != CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths).name()
        })
        .cloned()
        .collect();

    assert_ne!(
        pruned.len(),
        full.len(),
        "the pruned token was never declared"
    );
    assert_eq!(
        lines(kind_id, label, &pruned),
        lines(UNCURATED, label, &pruned),
        "one absent token left the curated headline standing beside derived wording",
    );
}

#[test]
fn every_registered_trigger_is_suggested_wording_its_own_delivery_can_carry() {
    let triggers = every_registered_trigger();

    for descriptor in triggers.all() {
        let variables = declared_variables(descriptor).unwrap_or_default();
        let stack = resolving_every(&declared_names(&variables));
        let (headline, subline) = lines(descriptor.id(), descriptor.label(), &variables);

        for line in [headline, subline] {
            assert!(
                !stack.interpolate(&line).contains('%'),
                "{} is suggested {line:?}, which names something it never declares",
                descriptor.id(),
            );
            assert!(
                !line.is_empty(),
                "{} is suggested a blank line, which a delivery replaces with the overlay's own stale wording",
                descriptor.id(),
            );
        }
    }
}
