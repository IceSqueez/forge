#![allow(clippy::expect_used)]

mod fakes;

use std::collections::HashSet;

use forge_registry::{
    ActorBlock, ActorIdentity, BREAK_LOOP_KIND_ID, CASE_CHAIN_KEY, CONTINUE_LOOP_KIND_ID, Finding,
    FormField, HealthSeverity, LOOP_KIND_ID, LoginSlot, STOP_KIND_ID, StepHealth, SubActionIo,
    SubActionRegistry, TriggerRegistry, TriggerVariables, analyze, flag_failed, flag_switched_off,
};
use forge_types::{
    Action, ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, IntegrationId, PlatformId,
    SubActionConfig, SubActionOutcome, SubActionStep, Variant, VariantKind,
};

use fakes::{FakeTrigger, action, encode_chain, runner, step, trigger_of};

const BRANCH_KIND: &str = "fake.branch";
const THEN_CHAIN_KEY: &str = "then_chain";
const LOOP_BODY_KEY: &str = "body";
const SWITCH_KIND: &str = "fake.switch";
const CASES_KEY: &str = "cases";
const DEFAULT_CHAIN_KEY: &str = "default_chain";
const CONSUMER_KIND: &str = "chat.send";
const MESSAGE_KEY: &str = "message";
const LEGACY_TRIGGER: &str = "stub.chat_with_legacy";
const CANONICAL_TRIGGER: &str = "stub.chat_canonical";
const LEGACY_NAME: &str = "username";
const PRODUCER_KIND: &str = "fake.producer";
const SCOPED_KIND: &str = "fake.scoped";
const SCOPED_BODY_KEY: &str = "scoped_body";
const DECLARED_VAR: &str = "probe.result";
const FLOW_CONTROL_KINDS: [&str; 3] = [BREAK_LOOP_KIND_ID, CONTINUE_LOOP_KIND_ID, STOP_KIND_ID];
const OBS_SCENE: &str = "obs.scene.set";
const TWITCH_SEND: &str = "twitch.chat.send";
const CORE_LOG: &str = "core.log";

fn sub_chain(key: &'static str) -> FormField {
    FormField::SubChain { key, label: key }
}

fn flow_registry() -> SubActionRegistry {
    let mut reg = SubActionRegistry::new();
    for nesting in [
        fakes::FakeRunner {
            fields: vec![sub_chain(THEN_CHAIN_KEY)],
            ..runner(BRANCH_KIND)
        },
        fakes::FakeRunner {
            fields: vec![sub_chain(LOOP_BODY_KEY)],
            ..runner(LOOP_KIND_ID)
        },
        fakes::FakeRunner {
            fields: vec![
                FormField::CaseList {
                    key: CASES_KEY,
                    label: CASES_KEY,
                },
                sub_chain(DEFAULT_CHAIN_KEY),
            ],
            ..runner(SWITCH_KIND)
        },
    ] {
        reg.register(Box::new(nesting))
            .expect("each nesting fake registers under its own id");
    }
    reg
}

fn chatter(_event: &forge_events::Event) -> ActorIdentity {
    ActorIdentity {
        id: "51938264".to_owned(),
        display_name: "StreamFan42".to_owned(),
        login: Some("streamfan42".to_owned()),
    }
}

fn canonical_actor_only() -> TriggerVariables {
    TriggerVariables::new().actor(
        ActorBlock {
            role: ActorRole::Principal,
            platform: PlatformId::Kick,
            login: LoginSlot::Declared,
        },
        chatter,
    )
}

fn canonical_actor_plus_legacy_username() -> TriggerVariables {
    canonical_actor_only().legacy(
        DeclaredVariable {
            name: LEGACY_NAME.to_owned(),
            kind: VariantKind::String,
            label: "Username".to_owned(),
            synthesis: None,
        },
        CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
        |_| Variant::String("streamfan42".to_owned()),
    )
}

type Declaring = (&'static str, fn() -> TriggerVariables);

fn triggers(declaring: &[Declaring]) -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    for (id, variables) in declaring {
        reg.register(Box::new(FakeTrigger {
            id,
            variables: Some(*variables),
        }))
        .expect("each fake trigger registers under its own id");
    }
    reg
}

