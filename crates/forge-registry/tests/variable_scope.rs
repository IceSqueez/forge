#![allow(clippy::expect_used)]

mod fakes;

use forge_registry::{
    FormField, ProducedVariable, ScopeLevel, ScopeOrigin, ScopedVariable, SubActionIo,
    SubActionRegistry, TriggerRegistry, TriggerVariables, variables_in_scope,
};
use forge_types::{
    Action, ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, ExecutionMode,
    SubActionConfig, SubActionStep, TriggerInstance, Variant, VariantKind,
};

use fakes::{FakeTrigger, action, encode_chain, runner, step, trigger_of};

const MAKE_TEXT: &str = "fake.make_text";
const MAKE_DATE: &str = "fake.make_date";
const FIXED: &str = "fake.fixed";
const LOOP: &str = "fake.loop";
const BRANCH: &str = "fake.branch";
const INTO_KEY: &str = "into_var";
const BODY_KEY: &str = "body";
const THEN_KEY: &str = "then";
const FIXED_OUT: &str = "fixed_out";
const LOOP_LOCAL: &str = "loop_index";
const ORDERED_TRIGGER: &str = "stub.ordered";
const NARROW_TRIGGER: &str = "stub.narrow";
const BARE_TRIGGER: &str = "stub.bare";
const STRING_STAMP_TRIGGER: &str = "stub.string_stamp";

fn producing(kind: VariantKind) -> SubActionIo {
    SubActionIo {
        produces: vec![ProducedVariable {
            output_name_key: INTO_KEY.to_owned(),
            kind,
            label: "Output".to_owned(),
        }],
        ..SubActionIo::default()
    }
}

fn sub_registry() -> SubActionRegistry {
    let mut reg = SubActionRegistry::new();
    for fake in [
        fakes::FakeRunner {
            io: producing(VariantKind::String),
            ..runner(MAKE_TEXT)
        },
        fakes::FakeRunner {
            io: producing(VariantKind::Datetime),
            ..runner(MAKE_DATE)
        },
        fakes::FakeRunner {
            io: SubActionIo {
                fixed_outputs: vec![FIXED_OUT.to_owned()],
                ..SubActionIo::default()
            },
            ..runner(FIXED)
        },
        fakes::FakeRunner {
            fields: vec![FormField::SubChain {
                key: BODY_KEY,
                label: BODY_KEY,
            }],
            io: SubActionIo {
                body_locals: vec![LOOP_LOCAL.to_owned()],
                ..SubActionIo::default()
            },
            ..runner(LOOP)
        },
        fakes::FakeRunner {
            fields: vec![FormField::SubChain {
                key: THEN_KEY,
                label: THEN_KEY,
            }],
            ..runner(BRANCH)
        },
    ] {
        reg.register(Box::new(fake))
            .expect("each scope fake registers under its own id");
    }
    reg
}

fn declared(name: &str, kind: VariantKind) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        label: name.to_owned(),
        synthesis: None,
    }
}

fn ordered_variables() -> TriggerVariables {
    TriggerVariables::new()
        .event_specific(declared("zeta", VariantKind::String), |_| Variant::Int(0))
        .legacy(
            declared("user", VariantKind::String),
            CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            |_| Variant::Int(0),
        )
        .event_specific(declared("alpha", VariantKind::Int), |_| Variant::Int(0))
}

fn narrow_variables() -> TriggerVariables {
    TriggerVariables::new().event_specific(declared("alpha", VariantKind::Int), |_| Variant::Int(0))
}

fn string_stamp_variables() -> TriggerVariables {
    TriggerVariables::new()
        .event_specific(declared("stamp", VariantKind::String), |_| Variant::Int(0))
}

fn trigger_registry() -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    for (id, variables) in [
        (
            ORDERED_TRIGGER,
            Some(ordered_variables as fn() -> TriggerVariables),
        ),
        (NARROW_TRIGGER, Some(narrow_variables)),
        (STRING_STAMP_TRIGGER, Some(string_stamp_variables)),
        (BARE_TRIGGER, None),
    ] {
        reg.register(Box::new(FakeTrigger { id, variables }))
            .expect("each fake trigger registers under its own id");
    }
    reg
}

fn make(kind_id: &str, into: &str) -> SubActionStep {
    step(
        kind_id,
        SubActionConfig::from([(INTO_KEY.to_owned(), Variant::String(into.to_owned()))]),
        true,
    )
}

