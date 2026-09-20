#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_platform_twitch::register_twitch_triggers;
use forge_registry::TriggerRegistry;
use forge_types::Variant;

const DEFAULT_SOURCE_TRIGGERS: &[(&str, &str)] = &[
    ("overlay.alert", "twitch.support.resubscriber"),
    ("overlay.chat", "twitch.chat.message"),
    ("overlay.ticker", "twitch.support.cheer"),
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
        let declared = source.map_or_else(BTreeSet::new, |trigger_id| {
            declared_variables(&triggers, trigger_id)
        });

        for (key, held) in &descriptor.default_config() {
            let Some(template) = Variant::as_str(held) else {
                continue;
            };
            for token in tokens(template) {
                assert!(
                    declared.contains(&token),
                    "{kind_id} defaults {key} to %{token}%, which {} never declares",
                    source.unwrap_or("any trigger paired with this kind")
                );
            }
        }
    }
}