fn consumer(template: &str) -> SubActionStep {
    step(
        CONSUMER_KIND,
        SubActionConfig::from([(MESSAGE_KEY.to_owned(), Variant::String(template.to_owned()))]),
        true,
    )
}

fn action_consuming(templates: &[&str]) -> Action {
    action(
        "stub action",
        templates.iter().map(|t| consumer(t)).collect(),
    )
}

fn nesting(kind_id: &str, key: &str, body: &[SubActionStep]) -> SubActionStep {
    step(
        kind_id,
        SubActionConfig::from([(key.to_owned(), encode_chain(body))]),
        true,
    )
}

fn branch(body: Vec<SubActionStep>) -> SubActionStep {
    nesting(BRANCH_KIND, THEN_CHAIN_KEY, &body)
}

fn inside_loop(body: Vec<SubActionStep>) -> SubActionStep {
    nesting(LOOP_KIND_ID, LOOP_BODY_KEY, &body)
}

fn inside_case(body: Vec<SubActionStep>) -> SubActionStep {
    let case = SubActionConfig::from([
        ("match".to_owned(), Variant::String("a".to_owned())),
        (CASE_CHAIN_KEY.to_owned(), encode_chain(&body)),
    ]);
    step(
        SWITCH_KIND,
        SubActionConfig::from([(
            CASES_KEY.to_owned(),
            Variant::Array(vec![Variant::Object(case)]),
        )]),
        true,
    )
}

fn inside_default(body: Vec<SubActionStep>) -> SubActionStep {
    nesting(SWITCH_KIND, DEFAULT_CHAIN_KEY, &body)
}

fn plain(kind_id: &str) -> SubActionStep {
    step(kind_id, SubActionConfig::new(), true)
}

#[test]
fn a_legacy_name_a_step_consumes_is_never_reported_as_unknown() {
    let health = analyze(
        &action_consuming(&["%username%", "%not_declared_anywhere%"]),
        &[trigger_of(LEGACY_TRIGGER)],
        &[None, None],
        &flow_registry(),
        &triggers(&[(LEGACY_TRIGGER, canonical_actor_plus_legacy_username)]),
    );

    assert_eq!(
        health,
        vec![
            StepHealth::default(),
            StepHealth {
                findings: vec![Finding::UnknownVariable("not_declared_anywhere".to_owned())],
            },
        ],
    );
}

#[test]
fn a_legacy_name_only_one_of_two_triggers_declares_is_reported_as_some_triggers_only() {
    let health = analyze(
        &action_consuming(&["%username%"]),
        &[trigger_of(LEGACY_TRIGGER), trigger_of(CANONICAL_TRIGGER)],
        &[None],
        &flow_registry(),
        &triggers(&[
            (LEGACY_TRIGGER, canonical_actor_plus_legacy_username),
            (CANONICAL_TRIGGER, canonical_actor_only),
        ]),
    );

    assert_eq!(
        health,
        vec![StepHealth {
            findings: vec![Finding::SomeTriggersOnly(LEGACY_NAME.to_owned())],
        }],
    );
}

#[test]
fn a_failed_last_run_marks_only_its_step_broken() {
    let health = analyze(
        &action_consuming(&["plain", "plain"]),
        &[],
        &[
            Some(SubActionOutcome::Success),
            Some(SubActionOutcome::Failed("timed out".to_owned())),
        ],
        &flow_registry(),
        &triggers(&[]),
    );

    assert_eq!(
        health,
        vec![
            StepHealth::default(),
            StepHealth {
                findings: vec![Finding::LastRunFailed("timed out".to_owned())],
            },
        ],
    );
    assert_eq!(health[1].severity(), HealthSeverity::Red);
}

fn outside_any_loop(kind_id: &str) -> Vec<(SubActionStep, &'static str)> {
    vec![
        (plain(kind_id), "at the top level"),
        (branch(vec![plain(kind_id)]), "inside an if branch"),
        (inside_case(vec![plain(kind_id)]), "inside a switch case"),
        (
            inside_default(vec![plain(kind_id)]),
            "inside a switch default",
        ),
        (
            inside_case(vec![branch(vec![plain(kind_id)])]),
            "inside an if nested in a switch case",
        ),
    ]
}

