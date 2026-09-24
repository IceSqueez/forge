#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use forge_script::{BoundExpression, ConditionEvaluator, EngineConfig, MathEvaluator};
use forge_types::{ArgStack, Variant};

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn args(entries: &[(&str, Variant)]) -> ArgStack {
    entries.iter().fold(ArgStack::new(), |stack, (k, v)| {
        stack.set((*k).to_owned(), v.clone())
    })
}

fn condition(expr: &str, stack: &ArgStack) -> bool {
    let bound = BoundExpression::bind(expr, stack);
    ConditionEvaluator::with_config(EngineConfig::default())
        .eval_bound(&bound)
        .unwrap_or_else(|e| panic!("{expr:?} -> {:?} failed: {e}", bound.source()))
}

fn rhai_literal(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn math(expr: &str, stack: &ArgStack) -> Variant {
    let bound = BoundExpression::bind(expr, stack);
    MathEvaluator::with_config(EngineConfig::default())
        .eval_bound(&bound)
        .unwrap_or_else(|e| panic!("{expr:?} -> {:?} failed: {e}", bound.source()))
}

#[test]
fn a_value_closing_the_string_literal_cannot_change_the_verdict() {
    let stack = args(&[("message", s(r#"a" == "a" || ""#))]);
    assert!(!condition(r#""%message%" == "!secret""#, &stack));
}

#[test]
fn values_holding_rhai_syntax_compare_as_their_literal_text() {
    for value in [
        r#"he said "hi" http://x"#,
        r"back\slash \n \",
        "back`tick`",
        "${forge_arg_0}",
        "// not a comment",
        "/* not a comment */",
        "'c'",
        "%other%",
        "line\nbreak",
        "",
    ] {
        let stack = args(&[("m", s(value))]);
        let expected = rhai_literal(value);
        assert!(
            condition(&format!(r#""%m%" == {expected}"#), &stack),
            "{value:?} must bind verbatim inside a double-quoted literal",
        );
        assert!(
            condition(&format!("`%m%` == {expected}"), &stack),
            "{value:?} must bind verbatim inside a backtick literal",
        );
    }
}

#[test]
fn text_around_a_token_inside_a_literal_is_kept() {
    let stack = args(&[("name", s("Alice")), ("n", Variant::Int(7))]);
    for expr in [
        r#""Hi %name%!" == "Hi Alice!""#,
        r#""\"%name%\"" == "\"Alice\"""#,
        r#""%name% has %n%" == "Alice has 7""#,
        r#"`Hi %name%!` == "Hi Alice!""#,
        r#"`${%n% + 1}` == "8""#,
        r#""say ""%name%""" == "say \"Alice\"""#,
        r#"`a``%name%` == "a`Alice""#,
    ] {
        assert!(condition(expr, &stack), "{expr}");
    }
}

#[test]
fn code_position_tokens_bind_as_typed_values() {
    let mut obj = BTreeMap::new();
    obj.insert("a".to_owned(), Variant::Int(1));
    let stack = args(&[
        ("count", Variant::Int(7)),
        ("ratio", Variant::Float(2.5)),
        ("flag", Variant::Bool(true)),
        ("digits", s(" 42 ")),
        ("decimal", s("1.5")),
        ("truth", s("false")),
        ("name", s("Alice")),
        (
            "list",
            Variant::Array(vec![Variant::Int(1), Variant::Int(2)]),
        ),
        ("obj", Variant::Object(obj)),
    ]);
    for expr in [
        "%count% * 2 == 14",
        "%ratio% * 2.0 == 5.0",
        "%flag%",
        "%digits% + 1 == 43",
        "%decimal% * 2.0 == 3.0",
        "!%truth%",
        r#"%name% == "Alice""#,
        "%list%.len() == 2",
        "%obj%.a == 1",
        "%count% + %count% == 14",
    ] {
        assert!(condition(expr, &stack), "{expr}");
    }
}

#[test]
fn a_numeric_string_argument_does_math_as_a_number() {
    let stack = args(&[("count", s("7"))]);
    assert_eq!(math("%count% * 2", &stack), Variant::Int(14));
}

#[test]
fn modulo_between_operands_is_not_mistaken_for_a_token() {
    let stack = args(&[("n", Variant::Int(9))]);
    for expr in [
        "5 % 3 == 2",
        "7 % 4 == 3 % 4",
        "%n% % 4 == 1",
        "{ let x = 7; let y = 4; let z = 5; x%y%z == 3 }",
    ] {
        assert!(condition(expr, &stack), "{expr}");
    }
}

#[test]
fn tokens_inside_comments_and_char_literals_are_left_alone() {
    let stack = ArgStack::new();
    for expr in ["true // %missing%", "/* %missing% */ true", "'%' == '%'"] {
        let bound = BoundExpression::bind(expr, &stack);
        assert!(
            bound.unresolved().is_empty(),
            "{expr}: {:?}",
            bound.unresolved()
        );
        assert!(condition(expr, &stack), "{expr}");
    }
}

#[test]
fn an_unresolved_code_token_is_reported_and_left_verbatim() {
    let stack = args(&[("known", Variant::Int(1))]);
    let bound = BoundExpression::bind("%known% + %missing% > 5", &stack);
    assert_eq!(bound.unresolved(), ["missing".to_owned()]);
    assert!(bound.source().contains("%missing%"));
}

#[test]
fn an_unresolved_token_inside_a_literal_stays_literal_text() {
    let stack = ArgStack::new();
    let bound = BoundExpression::bind(r#""%missing%" == "%missing%""#, &stack);
    assert!(bound.unresolved().is_empty());
    assert!(condition(r#""%missing%" == "%" + "missing%""#, &stack));
}

#[test]
fn a_binding_never_collides_with_an_author_variable_of_the_generated_name() {
    let stack = args(&[("n", Variant::Int(7))]);
    assert!(condition(
        "{ let forge_arg_0 = 100; forge_arg_0 + %n% == 107 }",
        &stack
    ));
}