fn text(into: &str) -> SubActionStep {
    make(MAKE_TEXT, into)
}

fn disabled(mut step: SubActionStep) -> SubActionStep {
    step.enabled = false;
    step
}

fn holding(kind_id: &str, key: &str, body: &[SubActionStep]) -> SubActionStep {
    step(
        kind_id,
        SubActionConfig::from([(key.to_owned(), encode_chain(body))]),
        true,
    )
}

fn looping(body: &[SubActionStep]) -> SubActionStep {
    holding(LOOP, BODY_KEY, body)
}

fn variable(
    name: &str,
    kind: Option<VariantKind>,
    origin: ScopeOrigin,
    some_triggers_only: bool,
) -> ScopedVariable {
    ScopedVariable {
        name: name.to_owned(),
        kind,
        origin,
        some_triggers_only,
    }
}

fn earlier_text(name: &str) -> ScopedVariable {
    variable(
        name,
        Some(VariantKind::String),
        ScopeOrigin::EarlierStep,
        false,
    )
}

fn loop_local() -> ScopedVariable {
    variable(LOOP_LOCAL, None, ScopeOrigin::EnclosingBody, false)
}

fn names(scope: &[ScopedVariable]) -> Vec<&str> {
    scope
        .iter()
        .map(|variable| variable.name.as_str())
        .collect()
}