fn inside_a_loop(kind_id: &str) -> Vec<(SubActionStep, &'static str)> {
    vec![
        (inside_loop(vec![plain(kind_id)]), "inside a loop body"),
        (
            inside_loop(vec![branch(vec![plain(kind_id)])]),
            "inside an if nested in a loop",
        ),
    ]
}

fn action_of(steps: Vec<SubActionStep>, concurrent: bool) -> Action {
    Action {
        concurrent,
        ..action("stub action", steps)
    }
}

fn health_without_triggers(action: &Action) -> Vec<StepHealth> {
    analyze(
        action,
        &[],
        &vec![None; action.sub_actions.len()],
        &flow_registry(),
        &triggers(&[]),
    )
}

fn concurrent_warning() -> StepHealth {
    StepHealth {
        findings: vec![Finding::ControlFlowInConcurrentAction],
    }
}

#[test]
fn flow_control_no_loop_absorbs_in_a_concurrent_action_flags_its_top_level_step() {
    let flagged = FLOW_CONTROL_KINDS
        .into_iter()
        .flat_map(|kind_id| {
            outside_any_loop(kind_id)
                .into_iter()
                .map(move |p| (kind_id, p))
        })
        .chain(
            inside_a_loop(STOP_KIND_ID)
                .into_iter()
                .map(|p| (STOP_KIND_ID, p)),
        );
    for (kind_id, (placed, place)) in flagged {
        let health = health_without_triggers(&action_of(vec![placed], true));
        assert_eq!(health, vec![concurrent_warning()], "{kind_id} {place}");
    }
}

#[test]
fn flow_control_in_a_sequential_action_raises_no_finding() {
    for kind_id in FLOW_CONTROL_KINDS {
        for (placed, place) in outside_any_loop(kind_id)
            .into_iter()
            .chain(inside_a_loop(kind_id))
        {
            let health = health_without_triggers(&action_of(vec![placed], false));
            assert_eq!(health, vec![StepHealth::default()], "{kind_id} {place}");
        }
    }
}

#[test]
fn a_loop_break_or_continue_in_a_concurrent_action_is_not_flagged() {
    for kind_id in [BREAK_LOOP_KIND_ID, CONTINUE_LOOP_KIND_ID] {
        for (placed, place) in inside_a_loop(kind_id) {
            let health = health_without_triggers(&action_of(vec![placed], true));
            assert_eq!(health, vec![StepHealth::default()], "{kind_id} {place}");
        }
    }
}

#[test]
fn flow_control_in_a_concurrent_action_is_flagged_whether_or_not_triggers_seed_it() {
    let action = action_of(vec![plain(STOP_KIND_ID)], true);
    for (instances, seeded) in [
        (Vec::new(), "no triggers"),
        (vec![trigger_of(CANONICAL_TRIGGER)], "a declaring trigger"),
    ] {
        let health = analyze(
            &action,
            &instances,
            &[None],
            &flow_registry(),
            &triggers(&[(CANONICAL_TRIGGER, canonical_actor_only)]),
        );
        assert_eq!(health, vec![concurrent_warning()], "{seeded}");
    }
}

#[test]
fn a_concurrent_action_flags_only_steps_that_hold_flow_control() {
    let health = health_without_triggers(&action_of(
        vec![
            consumer("hi"),
            branch(vec![consumer("hi")]),
            branch(vec![plain(STOP_KIND_ID)]),
        ],
        true,
    ));

    assert_eq!(
        health,
        vec![
            StepHealth::default(),
            StepHealth::default(),
            concurrent_warning()
        ]
    );
}

#[test]
fn disabled_flow_control_in_a_concurrent_action_raises_no_finding() {
    let disabled_stop = step(STOP_KIND_ID, SubActionConfig::new(), false);
    let mut disabled_branch = branch(vec![plain(STOP_KIND_ID)]);
    disabled_branch.enabled = false;

    let health = health_without_triggers(&action_of(
        vec![
            disabled_stop.clone(),
            disabled_branch,
            branch(vec![disabled_stop]),
        ],
        true,
    ));

    assert_eq!(health, vec![StepHealth::default(); 3]);
}

