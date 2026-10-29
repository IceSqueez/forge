#![allow(clippy::expect_used, clippy::unwrap_used)]
#[expect(dead_code)]
mod fakes;

use std::path::PathBuf;

use forge_registry::{
    CASE_CHAIN_KEY, ChainEntry, FormField, StepNode, StepReach, SubActionRegistry,
    UndecodableReason, UndecodableStep, decode_chain, decode_step, encode_chain, encode_step_onto,
    nested_chains, rewrite_steps, walk_steps,
};
use forge_types::{SubActionConfig, SubActionStep, Variant};

use fakes::{FakeRunner, runner, step};

const BRANCH_KIND: &str = "fake.branch";
const RENAMED_BRANCH_KIND: &str = "fake.branch.renamed";
const SWITCH_KIND: &str = "fake.switch";
const OPTIONAL_KIND: &str = "fake.optional";
const THEN_KEY: &str = "then_chain";
const ELSE_KEY: &str = "else_chain";
const CASES_KEY: &str = "cases";
const DEFAULT_KEY: &str = "default_chain";
const OPTIONAL_BODY_KEY: &str = "body";
const OLD_LEAF: &str = "leaf.old";
const NEW_LEAF: &str = "leaf.new";

const DRIFT_CASES: &[(&str, &str)] = &[
    ("nested_chains.json", "nested_chains.resaved.json"),
    ("canonical_chains.json", "canonical_chains.json"),
    ("malformed_mixed.json", "malformed_mixed.json"),
];

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/step_tree")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_default()
}

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn cfg<const N: usize>(fields: [(&str, Variant); N]) -> SubActionConfig {
    fields
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn obj<const N: usize>(fields: [(&str, Variant); N]) -> Variant {
    Variant::Object(cfg(fields))
}

fn entry(kind_id: &str, enabled: bool, config: Variant) -> Variant {
    obj([
        ("kind_id", s(kind_id)),
        ("enabled", Variant::Bool(enabled)),
        ("config", config),
    ])
}

fn leaf(kind_id: &str) -> Variant {
    entry(kind_id, true, obj([]))
}

fn top<const N: usize>(
    kind_id: &str,
    enabled: bool,
    fields: [(&str, Variant); N],
) -> SubActionStep {
    step(kind_id, cfg(fields), enabled)
}

fn registry() -> SubActionRegistry {
    let sub_chain = |key| FormField::SubChain { key, label: key };
    let mut registry = SubActionRegistry::new();
    for nesting in [
        FakeRunner {
            fields: vec![sub_chain(THEN_KEY), sub_chain(ELSE_KEY)],
            ..runner(BRANCH_KIND)
        },
        FakeRunner {
            fields: vec![sub_chain(THEN_KEY)],
            ..runner(RENAMED_BRANCH_KIND)
        },
        FakeRunner {
            fields: vec![
                FormField::CaseList {
                    key: CASES_KEY,
                    label: CASES_KEY,
                },
                sub_chain(DEFAULT_KEY),
            ],
            ..runner(SWITCH_KIND)
        },
        FakeRunner {
            fields: vec![FormField::Optional {
                key: "use_body",
                label: "use_body",
                inner: Box::new(sub_chain(OPTIONAL_BODY_KEY)),
            }],
            ..runner(OPTIONAL_KIND)
        },
    ] {
        registry
            .register(Box::new(nesting))
            .expect("each nesting fake registers under its own id");
    }
    registry
}

fn visited(steps: &[SubActionStep], reach: StepReach) -> Vec<String> {
    let mut order = Vec::new();
    walk_steps(steps, &registry(), reach, &mut |node| {
        order.push(match node {
            StepNode::Step(step) => step.kind_id.clone(),
            StepNode::Undecodable(undecodable) => format!("#{}", undecodable.position),
        });
    });
    order
}

fn rename(kind_id: &mut String, _: &mut SubActionConfig) -> bool {
    let renamed = match kind_id.as_str() {
        BRANCH_KIND => RENAMED_BRANCH_KIND,
        OLD_LEAF => NEW_LEAF,
        _ => return false,
    };
    *kind_id = renamed.to_owned();
    true
}

#[test]
fn decode_step_names_the_reason_for_every_malformed_shape() {
    let kind = || ("kind_id", s(OLD_LEAF));
    for (raw, reason) in [
        (s("not a step"), UndecodableReason::NotAnObject),
        (Variant::Array(vec![]), UndecodableReason::NotAnObject),
        (Variant::Int(7), UndecodableReason::NotAnObject),
        (obj([]), UndecodableReason::MissingKind),
        (
            obj([("config", obj([])), ("enabled", Variant::Bool(true))]),
            UndecodableReason::MissingKind,
        ),
        (
            obj([("kind_id", Variant::Int(7))]),
            UndecodableReason::MalformedField("kind_id"),
        ),
        (
            obj([kind(), ("enabled", s("yes"))]),
            UndecodableReason::MalformedField("enabled"),
        ),
        (
            obj([kind(), ("continue_on_error", Variant::Int(1))]),
            UndecodableReason::MalformedField("continue_on_error"),
        ),
        (
            obj([kind(), ("condition", Variant::Bool(true))]),
            UndecodableReason::MalformedField("condition"),
        ),
        (
            obj([kind(), ("label", Variant::Array(vec![]))]),
            UndecodableReason::MalformedField("label"),
        ),
        (
            obj([kind(), ("config", s("not an object"))]),
            UndecodableReason::MalformedField("config"),
        ),
        (
            obj([kind(), ("config", Variant::Array(vec![]))]),
            UndecodableReason::MalformedField("config"),
        ),
    ] {
        assert_eq!(decode_step(&raw), Err(reason), "{raw:?}");
    }
}

#[test]
fn decode_chain_keeps_each_undecodable_entry_verbatim_at_its_stored_position() {
    let missing_kind = obj([("config", obj([]))]);
    let chain = Variant::Array(vec![
        leaf(OLD_LEAF),
        s("not a step"),
        leaf(NEW_LEAF),
        missing_kind.clone(),
    ]);

    let decoded = decode_chain(Some(&chain));

    assert_eq!(
        decoded.entries(),
        [
            ChainEntry::Step(top(OLD_LEAF, true, [])),
            ChainEntry::Undecodable(UndecodableStep {
                position: 1,
                reason: UndecodableReason::NotAnObject,
                raw: s("not a step"),
            }),
            ChainEntry::Step(top(NEW_LEAF, true, [])),
            ChainEntry::Undecodable(UndecodableStep {
                position: 3,
                reason: UndecodableReason::MissingKind,
                raw: missing_kind,
            }),
        ]
    );
}

#[test]
fn stored_chains_re_encode_to_the_golden_bytes() {
    let mut drifted = Vec::new();
    for (stored, golden) in DRIFT_CASES {
        let chains: Vec<Variant> =
            serde_json::from_str(&fixture(stored)).expect("the fixture holds variant chains");

        let resaved: Vec<Variant> = chains
            .iter()
            .map(|chain| encode_chain(decode_chain(Some(chain)).entries()))
            .collect();

        let resaved = format!("{}\n", serde_json::to_string_pretty(&resaved).unwrap());
        if resaved != fixture(golden) {
            drifted.push(format!("{stored} -> {golden}:\n{resaved}"));
        }
    }
    assert!(
        drifted.is_empty(),
        "step chains drifted:\n{}",
        drifted.join("\n")
    );
}

fn walk_fixture() -> Vec<SubActionStep> {
    let switch = entry(
        SWITCH_KIND,
        true,
        obj([
            (
                CASES_KEY,
                Variant::Array(vec![
                    s("not a case"),
                    obj([(CASE_CHAIN_KEY, Variant::Array(vec![leaf("case.0")]))]),
                    obj([(CASE_CHAIN_KEY, Variant::Array(vec![leaf("case.1")]))]),
                ]),
            ),
            (DEFAULT_KEY, Variant::Array(vec![leaf("default")])),
        ]),
    );
    let disabled_branch = entry(
        BRANCH_KIND,
        false,
        obj([(
            THEN_KEY,
            Variant::Array(vec![leaf("under.disabled"), s("garbage under disabled")]),
        )]),
    );
    vec![
        top(
            BRANCH_KIND,
            true,
            [
                (
                    THEN_KEY,
                    Variant::Array(vec![leaf("then.0"), s("garbage"), switch, disabled_branch]),
                ),
                (ELSE_KEY, Variant::Array(vec![leaf("else.0")])),
            ],
        ),
        top(
            OPTIONAL_KIND,
            false,
            [(OPTIONAL_BODY_KEY, Variant::Array(vec![leaf("optional.0")]))],
        ),
        top("last", true, []),
    ]
}

#[test]
fn walk_visits_parents_before_children_in_field_and_case_order() {
    assert_eq!(
        visited(&walk_fixture(), StepReach::All),
        [
            BRANCH_KIND,
            "then.0",
            "#1",
            SWITCH_KIND,
            "case.0",
            "case.1",
            "default",
            BRANCH_KIND,
            "under.disabled",
            "#1",
            "else.0",
            OPTIONAL_KIND,
            "optional.0",
            "last",
        ]
    );
}

#[test]
fn walk_over_enabled_steps_skips_each_disabled_step_with_its_whole_subtree() {
    assert_eq!(
        visited(&walk_fixture(), StepReach::Enabled),
        [
            BRANCH_KIND,
            "then.0",
            "#1",
            SWITCH_KIND,
            "case.0",
            "case.1",
            "default",
            "else.0",
            "last",
        ]
    );
}

#[test]
fn nested_chains_reach_a_chain_behind_an_optional_field() {
    let step = top(
        OPTIONAL_KIND,
        true,
        [(OPTIONAL_BODY_KEY, Variant::Array(vec![leaf(OLD_LEAF)]))],
    );

    let chains = nested_chains(&step, &registry());

    let kinds: Vec<Vec<&str>> = chains
        .iter()
        .map(|chain| chain.steps().map(|step| step.kind_id.as_str()).collect())
        .collect();
    assert_eq!(kinds, [[OLD_LEAF]]);
}

#[test]
fn rewrite_renames_kinds_at_every_depth_and_descends_through_the_new_id() {
    let mut steps = vec![top(
        BRANCH_KIND,
        true,
        [(
            THEN_KEY,
            Variant::Array(vec![
                entry(OLD_LEAF, false, obj([])),
                entry(
                    BRANCH_KIND,
                    true,
                    obj([(THEN_KEY, Variant::Array(vec![leaf(OLD_LEAF)]))]),
                ),
            ]),
        )],
    )];

    let changed = rewrite_steps(&mut steps, &registry(), &mut rename);

    assert!(changed);
    assert_eq!(
        steps,
        [top(
            RENAMED_BRANCH_KIND,
            true,
            [(
                THEN_KEY,
                Variant::Array(vec![
                    entry(NEW_LEAF, false, obj([])),
                    entry(
                        RENAMED_BRANCH_KIND,
                        true,
                        obj([(THEN_KEY, Variant::Array(vec![leaf(NEW_LEAF)]))]),
                    ),
                ]),
            )],
        )]
    );
}

#[test]
fn rewrite_keeps_bare_entries_bare_and_leaves_undecodable_entries_untouched() {
    let wrong_flag = obj([("enabled", s("yes")), ("kind_id", s(OLD_LEAF))]);
    let chain = |bare_kind: &str| {
        Variant::Array(vec![
            obj([("kind_id", s(bare_kind))]),
            obj([("kind_id", Variant::Int(7))]),
            wrong_flag.clone(),
        ])
    };
    let mut steps = vec![top(SWITCH_KIND, true, [(DEFAULT_KEY, chain(OLD_LEAF))])];

    rewrite_steps(&mut steps, &registry(), &mut rename);

    assert_eq!(
        steps,
        [top(SWITCH_KIND, true, [(DEFAULT_KEY, chain(NEW_LEAF))])]
    );
}

#[test]
fn rewrite_edits_only_kind_and_config_of_a_nested_entry() {
    let stored = |kind_id: &str, config: Variant| {
        obj([
            ("condition", s("%armed%")),
            ("config", config),
            ("continue_on_error", Variant::Bool(true)),
            ("enabled", Variant::Bool(false)),
            ("kind_id", s(kind_id)),
            ("label", s("мітка")),
            ("note", s("unknown key")),
        ])
    };
    let mut steps = vec![top(
        BRANCH_KIND,
        true,
        [(
            ELSE_KEY,
            Variant::Array(vec![stored(OLD_LEAF, obj([("level", s("info"))]))]),
        )],
    )];

    rewrite_steps(&mut steps, &registry(), &mut |kind_id, config| {
        if kind_id != OLD_LEAF {
            return false;
        }
        *kind_id = NEW_LEAF.to_owned();
        config.insert("level".to_owned(), s("warn"));
        true
    });

    assert_eq!(
        steps,
        [top(
            BRANCH_KIND,
            true,
            [(
                ELSE_KEY,
                Variant::Array(vec![stored(NEW_LEAF, obj([("level", s("warn"))]))]),
            )],
        )]
    );
}

#[test]
fn rewrite_with_an_edit_that_changes_nothing_reports_no_change_and_keeps_the_tree() {
    let original = walk_fixture();
    let mut steps = original.clone();

    let changed = rewrite_steps(&mut steps, &registry(), &mut |_, _| false);

    assert!(!changed);
    assert_eq!(steps, original);
}

#[test]
fn encode_onto_stored_keeps_unchanged_entries_verbatim_and_unknown_keys_on_edit() {
    let bare = obj([("kind_id", s(OLD_LEAF)), ("x_extra", Variant::Int(1))]);
    let annotated = obj([
        ("kind_id", s(OLD_LEAF)),
        ("config", obj([])),
        ("enabled", Variant::Bool(true)),
        ("continue_on_error", Variant::Bool(false)),
        ("condition", s("%user% == \"bob\"")),
        ("label", s("greet")),
        ("x_extra", Variant::Int(1)),
    ]);
    let edited = |enabled: bool| {
        obj([
            ("kind_id", s(OLD_LEAF)),
            ("config", obj([])),
            ("enabled", Variant::Bool(enabled)),
            ("continue_on_error", Variant::Bool(false)),
            ("x_extra", Variant::Int(1)),
        ])
    };
    let fresh = obj([
        ("kind_id", s(OLD_LEAF)),
        ("config", obj([])),
        ("enabled", Variant::Bool(true)),
        ("continue_on_error", Variant::Bool(false)),
    ]);
    for (case, edit, stored, expected) in [
        (
            "unchanged bare entry",
            step(OLD_LEAF, cfg([]), true),
            bare.clone(),
            bare.clone(),
        ),
        (
            "edited entry",
            step(OLD_LEAF, cfg([]), false),
            bare,
            edited(false),
        ),
        (
            "condition and label cleared",
            step(OLD_LEAF, cfg([]), true),
            annotated,
            edited(true),
        ),
        (
            "stored value is not an entry",
            step(OLD_LEAF, cfg([]), true),
            s("not a step"),
            fresh,
        ),
    ] {
        assert_eq!(encode_step_onto(&edit, &stored), expected, "{case}");
    }
}