fn scope_of(
    action: &Action,
    levels: &[ScopeLevel<'_>],
    triggers: &[TriggerInstance],
) -> Vec<ScopedVariable> {
    variables_in_scope(
        action,
        levels,
        triggers,
        &sub_registry(),
        &trigger_registry(),
    )
}

fn root_scope(chain: Vec<SubActionStep>, step_index: usize) -> Vec<ScopedVariable> {
    let action = action("scope", chain);
    scope_of(
        &action,
        &[ScopeLevel {
            chain: &action.sub_actions,
            step_index,
        }],
        &[],
    )
}

fn trigger_scope(triggers: &[TriggerInstance]) -> Vec<ScopedVariable> {
    let action = action("scope", Vec::new());
    scope_of(
        &action,
        &[ScopeLevel {
            chain: &action.sub_actions,
            step_index: 0,
        }],
        triggers,
    )
}

#[test]
fn trigger_variables_keep_declaration_order_and_leave_out_legacy_aliases() {
    let scope = trigger_scope(&[trigger_of(ORDERED_TRIGGER)]);

    assert_eq!(
        scope,
        vec![
            variable(
                "zeta",
                Some(VariantKind::String),
                ScopeOrigin::Trigger,
                false
            ),
            variable("alpha", Some(VariantKind::Int), ScopeOrigin::Trigger, false),
        ]
    );
}

#[test]
fn a_name_only_some_triggers_declare_is_marked_some_triggers_only() {
    let scope = trigger_scope(&[trigger_of(ORDERED_TRIGGER), trigger_of(NARROW_TRIGGER)]);

    let marks: Vec<(&str, bool)> = scope
        .iter()
        .map(|variable| (variable.name.as_str(), variable.some_triggers_only))
        .collect();
    assert_eq!(marks, vec![("zeta", true), ("alpha", false)]);
}

#[test]
fn a_trigger_without_declared_variables_contributes_nothing() {
    assert_eq!(trigger_scope(&[trigger_of(BARE_TRIGGER)]), Vec::new());
}

#[test]
fn a_step_sees_only_the_outputs_of_the_steps_before_it() {
    let chain = || vec![text("first"), text("second"), text("third")];
    for (step_index, expected, case) in [
        (0, vec![], "editing the first step"),
        (1, vec!["first"], "editing the second step"),
        (2, vec!["first", "second"], "editing the last step"),
        (
            3,
            vec!["first", "second", "third"],
            "adding after the chain",
        ),
        (
            9,
            vec!["first", "second", "third"],
            "an index past the chain",
        ),
    ] {
        let scope = root_scope(chain(), step_index);
        assert_eq!(names(&scope), expected, "{case}");
    }
}

#[test]
fn a_disabled_step_contributes_no_outputs() {
    for (chain, case) in [
        (
            vec![disabled(text("off")), text("on")],
            "a disabled producer",
        ),
        (
            vec![
                disabled(holding(BRANCH, THEN_KEY, &[text("off")])),
                text("on"),
            ],
            "a producer nested in a disabled container",
        ),
    ] {
        let scope = root_scope(chain, 2);
        assert_eq!(names(&scope), vec!["on"], "{case}");
    }
}

#[test]
fn outputs_produced_inside_an_earlier_steps_body_are_in_scope_after_it() {
    let scope = root_scope(vec![holding(BRANCH, THEN_KEY, &[text("inner")])], 1);

    assert_eq!(scope, vec![earlier_text("inner")]);
}

#[test]
fn a_loop_local_is_out_of_scope_after_its_loop() {
    let scope = root_scope(vec![looping(&[text("inner")])], 1);

    assert_eq!(names(&scope), vec!["inner"]);
}

fn nested_chain() -> Vec<SubActionStep> {
    vec![
        text("before"),
        looping(&[text("sibling"), text("own"), text("later")]),
        text("after"),
    ]
}

fn nested_scope(action: &Action) -> Vec<ScopedVariable> {
    let body = vec![text("sibling"), text("own"), text("later")];
    scope_of(
        action,
        &[
            ScopeLevel {
                chain: &action.sub_actions,
                step_index: 1,
            },
            ScopeLevel {
                chain: &body,
                step_index: 1,
            },
        ],
        &[],
    )
}

#[test]
fn a_nested_step_sees_outer_earlier_steps_its_loop_locals_and_earlier_siblings() {
    let scope = nested_scope(&action("scope", nested_chain()));

    assert_eq!(
        scope,
        vec![
            earlier_text("before"),
            loop_local(),
            earlier_text("sibling")
        ]
    );
}

#[test]
fn a_disabled_enclosing_loop_lends_no_locals_to_its_body() {
    let mut chain = nested_chain();
    chain[1].enabled = false;

    let scope = nested_scope(&action("scope", chain));

    assert_eq!(names(&scope), vec!["before", "sibling"]);
}

fn isolated_actions() -> [(Action, &'static str); 2] {
    let concurrent = Action {
        concurrent: true,
        ..action("concurrent", nested_chain())
    };
    let random = Action {
        execution_mode: ExecutionMode::RandomPick,
        ..action("random pick", nested_chain())
    };
    [(concurrent, "concurrent"), (random, "random pick")]
}

#[test]
fn a_top_level_step_of_an_isolated_action_sees_no_sibling_outputs() {
    for (action, case) in isolated_actions() {
        let scope = scope_of(
            &action,
            &[ScopeLevel {
                chain: &action.sub_actions,
                step_index: 3,
            }],
            &[],
        );
        assert_eq!(scope, Vec::new(), "{case}");
    }
}

#[test]
fn nested_levels_of_an_isolated_action_stay_sequential() {
    for (action, case) in isolated_actions() {
        let scope = nested_scope(&action);
        assert_eq!(scope, vec![loop_local(), earlier_text("sibling")], "{case}");
    }
}

#[test]
fn a_later_producer_takes_over_the_kind_and_origin_of_a_trigger_name() {
    let action = action("scope", vec![make(MAKE_DATE, "stamp")]);

    let scope = scope_of(
        &action,
        &[ScopeLevel {
            chain: &action.sub_actions,
            step_index: 1,
        }],
        &[trigger_of(STRING_STAMP_TRIGGER), trigger_of(NARROW_TRIGGER)],
    );

    assert_eq!(
        scope.iter().find(|variable| variable.name == "stamp"),
        Some(&variable(
            "stamp",
            Some(VariantKind::Datetime),
            ScopeOrigin::EarlierStep,
            false
        ))
    );
}

#[test]
fn a_fixed_output_carries_no_kind_and_keeps_a_kind_already_known() {
    for (chain, expected, case) in [
        (
            vec![step(FIXED, SubActionConfig::new(), true)],
            None,
            "alone",
        ),
        (
            vec![
                make(MAKE_DATE, FIXED_OUT),
                step(FIXED, SubActionConfig::new(), true),
            ],
            Some(VariantKind::Datetime),
            "after a datetime producer of the same name",
        ),
    ] {
        let scope = root_scope(chain, 2);
        let kind = scope
            .iter()
            .find(|variable| variable.name == FIXED_OUT)
            .map(|variable| variable.kind);
        assert_eq!(kind, Some(expected), "{case}");
    }
}