#[test]
fn flow_control_in_a_concurrent_action_warns_without_marking_the_step_broken() {
    let health = health_without_triggers(&action_of(vec![plain(STOP_KIND_ID)], true));

    assert_eq!(health[0].severity(), HealthSeverity::Yellow);
}

fn declaring_registry(io: SubActionIo) -> SubActionRegistry {
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(fakes::FakeRunner {
        io: io.clone(),
        ..runner(PRODUCER_KIND)
    }))
    .expect("the producing fake registers");
    reg.register(Box::new(fakes::FakeRunner {
        fields: vec![sub_chain(SCOPED_BODY_KEY)],
        io,
        ..runner(SCOPED_KIND)
    }))
    .expect("the scoping fake registers");
    reg
}

fn fixed_output() -> SubActionIo {
    SubActionIo {
        fixed_outputs: vec![DECLARED_VAR.to_owned()],
        ..SubActionIo::default()
    }
}

fn body_local() -> SubActionIo {
    SubActionIo {
        body_locals: vec![DECLARED_VAR.to_owned()],
        ..SubActionIo::default()
    }
}

fn seeded_health(steps: Vec<SubActionStep>, registry: &SubActionRegistry) -> Vec<StepHealth> {
    analyze(
        &action("declarations", steps.clone()),
        &[trigger_of(CANONICAL_TRIGGER)],
        &vec![None; steps.len()],
        registry,
        &triggers(&[(CANONICAL_TRIGGER, canonical_actor_only)]),
    )
}

fn unknown_declared() -> StepHealth {
    StepHealth {
        findings: vec![Finding::UnknownVariable(DECLARED_VAR.to_owned())],
    }
}

fn token() -> String {
    format!("%{DECLARED_VAR}%")
}

#[test]
fn a_declared_fixed_output_is_in_scope_for_the_steps_after_its_producer() {
    for (io, expected, declared) in [
        (fixed_output(), StepHealth::default(), "as a fixed output"),
        (body_local(), unknown_declared(), "only as a body local"),
        (SubActionIo::default(), unknown_declared(), "nowhere"),
    ] {
        let health = seeded_health(
            vec![plain(PRODUCER_KIND), consumer(&token())],
            &declaring_registry(io),
        );
        assert_eq!(health[1], expected, "the name is declared {declared}");
    }
}

#[test]
fn a_declared_fixed_output_consumed_before_its_producer_is_produced_later() {
    let health = seeded_health(
        vec![consumer(&token()), plain(PRODUCER_KIND)],
        &declaring_registry(fixed_output()),
    );

    assert_eq!(
        health[0],
        StepHealth {
            findings: vec![Finding::ProducedLater(DECLARED_VAR.to_owned())],
        }
    );
}

#[test]
fn a_declared_body_local_is_in_scope_only_inside_its_steps_body() {
    for (io, expected, declared) in [
        (body_local(), StepHealth::default(), "as a body local"),
        (
            fixed_output(),
            StepHealth {
                findings: vec![Finding::ProducedLater(DECLARED_VAR.to_owned())],
            },
            "only as a fixed output",
        ),
        (SubActionIo::default(), unknown_declared(), "nowhere"),
    ] {
        let health = seeded_health(
            vec![nesting(SCOPED_KIND, SCOPED_BODY_KEY, &[consumer(&token())])],
            &declaring_registry(io),
        );
        assert_eq!(health[0], expected, "the name is declared {declared}");
    }
}

#[test]
fn a_declared_body_local_is_out_of_scope_after_its_step() {
    let health = seeded_health(
        vec![
            nesting(SCOPED_KIND, SCOPED_BODY_KEY, &[]),
            consumer(&token()),
        ],
        &declaring_registry(body_local()),
    );

    assert_eq!(health[1], unknown_declared());
}

fn obs() -> IntegrationId {
    IntegrationId::new("obs")
}

