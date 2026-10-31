#![expect(clippy::expect_used, reason = "test fixtures")]
use forge_script::{Engine, EngineConfig, ScriptError};
use forge_types::Variant;

const WORKER_STACK: usize = 2 << 20;
const HIGHEST_OP_LIMIT: u64 = 10_000_000;
const HANG_GUARD_MS: u64 = 60_000;
const DATA_REFUSAL: &str = "Script data too large";
const BIG_MAP: &str = "let big = #{}; let j = 0; while j < 9000 { big[`k${j}`] = j; j += 1; } ";

fn run_on_worker_stack(script: String) -> Result<Option<Variant>, ScriptError> {
    std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(move || {
            Engine::with_config(EngineConfig {
                op_limit: HIGHEST_OP_LIMIT,
                wall_time_ms: HANG_GUARD_MS,
            })
            .eval_script_with_scope_as_variant(&script, &mut rhai::Scope::new())
        })
        .expect("a thread")
        .join()
        .expect("the script thread")
}

fn assert_refused_as_data(what: &str, script: String) {
    let outcome = run_on_worker_stack(script);
    assert!(
        matches!(&outcome, Err(ScriptError::Runtime { reason }) if reason.contains(DATA_REFUSAL)),
        "{what}: {outcome:?}"
    );
}

fn wrapped(depth: usize) -> String {
    format!("{}v{}", "[".repeat(depth), "]".repeat(depth))
}

fn map_wrapped(depth: usize) -> String {
    format!("{}v{}", "#{a:".repeat(depth), "}".repeat(depth))
}

#[test]
fn dense_wraps_beside_a_big_map_are_refused_before_the_stack_runs_out() {
    for (what, wrap) in [
        ("eight arrays", wrapped(8)),
        ("deepest parsable arrays", wrapped(15)),
        ("eight maps", map_wrapped(8)),
    ] {
        assert_refused_as_data(
            what,
            format!("{BIG_MAP}let v = 0; loop {{ v = {wrap}; }} 0"),
        );
    }
}

#[test]
fn wraps_hidden_from_a_plain_assignment_are_refused_before_the_stack_runs_out() {
    let wrap = wrapped(8);
    for (what, script) in [
        (
            "for loop",
            format!("let v = 0; for i in 0..100000000 {{ v = {wrap}; }} 0"),
        ),
        (
            "if expression",
            format!("let v = 0; loop {{ v = if true {{ {wrap} }} else {{ 0 }}; }} 0"),
        ),
        (
            "switch expression",
            format!("let v = 0; loop {{ v = switch 1 {{ 1 => {wrap}, _ => 0 }}; }} 0"),
        ),
        (
            "function result",
            format!("fn w(v) {{ {wrap} }} let v = 0; loop {{ v = w(w(w(w(v)))); }} 0"),
        ),
        (
            "constant in a function",
            format!("fn w(v) {{ const c = {wrap}; c }} let v = 0; loop {{ v = w(v); }} 0"),
        ),
        (
            "indexed slot",
            "let v = [0]; loop { v[0] = [[[[[[[[v[0]]]]]]]]]; } 0".to_owned(),
        ),
        (
            "this",
            "fn w() { loop { this = [[[[[[[[this]]]]]]]]; } } let v = 0; v.w(); 0".to_owned(),
        ),
        (
            "function arguments across calls",
            "fn r(n, v) { let i = 0; while i < 1000 { v = [v]; i += 1; } \
             if n == 0 { v } else { r(n - 1, v) } } r(60, 0)"
                .to_owned(),
        ),
    ] {
        assert_refused_as_data(what, script);
    }
}

#[test]
fn reading_a_deep_value_in_every_shape_is_refused_at_the_nesting_limit() {
    let deep = "let v = 0; let i = 0; while i < 900 { v = [v]; i += 1; } ";
    for (what, tail) in [
        ("returned", "v"),
        ("debug text", "v.to_debug()"),
        ("interpolated", "`${v}`"),
        ("cloned", "let c = v; [c, c]"),
        ("compared", "let c = v; c == v"),
    ] {
        assert_refused_as_data(what, format!("{deep}{tail}"));
    }
}
