#![allow(clippy::expect_used, clippy::unwrap_used)]
#[expect(dead_code)]
mod fakes;

use forge_events::{Event, EventSource};
use forge_registry::{
    CanonicalVariable, EventFilter, FormField, KindPlatformContract, SeedVariable, TriggerCategory,
    TriggerCoverage, TriggerKindDescriptor, TriggerRegistry, TriggerSeed, TriggerVariables,
    declared_variables,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use fakes::{FakeTrigger, trigger_of};

const WIDE: &str = "stub.wide";
const NARROW: &str = "stub.narrow";
const ALIASED: &str = "stub.aliased";
const SILENT: &str = "stub.silent";
const MISSING: &str = "stub.never_registered";
const HAND_STACKED: &str = "stub.hand_stacked";

fn declared(name: &str, kind: VariantKind) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        label: name.to_owned(),
        synthesis: None,
    }
}

fn wide_variables() -> TriggerVariables {
    TriggerVariables::new()
        .event_specific(declared("reason", VariantKind::String), |event| {
            Variant::String(
                event.payload["reason"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .event_specific(declared("amount", VariantKind::Int), |event| {
            Variant::Int(event.payload["amount"].as_i64().unwrap_or_default())
        })
}

fn narrow_variables() -> TriggerVariables {
    TriggerVariables::new()
        .event_specific(declared("amount", VariantKind::Int), |_| Variant::Int(0))
}

fn aliased_variables() -> TriggerVariables {
    TriggerVariables::new()
        .legacy("msg", "msg", CanonicalVariable::MessageText)
        .message_text(|_| String::new())
}

struct HandStacked;

impl TriggerKindDescriptor for HandStacked {
    fn id(&self) -> &str {
        HAND_STACKED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        HAND_STACKED
    }

    fn summary(&self) -> &str {
        HAND_STACKED
    }

    fn search_text(&self) -> &str {
        HAND_STACKED
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![declared("scene_name", VariantKind::String)],
        })
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        ArgStack::new().set(
            "scene_name".to_owned(),
            Variant::String(
                event.payload["scene"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ),
        )
    }
}

fn registry() -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    for (id, variables) in [
        (WIDE, Some(wide_variables as fn() -> TriggerVariables)),
        (NARROW, Some(narrow_variables)),
        (ALIASED, Some(aliased_variables)),
        (SILENT, None),
    ] {
        reg.register(Box::new(FakeTrigger { id, variables }))
            .expect("each fake trigger registers under its own id");
    }
    reg.register(Box::new(HandStacked))
        .expect("the hand-stacked trigger registers under its own id");
    reg
}

fn seed(kinds: &[&str]) -> TriggerSeed {
    let triggers: Vec<_> = kinds.iter().map(|kind| trigger_of(kind)).collect();
    TriggerSeed::build(&triggers, &registry())
}

fn partial_marks(seed: &TriggerSeed) -> Vec<(&str, bool)> {
    seed.variables()
        .iter()
        .map(|variable| (variable.name.as_str(), variable.some_triggers_only))
        .collect()
}

fn event() -> Event {
    Event::new(
        EventSource::Core,
        "test.seed",
        serde_json::json!({ "reason": "spam", "amount": 7, "scene": "Intro" }),
    )
}

#[test]
fn no_triggers_build_an_empty_seed_with_no_coverage() {
    let built = seed(&[]);

    assert_eq!(built.coverage(), &TriggerCoverage::NoTriggers);
    assert!(built.variables().is_empty());
}

#[test]
fn fully_declared_triggers_mark_only_names_some_of_them_lack() {
    let built = seed(&[WIDE, NARROW]);

    assert_eq!(built.coverage(), &TriggerCoverage::Declared);
    assert_eq!(
        partial_marks(&built),
        vec![("reason", true), ("amount", false)]
    );
}

#[test]
fn an_undeclared_trigger_beside_declared_ones_marks_every_name_partial() {
    let built = seed(&[WIDE, NARROW, SILENT]);

    assert_eq!(
        built.coverage(),
        &TriggerCoverage::PartlyUnknown {
            undeclared_kinds: vec![SILENT.to_owned()],
        }
    );
    assert_eq!(
        partial_marks(&built),
        vec![("reason", true), ("amount", true)]
    );
}

#[test]
fn only_undeclared_triggers_list_each_unknown_kind_once_and_seed_no_names() {
    let built = seed(&[SILENT, MISSING, SILENT]);

    assert_eq!(
        built.coverage(),
        &TriggerCoverage::PartlyUnknown {
            undeclared_kinds: vec![SILENT.to_owned(), MISSING.to_owned()],
        }
    );
    assert!(built.variables().is_empty());
}

#[test]
fn a_kind_missing_from_the_registry_counts_as_undeclared() {
    let built = seed(&[WIDE, MISSING]);

    assert_eq!(
        built.coverage(),
        &TriggerCoverage::PartlyUnknown {
            undeclared_kinds: vec![MISSING.to_owned()],
        }
    );
}

#[test]
fn a_legacy_only_name_follows_every_current_name_and_is_marked_superseded() {
    let built = seed(&[ALIASED, NARROW]);

    let listed: Vec<(&str, bool)> = built
        .variables()
        .iter()
        .map(|variable| (variable.name.as_str(), variable.superseded))
        .collect();
    assert_eq!(
        listed,
        vec![("message_text", false), ("amount", false), ("msg", true)]
    );
}

#[test]
fn a_name_current_on_one_trigger_and_legacy_on_another_stays_current() {
    fn current_msg() -> TriggerVariables {
        TriggerVariables::new()
            .event_specific(declared("msg", VariantKind::String), |_| Variant::Int(0))
    }
    let mut reg = registry();
    reg.register(Box::new(FakeTrigger {
        id: "stub.current_msg",
        variables: Some(current_msg),
    }))
    .expect("a fresh id registers");

    let built = TriggerSeed::build(&[trigger_of(ALIASED), trigger_of("stub.current_msg")], &reg);

    let msg: Vec<&SeedVariable> = built
        .variables()
        .iter()
        .filter(|variable| variable.name == "msg")
        .collect();
    assert_eq!(msg.len(), 1);
    assert!(!msg[0].superseded);
}

#[test]
fn the_registry_lists_the_same_declaration_as_the_descriptor() {
    let reg = registry();

    for kind in [WIDE, ALIASED, HAND_STACKED, SILENT] {
        let descriptor = reg.get(kind).expect("registered");
        assert_eq!(
            reg.declared_variables(kind).map(<[_]>::to_vec),
            declared_variables(descriptor),
            "{kind}"
        );
    }
    assert!(reg.declared_variables(MISSING).is_none());
}

#[test]
fn the_registry_builds_the_same_arg_stack_as_the_descriptor() {
    let reg = registry();
    let event = event();

    for kind in [WIDE, HAND_STACKED] {
        let descriptor = reg.get(kind).expect("registered");
        assert_eq!(
            reg.arg_stack(kind, &event, &TriggerConfig::new())
                .snapshot(),
            descriptor.build_arg_stack(&event).snapshot(),
            "{kind}"
        );
    }
}

#[test]
fn the_registry_arg_stack_of_an_unknown_kind_is_empty() {
    assert!(
        registry()
            .arg_stack(MISSING, &event(), &TriggerConfig::new())
            .snapshot()
            .is_empty()
    );
}
