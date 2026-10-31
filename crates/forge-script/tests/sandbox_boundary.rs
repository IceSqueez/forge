#![allow(clippy::panic, clippy::expect_used)]
use forge_script::{
    ConditionEvaluator, Engine, EngineConfig, MathEvaluator, ScriptError, validate_syntax,
};

const HANG_GUARD_MS: u64 = 60_000;
const LIMIT_NOT_REACHED_FIRST: u64 = u64::MAX;

fn engine() -> Engine {
    Engine::with_config(EngineConfig {
        op_limit: LIMIT_NOT_REACHED_FIRST,
        wall_time_ms: HANG_GUARD_MS,
    })
}

fn refusal(script: &str) -> ScriptError {
    engine()
        .eval_script(script)
        .err()
        .unwrap_or_else(|| panic!("the sandbox ran {script:?}"))
}

fn nested_parens(depth: usize) -> String {
    format!("{}1{}", "(".repeat(depth), ")".repeat(depth))
}

#[test]
fn dynamic_code_evaluation_is_refused_at_compile_time_everywhere() {
    const SOURCE: &str = "eval(\"1\")";

    assert!(matches!(refusal(SOURCE), ScriptError::Compile { .. }));
    assert!(matches!(
        validate_syntax(SOURCE),
        Err(ScriptError::Compile { .. })
    ));
    assert!(
        ConditionEvaluator::with_config(EngineConfig::default())
            .eval(SOURCE)
            .is_err()
    );
    assert!(
        MathEvaluator::with_config(EngineConfig::default())
            .eval(SOURCE)
            .is_err()
    );
}

#[test]
fn a_module_on_disk_cannot_be_imported() {
    let dir = tempfile::tempdir().expect("a temp dir");
    std::fs::write(dir.path().join("planted.rhai"), "export const secret = 41;")
        .expect("a planted module");
    let script = format!(
        "import \"{}\" as planted; planted::secret + 1",
        dir.path().join("planted").display()
    );

    assert!(engine().eval_script(&script).is_err());
}

#[test]
fn no_host_escape_function_exists() {
    for script in [
        "open_file(\"/etc/passwd\")",
        "read_string(\"/etc/passwd\")",
        "system(\"id\")",
        "exec(\"id\")",
        "spawn(\"id\")",
        "http_get(\"http://192.0.2.1/\")",
    ] {
        assert!(engine().eval_script(script).is_err(), "{script}");
    }
}

#[test]
fn runaway_recursion_is_a_runtime_error_not_a_stack_overflow() {
    assert!(matches!(
        refusal("fn dive(n) { dive(n + 1) } dive(0)"),
        ScriptError::Runtime { .. }
    ));
}

#[test]
fn recursion_inside_the_call_limit_still_runs() {
    let value = engine()
        .eval_script("fn dive(n) { if n == 0 { 0 } else { dive(n - 1) } } dive(32)")
        .expect("a shallow recursion");

    assert_eq!(value.as_int(), Ok(0));
}

#[test]
fn growing_a_value_past_its_cap_is_a_runtime_error() {
    for (kind, script) in [
        ("string", "let s = \"xxxxxxxx\"; loop { s += s; }"),
        ("array", "let a = [0]; loop { a += a; }"),
        (
            "map",
            "let m = #{}; let i = 0; loop { m[`k${i}`] = i; i += 1; m.len(); }",
        ),
    ] {
        assert!(
            matches!(refusal(script), ScriptError::Runtime { .. }),
            "{kind}"
        );
    }
}

#[test]
fn nesting_past_the_expression_depth_is_refused_by_validation_and_the_engine_alike() {
    let source = nested_parens(200);

    assert!(matches!(refusal(&source), ScriptError::Compile { .. }));
    assert!(matches!(
        validate_syntax(&source),
        Err(ScriptError::Compile { .. })
    ));
}

#[test]
fn nesting_inside_the_expression_depth_runs() {
    assert!(engine().eval_script(&nested_parens(8)).is_ok());
}
