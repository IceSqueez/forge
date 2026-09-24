#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreMathEvaluateRunner, CoreStringConcatRunner, CoreStringFormatRunner, CoreStringLengthRunner,
    CoreStringLowercaseRunner, CoreStringRegexMatchRunner, CoreStringReplaceRunner,
    CoreStringSplitRunner, CoreStringSubstringRunner, CoreStringTitlecaseRunner,
    CoreStringTrimRunner, CoreStringUppercaseRunner,
};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn cfg(entries: &[(&str, Variant)]) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("into_var".to_owned(), s("out"));
    for (k, v) in entries {
        c.insert((*k).to_owned(), v.clone());
    }
    c
}

fn args(entries: &[(&str, Variant)]) -> ArgStack {
    entries.iter().fold(ArgStack::new(), |acc, (k, v)| {
        acc.set((*k).to_owned(), v.clone())
    })
}

async fn execute(
    runner: &dyn SubActionRunner,
    config: &SubActionConfig,
    stack: &ArgStack,
) -> (SubActionOutcome, Option<ArgStack>) {
    let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, out) = runner.execute(config, &ctx).await;
    (telemetry.outcome, out)
}

async fn output(
    runner: &dyn SubActionRunner,
    config: &SubActionConfig,
    stack: &ArgStack,
) -> Variant {
    let (outcome, out) = execute(runner, config, stack).await;
    assert!(
        matches!(outcome, SubActionOutcome::Success),
        "{}: {outcome:?}",
        runner.id()
    );
    out.and_then(|stack| stack.get("out").cloned())
        .unwrap_or_else(|| panic!("{}: no output", runner.id()))
}

#[tokio::test]
async fn every_string_runner_resolves_variables_in_its_text_inputs() {
    let stack = args(&[
        ("name", s("Alice")),
        ("padded", s("  Alice  ")),
        ("greeting", s("hello world")),
        ("csv", s("a,b")),
        ("sep", s("-")),
        ("dot", s(".")),
        ("x", s("X")),
    ]);
    let cases: Vec<(Box<dyn SubActionRunner>, SubActionConfig, Variant)> = vec![
        (
            Box::new(CoreStringFormatRunner),
            cfg(&[("template", s("Hi %name%"))]),
            s("Hi Alice"),
        ),
        (
            Box::new(CoreStringConcatRunner),
            cfg(&[
                ("parts", Variant::Array(vec![s("Hi"), s("%name%")])),
                ("separator", s("%sep%")),
            ]),
            s("Hi-Alice"),
        ),
        (
            Box::new(CoreStringConcatRunner),
            cfg(&[("parts", s("Hi\n%name%")), ("separator", s(" "))]),
            s("Hi Alice"),
        ),
        (
            Box::new(CoreStringUppercaseRunner),
            cfg(&[("source", s("%name%"))]),
            s("ALICE"),
        ),
        (
            Box::new(CoreStringLowercaseRunner),
            cfg(&[("source", s("%name%"))]),
            s("alice"),
        ),
        (
            Box::new(CoreStringTitlecaseRunner),
            cfg(&[("source", s("%greeting%"))]),
            s("Hello World"),
        ),
        (
            Box::new(CoreStringTrimRunner),
            cfg(&[("source", s("%padded%"))]),
            s("Alice"),
        ),
        (
            Box::new(CoreStringLengthRunner),
            cfg(&[("source", s("%name%"))]),
            Variant::Int(5),
        ),
        (
            Box::new(CoreStringSubstringRunner),
            cfg(&[
                ("source", s("%name%")),
                ("start_index", Variant::Int(0)),
                ("end_index", Variant::Int(2)),
            ]),
            s("Al"),
        ),
        (
            Box::new(CoreStringSplitRunner),
            cfg(&[("source", s("%csv%")), ("separator", s(","))]),
            Variant::Array(vec![s("a"), s("b")]),
        ),
        (
            Box::new(CoreStringReplaceRunner),
            cfg(&[
                ("source", s("%name%.b")),
                ("search", s("%dot%")),
                ("replace_with", s("%x%")),
                ("is_regex", Variant::Bool(false)),
            ]),
            s("AliceXb"),
        ),
        (
            Box::new(CoreStringRegexMatchRunner),
            cfg(&[("source", s("%name%")), ("pattern", s("^Ali"))]),
            Variant::Bool(true),
        ),
    ];
    for (runner, config, expected) in cases {
        assert_eq!(
            output(runner.as_ref(), &config, &stack).await,
            expected,
            "{}",
            runner.id()
        );
    }
}

#[tokio::test]
async fn regex_patterns_are_never_built_from_argument_text() {
    let stack = args(&[
        ("msg", s("a.b")),
        ("any", s(".*")),
        ("dot", s(".")),
        ("x", s("X")),
    ]);
    let regex_replace = cfg(&[
        ("source", s("%msg%")),
        ("search", s("%dot%")),
        ("replace_with", s("%x%")),
        ("is_regex", Variant::Bool(true)),
    ]);
    assert_eq!(
        output(&CoreStringReplaceRunner, &regex_replace, &stack).await,
        s("a.b"),
        "a regex search must match the literal token, not the argument's regex syntax",
    );

    let regex_match = cfg(&[("source", s("%msg%")), ("pattern", s("%any%"))]);
    assert_eq!(
        output(&CoreStringRegexMatchRunner, &regex_match, &stack).await,
        Variant::Bool(false),
    );
}

#[tokio::test]
async fn literal_replace_treats_regex_syntax_in_an_argument_as_plain_text() {
    let stack = args(&[("msg", s("a.b")), ("dot", s(".")), ("x", s("X"))]);
    let literal = cfg(&[
        ("source", s("%msg%")),
        ("search", s("%dot%")),
        ("replace_with", s("%x%")),
        ("is_regex", Variant::Bool(false)),
    ]);
    assert_eq!(
        output(&CoreStringReplaceRunner, &literal, &stack).await,
        s("aXb")
    );
}

fn math_cfg(expression: &str) -> SubActionConfig {
    cfg(&[("expression", s(expression))])
}

#[tokio::test]
async fn math_evaluates_over_bound_argument_values() {
    for (value, expected) in [(Variant::Int(7), 14), (s("7"), 14)] {
        let stack = args(&[("count", value.clone())]);
        let got = output(
            &CoreMathEvaluateRunner::new(),
            &math_cfg("%count% * 2"),
            &stack,
        )
        .await;
        assert_eq!(got, Variant::Int(expected), "count = {value:?}");
    }
}

#[tokio::test]
async fn math_with_undefined_variables_fails_naming_them() {
    let stack = args(&[("known", Variant::Int(1))]);
    let (outcome, out) = execute(
        &CoreMathEvaluateRunner::new(),
        &math_cfg("%known% + %missing% * %gone%"),
        &stack,
    )
    .await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("missing") && m.contains("gone") && !m.contains("known")),
        "got {outcome:?}"
    );
    assert!(out.is_none());
}
