#![allow(clippy::expect_used)]
mod fakes;

use std::collections::HashSet;

use forge_registry::{
    ActorBlock, ActorIdentity, BREAK_LOOP_KIND_ID, CASE_CHAIN_KEY, CONTINUE_LOOP_KIND_ID, CodeKind,
    CodeLanguage, ConfigProblem, Finding, FormField, HealthSeverity, LOOP_KIND_ID, LoginSlot,
    ProducedVariable, STOP_KIND_ID, StepHealth, SubActionIo, SubActionRegistry, TriggerRegistry,
    TriggerVariables, UndecodableReason, analyze, flag_failed, flag_switched_off,
};
use forge_registry::{ActorRole, ActorSlot, CanonicalVariable};
use forge_types::{
    Action, DeclaredVariable, ExecutionMode, IntegrationId, MessageCode, MessageRef, PlatformId,
    SubActionConfig, SubActionOutcome, SubActionStep, TriggerInstance, Variant, VariantKind,
    VerbatimText,
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
        LEGACY_NAME,
        "Username",
        CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
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

const SILENT_TRIGGER: &str = "stub.silent";

fn triggers_with_a_silent_one() -> TriggerRegistry {
    let mut reg = triggers(&[(LEGACY_TRIGGER, canonical_actor_plus_legacy_username)]);
    reg.register(Box::new(FakeTrigger {
        id: SILENT_TRIGGER,
        variables: None,
    }))
    .expect("the silent trigger registers under its own id");
    reg
}

#[test]
fn beside_an_undeclared_trigger_a_declared_name_is_some_triggers_only_and_any_other_is_unverifiable()
 {
    let health = analyze(
        &action_consuming(&["%username%", "%not_declared_anywhere%"]),
        &[trigger_of(LEGACY_TRIGGER), trigger_of(SILENT_TRIGGER)],
        &[None, None],
        &flow_registry(),
        &triggers_with_a_silent_one(),
    );

    assert_eq!(
        health,
        vec![
            StepHealth {
                findings: vec![Finding::SomeTriggersOnly(LEGACY_NAME.to_owned())],
            },
            StepHealth {
                findings: vec![Finding::UnverifiableVariable(
                    "not_declared_anywhere".to_owned()
                )],
            },
        ],
    );
}

#[test]
fn with_only_undeclared_triggers_every_variable_is_unverifiable_and_only_warns() {
    let health = analyze(
        &action_consuming(&["%username%"]),
        &[
            trigger_of(SILENT_TRIGGER),
            trigger_of("stub.never_registered"),
        ],
        &[None],
        &flow_registry(),
        &triggers_with_a_silent_one(),
    );

    assert_eq!(
        health,
        vec![StepHealth {
            findings: vec![Finding::UnverifiableVariable(LEGACY_NAME.to_owned())],
        }],
    );
    assert_eq!(health[0].severity(), HealthSeverity::Yellow);
}

#[test]
fn a_failed_last_run_marks_only_its_step_broken() {
    let health = analyze(
        &action_consuming(&["plain", "plain"]),
        &[],
        &[
            Some(SubActionOutcome::Success),
            Some(SubActionOutcome::Failed(
                forge_types::UserText::unlocalized("timed out"),
            )),
        ],
        &flow_registry(),
        &triggers(&[]),
    );

    assert_eq!(
        health,
        vec![
            StepHealth::default(),
            StepHealth {
                findings: vec![Finding::LastRunFailed(forge_types::UserText::unlocalized(
                    "timed out"
                ))],
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

#[test]
fn flow_control_raises_no_finding_whether_or_not_the_action_is_concurrent() {
    for concurrent in [false, true] {
        for kind_id in FLOW_CONTROL_KINDS {
            for (placed, place) in outside_any_loop(kind_id)
                .into_iter()
                .chain(inside_a_loop(kind_id))
            {
                let health = health_without_triggers(&action_of(vec![placed], concurrent));
                assert_eq!(
                    health,
                    vec![StepHealth::default()],
                    "{kind_id} {place} concurrent={concurrent}"
                );
            }
        }
    }
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
fn a_sibling_output_is_isolated_only_in_a_random_pick_action() {
    let registry = declaring_registry(fixed_output());
    let steps = vec![plain(PRODUCER_KIND), consumer(&token())];
    let random_pick = Action {
        execution_mode: ExecutionMode::RandomPick,
        ..action("random pick", steps.clone())
    };
    for (shape, expected) in [
        (action_of(steps.clone(), true), StepHealth::default()),
        (
            random_pick,
            StepHealth {
                findings: vec![Finding::IsolatedSibling(DECLARED_VAR.to_owned())],
            },
        ),
    ] {
        let health = analyze(
            &shape,
            &[trigger_of(CANONICAL_TRIGGER)],
            &vec![None; steps.len()],
            &registry,
            &triggers(&[(CANONICAL_TRIGGER, canonical_actor_only)]),
        );
        assert_eq!(health[1], expected, "{:?}", shape.execution_mode);
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

const STAMP: &str = "stamp";
const STAMP_TRIGGER: &str = "stub.stamped";
const OTHER_STAMP_TRIGGER: &str = "stub.stamped_again";
const INT_STAMP_TRIGGER: &str = "stub.counted";
const CODE_KIND: &str = "fake.code";
const CODE_KEY: &str = "script";
const CHECKED_KIND: &str = "fake.checked";
const CHECKED_FIELD: &str = "pattern";

fn check_failure() -> ConfigProblem {
    ConfigProblem::field(CHECKED_FIELD, forge_registry::invalid_id())
}
const INTO_VAR_KEY: &str = "into_var";

fn stamp_as(kind: VariantKind) -> TriggerVariables {
    TriggerVariables::new().event_specific(
        DeclaredVariable {
            name: STAMP.to_owned(),
            kind,
            label: "Stamp".to_owned(),
            synthesis: None,
        },
        |_| Variant::Int(0),
    )
}

fn string_stamp() -> TriggerVariables {
    stamp_as(VariantKind::String)
}

fn int_stamp() -> TriggerVariables {
    stamp_as(VariantKind::Int)
}

fn datetime_stamp() -> TriggerVariables {
    stamp_as(VariantKind::Datetime)
}

fn producing(kind: VariantKind) -> SubActionIo {
    SubActionIo {
        produces: vec![ProducedVariable {
            output_name_key: INTO_VAR_KEY.to_owned(),
            kind,
            label: "Output".to_owned(),
        }],
        ..SubActionIo::default()
    }
}

fn stamp_fixed_output() -> SubActionIo {
    SubActionIo {
        fixed_outputs: vec![STAMP.to_owned()],
        ..SubActionIo::default()
    }
}

fn stamp_body_local() -> SubActionIo {
    SubActionIo {
        body_locals: vec![STAMP.to_owned()],
        ..SubActionIo::default()
    }
}

fn produce_stamp() -> SubActionStep {
    step(
        PRODUCER_KIND,
        SubActionConfig::from([(INTO_VAR_KEY.to_owned(), Variant::String(STAMP.to_owned()))]),
        true,
    )
}

fn date_registry(io: SubActionIo) -> SubActionRegistry {
    let mut reg = declaring_registry(io);
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
        fakes::FakeRunner {
            fields: vec![FormField::Code {
                key: CODE_KEY,
                label: CODE_KEY,
                language: CodeLanguage::Rhai,
                kind: CodeKind::Script,
            }],
            ..runner(CODE_KIND)
        },
        fakes::FakeRunner {
            config_error: Some(check_failure()),
            checks_while_editing: true,
            ..runner(CHECKED_KIND)
        },
    ] {
        reg.register(Box::new(nesting))
            .expect("each date-analysis fake registers under its own id");
    }
    reg
}

fn stamp_triggers() -> TriggerRegistry {
    triggers(&[
        (STAMP_TRIGGER, string_stamp),
        (OTHER_STAMP_TRIGGER, datetime_stamp),
        (INT_STAMP_TRIGGER, int_stamp),
        (CANONICAL_TRIGGER, canonical_actor_only),
    ])
}

fn health_with(
    steps: Vec<SubActionStep>,
    instances: &[TriggerInstance],
    io: SubActionIo,
    trigger_registry: &TriggerRegistry,
) -> Vec<StepHealth> {
    analyze(
        &action("dates", steps.clone()),
        instances,
        &vec![None; steps.len()],
        &date_registry(io),
        trigger_registry,
    )
}

fn findings(list: Vec<Finding>) -> StepHealth {
    StepHealth { findings: list }
}

fn not_datetime() -> StepHealth {
    findings(vec![Finding::DateFormatOnNonDatetime(STAMP.to_owned())])
}

fn date_reason(name: &'static str) -> MessageRef {
    MessageRef::new(MessageCode::from_static("condition", name))
}

fn replaced_spelling(spelling: &str, replacement: &str) -> MessageRef {
    date_reason("date_pattern_replaced_spelling")
        .with_arg("spelling", VerbatimText::new(spelling))
        .with_arg("replacement", VerbatimText::new(replacement))
        .with_arg("position", 0_usize)
}

fn pattern_reason(pattern: &str) -> MessageRef {
    match pattern {
        "qq" => date_reason("date_pattern_unsupported_letter")
            .with_arg("letter", VerbatimText::new("q"))
            .with_arg("position", 0_usize),
        "'HH" => date_reason("date_pattern_unterminated_quote").with_arg("position", 0_usize),
        "HHH" => date_reason("date_pattern_unsupported_length")
            .with_arg("letter", VerbatimText::new("H"))
            .with_arg("spelling", VerbatimText::new("HHH"))
            .with_arg("position", 0_usize),
        "dddd" => replaced_spelling("dddd", "EEEE"),
        "ddd" => replaced_spelling("ddd", "EEE"),
        "tt" => replaced_spelling("tt", "a"),
        _ => date_reason("date_pattern_empty"),
    }
}

fn invalid_pattern(pattern: &str) -> Finding {
    Finding::InvalidDatePattern {
        name: STAMP.to_owned(),
        pattern: pattern.to_owned(),
        reason: pattern_reason(pattern),
    }
}

fn formatted(pattern: &str) -> String {
    format!("%{STAMP}:{pattern}%")
}

#[test]
fn variable_findings_read_tokens_the_way_interpolation_does() {
    for (template, expected, case) in [
        (
            "%stamp:HH:mm%",
            StepHealth::default(),
            "a formatted token resolves the name before its first colon",
        ),
        (
            "% not_declared_anywhere %",
            StepHealth::default(),
            "a token padded with spaces is literal text",
        ),
        (
            "%not_declared_anywhere:HH%",
            findings(vec![Finding::UnknownVariable(
                "not_declared_anywhere".to_owned(),
            )]),
            "a formatted token naming nothing is only an unknown variable",
        ),
    ] {
        let health = health_with(
            vec![consumer(template)],
            &[trigger_of(OTHER_STAMP_TRIGGER)],
            SubActionIo::default(),
            &stamp_triggers(),
        );
        assert_eq!(health, vec![expected], "{case}");
    }
}

#[test]
fn a_code_field_is_scanned_neither_for_variables_nor_for_date_patterns() {
    let script = step(
        CODE_KIND,
        SubActionConfig::from([(
            CODE_KEY.to_owned(),
            Variant::String("%not_declared_anywhere% %stamp:qq%".to_owned()),
        )]),
        true,
    );

    let health = health_with(
        vec![script],
        &[trigger_of(OTHER_STAMP_TRIGGER)],
        SubActionIo::default(),
        &stamp_triggers(),
    );

    assert_eq!(health, vec![StepHealth::default()]);
}

#[test]
fn a_date_format_on_a_variable_every_source_declares_non_datetime_warns() {
    for (steps, instances, io, case) in [
        (
            vec![consumer(&formatted("HH:mm"))],
            vec![trigger_of(STAMP_TRIGGER)],
            SubActionIo::default(),
            "a string trigger variable",
        ),
        (
            vec![consumer(&formatted("HH:mm"))],
            vec![trigger_of(INT_STAMP_TRIGGER)],
            SubActionIo::default(),
            "an integer trigger variable",
        ),
        (
            vec![produce_stamp(), consumer(&formatted("HH:mm"))],
            vec![trigger_of(CANONICAL_TRIGGER)],
            producing(VariantKind::String),
            "a step output declared as a string",
        ),
    ] {
        let health = health_with(steps, &instances, io, &stamp_triggers());
        assert_eq!(health.last(), Some(&not_datetime()), "{case}");
    }
}

#[test]
fn a_date_format_on_a_variable_some_source_may_hold_a_datetime_is_silent() {
    let consume = || consumer(&formatted("HH:mm"));
    for (steps, instances, io, case) in [
        (
            vec![consume()],
            vec![trigger_of(OTHER_STAMP_TRIGGER)],
            SubActionIo::default(),
            "a datetime trigger variable",
        ),
        (
            vec![produce_stamp(), consume()],
            vec![trigger_of(CANONICAL_TRIGGER)],
            producing(VariantKind::Datetime),
            "a step output declared as a datetime",
        ),
        (
            vec![produce_stamp(), consume()],
            vec![trigger_of(STAMP_TRIGGER)],
            producing(VariantKind::Datetime),
            "a string trigger variable a step overwrites with a datetime",
        ),
        (
            vec![consume()],
            vec![trigger_of(STAMP_TRIGGER), trigger_of(OTHER_STAMP_TRIGGER)],
            SubActionIo::default(),
            "a string from one trigger and a datetime from another",
        ),
        (
            vec![plain(PRODUCER_KIND), consume()],
            vec![trigger_of(CANONICAL_TRIGGER)],
            stamp_fixed_output(),
            "a fixed output with no declared kind",
        ),
        (
            vec![nesting(SCOPED_KIND, SCOPED_BODY_KEY, &[consume()])],
            vec![trigger_of(CANONICAL_TRIGGER)],
            stamp_body_local(),
            "a body local with no declared kind",
        ),
    ] {
        let health = health_with(steps, &instances, io, &stamp_triggers());
        assert_eq!(health.last(), Some(&StepHealth::default()), "{case}");
    }
}

#[test]
fn a_non_datetime_variable_formatted_twice_in_one_step_warns_once() {
    let health = health_with(
        vec![consumer("%stamp:HH% at %stamp:dd.MM%")],
        &[trigger_of(STAMP_TRIGGER)],
        SubActionIo::default(),
        &stamp_triggers(),
    );

    assert_eq!(health, vec![not_datetime()]);
}

#[test]
fn a_date_format_mismatch_is_not_judged_without_a_trigger_seed() {
    let health = health_with(
        vec![produce_stamp(), consumer(&formatted("HH:mm"))],
        &[],
        producing(VariantKind::String),
        &stamp_triggers(),
    );

    assert_eq!(health, vec![StepHealth::default(); 2]);
}

#[test]
fn an_invalid_inline_date_pattern_is_reported_with_or_without_triggers() {
    for pattern in ["qq", "'HH", "HHH", "dddd", "ddd", "tt", ""] {
        for (instances, seeded) in [
            (Vec::new(), "without triggers"),
            (
                vec![trigger_of(OTHER_STAMP_TRIGGER)],
                "with a declaring trigger",
            ),
        ] {
            let health = health_with(
                vec![consumer(&formatted(pattern))],
                &instances,
                SubActionIo::default(),
                &stamp_triggers(),
            );
            assert_eq!(
                health,
                vec![findings(vec![invalid_pattern(pattern)])],
                "{pattern:?} {seeded}"
            );
        }
    }
}

#[test]
fn an_invalid_pattern_on_a_non_datetime_variable_reports_both_findings() {
    let health = health_with(
        vec![consumer(&formatted("qq"))],
        &[trigger_of(STAMP_TRIGGER)],
        SubActionIo::default(),
        &stamp_triggers(),
    );

    assert_eq!(
        health,
        vec![findings(vec![
            Finding::DateFormatOnNonDatetime(STAMP.to_owned()),
            invalid_pattern("qq"),
        ])]
    );
}

#[test]
fn a_nested_invalid_date_pattern_is_reported_once_on_its_top_level_step() {
    let bad = || consumer(&formatted("qq"));
    for (placed, place) in [
        (branch(vec![bad()]), "inside an if branch"),
        (inside_loop(vec![bad()]), "inside a loop body"),
        (inside_case(vec![bad()]), "inside a switch case"),
        (inside_default(vec![bad()]), "inside a switch default"),
        (
            inside_case(vec![branch(vec![bad()])]),
            "inside an if nested in a switch case",
        ),
    ] {
        let health = health_with(
            vec![consumer("fine"), placed],
            &[],
            SubActionIo::default(),
            &stamp_triggers(),
        );
        assert_eq!(
            health,
            vec![StepHealth::default(), findings(vec![invalid_pattern("qq")])],
            "{place}"
        );
    }
}

#[test]
fn an_opted_in_runner_failing_validation_reports_invalid_step_config_keyed_to_its_field() {
    let failing = || plain(CHECKED_KIND);
    let expected = findings(vec![Finding::InvalidStepConfig(check_failure())]);
    for (placed, place) in [
        (failing(), "at the top level"),
        (branch(vec![failing()]), "inside an if branch"),
    ] {
        let health = health_with(vec![placed], &[], SubActionIo::default(), &stamp_triggers());
        assert_eq!(health, vec![expected.clone()], "{place}");
    }
}

#[test]
fn a_runner_that_does_not_opt_in_is_not_validated_while_editing() {
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(fakes::FakeRunner {
        config_error: Some(check_failure()),
        ..runner(CHECKED_KIND)
    }))
    .expect("the failing fake registers");

    let health = analyze(
        &action("unchecked", vec![plain(CHECKED_KIND)]),
        &[],
        &[None],
        &reg,
        &triggers(&[]),
    );

    assert_eq!(health, vec![StepHealth::default()]);
}

#[test]
fn a_disabled_step_raises_no_date_pattern_or_step_config_finding() {
    let off = |mut step: SubActionStep| {
        step.enabled = false;
        step
    };
    for (placed, place) in [
        (off(consumer(&formatted("qq"))), "a disabled consumer"),
        (off(plain(CHECKED_KIND)), "a disabled opted-in step"),
        (
            branch(vec![off(consumer(&formatted("qq")))]),
            "a disabled consumer inside a live branch",
        ),
        (
            off(branch(vec![
                consumer(&formatted("qq")),
                plain(CHECKED_KIND),
            ])),
            "live steps inside a disabled branch",
        ),
    ] {
        let health = health_with(vec![placed], &[], SubActionIo::default(), &stamp_triggers());
        assert_eq!(health, vec![StepHealth::default()], "{place}");
    }
}

#[test]
fn broken_patterns_configs_and_unreadable_steps_break_the_step_while_a_kind_mismatch_warns() {
    for (finding, severity) in [
        (invalid_pattern("qq"), HealthSeverity::Red),
        (
            Finding::InvalidStepConfig(check_failure()),
            HealthSeverity::Red,
        ),
        (
            Finding::UndecodableStep {
                position: 0,
                reason: UndecodableReason::MissingKind,
            },
            HealthSeverity::Red,
        ),
        (
            Finding::DateFormatOnNonDatetime(STAMP.to_owned()),
            HealthSeverity::Yellow,
        ),
    ] {
        assert_eq!(
            findings(vec![finding.clone()]).severity(),
            severity,
            "{finding:?}"
        );
    }
}

#[test]
fn an_unreadable_nested_step_is_reported_on_its_top_level_step_only_when_that_step_is_enabled() {
    let branch_holding_garbage = |enabled| {
        let mut chain = encode_chain(&[plain(CORE_LOG)]);
        if let Variant::Array(entries) = &mut chain {
            entries.push(Variant::String("not a step".to_owned()));
        }
        step(
            BRANCH_KIND,
            SubActionConfig::from([(THEN_CHAIN_KEY.to_owned(), chain)]),
            enabled,
        )
    };

    let health = analyze(
        &action(
            "stub action",
            vec![branch_holding_garbage(true), branch_holding_garbage(false)],
        ),
        &[],
        &[None, None],
        &flow_registry(),
        &TriggerRegistry::new(),
    );

    assert_eq!(
        health,
        vec![
            StepHealth {
                findings: vec![Finding::UndecodableStep {
                    position: 1,
                    reason: UndecodableReason::NotAnObject,
                }],
            },
            StepHealth::default(),
        ]
    );
}