fn owned_registry() -> SubActionRegistry {
    let mut registry = SubActionRegistry::new();
    registry
        .register(Box::new(fakes::FakeRunner {
            fields: vec![sub_chain(THEN_CHAIN_KEY)],
            ..runner(BRANCH_KIND)
        }))
        .expect("the container fake registers");
    for (kind, owner) in [(OBS_SCENE, "obs"), (TWITCH_SEND, "twitch")] {
        registry
            .owned_by(IntegrationId::from_static(owner))
            .register(Box::new(runner(kind)))
            .expect("every owned fake carries its own id");
    }
    registry
}

fn container(body: Vec<SubActionStep>, enabled: bool) -> SubActionStep {
    SubActionStep {
        enabled,
        ..branch(body)
    }
}

fn owned(kind_id: &str, enabled: bool) -> SubActionStep {
    step(kind_id, SubActionConfig::new(), enabled)
}

type Flag = fn(&[SubActionStep], &SubActionRegistry, &HashSet<IntegrationId>, &mut [StepHealth]);
type Owner = fn(&StepHealth) -> Option<&IntegrationId>;

const GATES: [(Flag, Owner, &str); 2] = [
    (
        flag_switched_off,
        StepHealth::disabled_integration,
        "switched off",
    ),
    (flag_failed, StepHealth::failed_integration, "failed"),
];

fn flagged_owners(
    flag: Flag,
    owner: Owner,
    steps: &[SubActionStep],
    gated: &[IntegrationId],
) -> Vec<Option<IntegrationId>> {
    let mut health = vec![StepHealth::default(); steps.len()];
    flag(
        steps,
        &owned_registry(),
        &gated.iter().cloned().collect(),
        &mut health,
    );
    health.iter().map(|step| owner(step).cloned()).collect()
}

#[test]
fn only_an_enabled_step_reaching_a_gated_integration_is_flagged() {
    for (steps, expected, case) in [
        (
            vec![owned(CORE_LOG, true), owned(OBS_SCENE, true)],
            vec![None, Some(obs())],
            "an enabled step the gated integration owns",
        ),
        (vec![owned(OBS_SCENE, false)], vec![None], "a disabled step"),
        (
            vec![container(vec![owned(OBS_SCENE, true)], true)],
            vec![Some(obs())],
            "a step nested under an enabled container flags the top step",
        ),
        (
            vec![container(vec![owned(OBS_SCENE, true)], false)],
            vec![None],
            "a step nested under a disabled container",
        ),
        (
            vec![container(vec![owned(OBS_SCENE, false)], true)],
            vec![None],
            "a disabled step nested under an enabled container",
        ),
        (
            vec![owned(TWITCH_SEND, true)],
            vec![None],
            "a step owned by an integration that is not gated",
        ),
        (
            vec![
                owned(CORE_LOG, true),
                container(vec![owned(CORE_LOG, true)], true),
            ],
            vec![None, None],
            "steps no integration owns",
        ),
    ] {
        for (flag, owner, gate) in GATES {
            assert_eq!(
                flagged_owners(flag, owner, &steps, &[obs()]),
                expected,
                "{gate}: {case}"
            );
        }
    }
}

#[test]
fn each_gate_raises_its_own_finding_and_not_the_other() {
    let steps = [owned(OBS_SCENE, true)];

    assert_eq!(
        [
            flagged_owners(
                flag_switched_off,
                StepHealth::failed_integration,
                &steps,
                &[obs()]
            ),
            flagged_owners(
                flag_failed,
                StepHealth::disabled_integration,
                &steps,
                &[obs()]
            ),
        ],
        [vec![None], vec![None]]
    );
}

#[test]
fn a_step_hit_by_a_gated_integration_warns() {
    for (flag, _, gate) in GATES {
        let mut health = vec![StepHealth::default()];
        flag(
            &[owned(OBS_SCENE, true)],
            &owned_registry(),
            &HashSet::from([obs()]),
            &mut health,
        );
        assert_eq!(health[0].severity(), HealthSeverity::Yellow, "{gate}");
    }
}

#[test]
fn analysis_without_a_trigger_seed_reports_no_variable_findings() {
    let health = analyze(
        &action_consuming(&["%not_declared_anywhere%"]),
        &[],
        &[None],
        &flow_registry(),
        &triggers(&[]),
    );

    assert_eq!(health, vec![StepHealth::default()]);
}
