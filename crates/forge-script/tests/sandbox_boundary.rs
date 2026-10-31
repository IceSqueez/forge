#![allow(clippy::panic, clippy::expect_used)]
use forge_script::{
    ConditionEvaluator, Engine, EngineConfig, MathEvaluator, ScriptError, validate_syntax,
};
use forge_types::{VARIANT_MAX_COLLECTION_LEN, VARIANT_MAX_DEPTH};

const HANG_GUARD_MS: u64 = 60_000;
const LIMIT_NOT_REACHED_FIRST: u64 = u64::MAX;
const DATA_REFUSAL: &str = "Script data too large";
const UNCAPTURED_VARIABLE: &str = "Variable not found";
const PAST_THE_COLLECTION_LIMIT: usize = VARIANT_MAX_COLLECTION_LEN * 3;
const INSIDE_THE_COLLECTION_LIMIT: usize = 9_000;
const BIG_TEXT_DOUBLINGS: usize = 19;
const ENOUGH_BIG_TEXTS_TO_LEAVE_THE_BUDGET: usize = 1_000;
const FEW_ENOUGH_BIG_TEXTS_TO_STAY_IN_THE_BUDGET: usize = 100;
const ENOUGH_READS_TO_BE_SAMPLED: usize = 5_000;
const OPS_FOR_A_DEEP_WRAP: u64 = 10_000;
const STACK_FOR_A_DEEP_WRAP: usize = 1 << 28;
const EMPTY_FRAME_CALLS: usize = 2_000;
const REPEAT_RUNS: usize = 3;

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

fn assert_data_refusal(what: &str, script: &str) {
    match refusal(script) {
        ScriptError::Runtime { reason } => {
            assert!(reason.contains(DATA_REFUSAL), "{what}: {reason}")
        }
        other => panic!("{what}: {other:?}"),
    }
}

fn int_result(what: &str, script: &str) -> i64 {
    engine()
        .eval_script(script)
        .unwrap_or_else(|e| panic!("{what}: {e:?}"))
        .as_int()
        .unwrap_or_else(|got| panic!("{what}: returned {got}"))
}

fn big_text_setup() -> String {
    format!("let blob = \"x\"; for step in 0..{BIG_TEXT_DOUBLINGS} {{ blob += blob; }} ")
}

fn indexed_growth_scripts(entries: usize) -> [(&'static str, String); 5] {
    [
        (
            "top level",
            format!(
                "let m = #{{}}; let i = 0; while i < {entries} {{ m[`k${{i}}`] = i; i += 1; }} m.len()"
            ),
        ),
        (
            "nested object",
            format!(
                "let m = #{{ inner: #{{}} }}; let i = 0; \
                 while i < {entries} {{ m.inner[`k${{i}}`] = i; i += 1; }} m.inner.len()"
            ),
        ),
        (
            "this",
            format!(
                "fn grow(n) {{ let i = 0; while i < n {{ this[`k${{i}}`] = i; i += 1; }} }} \
                 let m = #{{}}; m.grow({entries}); m.len()"
            ),
        ),
        (
            "function local",
            format!(
                "fn build(n) {{ let m = #{{}}; let i = 0; \
                 while i < n {{ m[`k${{i}}`] = i; i += 1; }} m.len() }} build({entries})"
            ),
        ),
        (
            "array of maps",
            format!(
                "let a = [#{{}}]; let i = 0; \
                 while i < {entries} {{ a[0][`k${{i}}`] = i; i += 1; }} a[0].len()"
            ),
        ),
    ]
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
            "let m = #{}; let i = 0; loop { m[`k${i}`] = i; i += 1; }",
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

#[test]
fn indexing_a_map_past_the_collection_limit_is_refused_wherever_the_map_lives() {
    for (place, script) in indexed_growth_scripts(PAST_THE_COLLECTION_LIMIT) {
        assert_data_refusal(place, &script);
    }
}

#[test]
fn maps_inside_the_collection_limit_are_left_alone_wherever_they_live() {
    for (place, script) in indexed_growth_scripts(INSIDE_THE_COLLECTION_LIMIT) {
        assert_eq!(
            int_result(place, &script),
            i64::try_from(INSIDE_THE_COLLECTION_LIMIT).expect("a small count"),
            "{place}"
        );
    }
}

#[test]
fn a_growing_map_is_refused_even_when_helpers_do_most_of_the_reading() {
    let script = format!(
        "fn key(i) {{ let t = 0; let n = 0; while n < 300 {{ t += i; n += 1; }} t }} \
         let m = #{{}}; let i = 0; \
         while i < {PAST_THE_COLLECTION_LIMIT} {{ m[`k${{key(i)}}`] = i; i += 1; }} m.len()"
    );

    assert_data_refusal("helper-heavy growth", &script);
}

#[test]
fn big_values_and_big_keys_past_the_data_budget_are_refused() {
    let setup = big_text_setup();
    let limit = ENOUGH_BIG_TEXTS_TO_LEAVE_THE_BUDGET;
    for (kind, body) in [
        ("values", "m[`k${i}`] = blob;"),
        ("keys", "m[`${blob}${i}`] = 0;"),
    ] {
        let script = format!(
            "{setup}let m = #{{}}; let i = 0; while i < {limit} {{ {body} i += 1; }} m.len()"
        );
        assert_data_refusal(kind, &script);
    }
}

#[test]
fn big_values_inside_the_data_budget_are_left_alone() {
    let setup = big_text_setup();
    let limit = FEW_ENOUGH_BIG_TEXTS_TO_STAY_IN_THE_BUDGET;
    let script = format!(
        "{setup}let m = #{{}}; let i = 0; while i < {limit} {{ m[`k${{i}}`] = blob; i += 1; }} \
         let reads = 0; while reads < {ENOUGH_READS_TO_BE_SAMPLED} {{ reads += 1; }} i"
    );

    assert_eq!(
        int_result("big values inside the budget", &script),
        i64::try_from(limit).expect("a small count")
    );
}

#[test]
fn wrapping_a_value_past_the_nesting_limit_is_refused() {
    for (kind, wrap) in [("array", "v = [v];"), ("map", "v = #{ inner: v };")] {
        let script = format!(
            "let v = 0; let i = 0; while i < {ENOUGH_READS_TO_BE_SAMPLED} {{ {wrap} i += 1; }} i"
        );
        assert_data_refusal(kind, &script);
    }
}

#[test]
fn wrapping_this_past_the_nesting_limit_is_refused_without_any_variable_read() {
    for (kind, wrap) in [
        ("array", "this = [this];"),
        ("map", "this = #{ inner: this };"),
    ] {
        let script = format!("fn wrap() {{ loop {{ {wrap} }} }} let v = 0; v.wrap(); v");
        let outcome = std::thread::Builder::new()
            .stack_size(STACK_FOR_A_DEEP_WRAP)
            .spawn(move || {
                Engine::with_config(EngineConfig {
                    op_limit: OPS_FOR_A_DEEP_WRAP,
                    wall_time_ms: HANG_GUARD_MS,
                })
                .eval_script(&script)
                .map(|_| ())
            })
            .expect("a thread")
            .join()
            .expect("the script thread");

        assert!(
            matches!(&outcome, Err(ScriptError::Runtime { reason }) if reason.contains(DATA_REFUSAL)),
            "{kind}: {outcome:?}"
        );
    }
}

#[test]
fn nesting_at_the_depth_limit_is_left_alone() {
    let script = format!(
        "let v = []; let i = 0; while i < {VARIANT_MAX_DEPTH} {{ v = [v]; i += 1; }} \
         let reads = 0; while reads < {ENOUGH_READS_TO_BE_SAMPLED} {{ reads += 1; }} i"
    );

    assert_eq!(
        int_result("deepest legal nesting", &script),
        i64::from(VARIANT_MAX_DEPTH)
    );
}

#[test]
fn a_bound_constant_past_every_limit_is_not_refused() {
    let oversized: rhai::Array = (0..PAST_THE_COLLECTION_LIMIT)
        .map(|_| rhai::Dynamic::from_int(1))
        .collect();
    let mut scope = rhai::Scope::new();
    scope.push_constant_dynamic("bound", rhai::Dynamic::from_array(oversized));
    let body = format!(
        "let total = 0; let i = 0; \
         while i < {ENOUGH_READS_TO_BE_SAMPLED} {{ total += bound[0]; i += 1; }} total"
    );

    let total = engine()
        .eval_script_with_scope(&body, &mut scope)
        .expect("a constant is read, never grown")
        .as_int();

    assert_eq!(
        total,
        Ok(i64::try_from(ENOUGH_READS_TO_BE_SAMPLED).expect("a small count"))
    );
}

#[test]
fn a_bound_mutable_value_past_the_limit_is_refused() {
    let oversized: rhai::Array = (0..PAST_THE_COLLECTION_LIMIT)
        .map(|_| rhai::Dynamic::from_int(1))
        .collect();
    let mut scope = rhai::Scope::new();
    scope.push_dynamic("bound", rhai::Dynamic::from_array(oversized));
    let body =
        format!("let i = 0; while i < {ENOUGH_READS_TO_BE_SAMPLED} {{ bound[0] = i; i += 1; }} i");

    let refused = engine().eval_script_with_scope(&body, &mut scope);

    assert!(
        matches!(&refused, Err(ScriptError::Runtime { reason }) if reason.contains(DATA_REFUSAL)),
        "{refused:?}"
    );
}

#[test]
fn sampling_never_refuses_a_legitimate_script_however_often_it_runs() {
    let engine = engine();
    let at_the_limit = format!(
        "let m = #{{}}; let i = 0; \
         while i < {VARIANT_MAX_COLLECTION_LEN} {{ m[`k${{i}}`] = i; i += 1; }} \
         let reads = 0; while reads < {ENOUGH_READS_TO_BE_SAMPLED} {{ reads += 1; }} m.len()"
    );
    let reentrant_pointer = format!(
        "let m = #{{ n: 0 }}; let bump = |d| {{ this.n += d; }}; let i = 0; \
         while i < {ENOUGH_READS_TO_BE_SAMPLED} {{ m.call(bump, 1); i += 1; }} m.n"
    );
    let empty_frame =
        format!("fn one() {{ 1 }} let i = 0; while i < {EMPTY_FRAME_CALLS} {{ i += one(); }} i");
    let expected = [
        (at_the_limit, VARIANT_MAX_COLLECTION_LEN),
        (reentrant_pointer, ENOUGH_READS_TO_BE_SAMPLED),
        (empty_frame, EMPTY_FRAME_CALLS),
    ];

    for _ in 0..REPEAT_RUNS {
        for (script, count) in &expected {
            let value = engine.eval_script(script).expect("a legitimate script");
            assert_eq!(value.as_int().ok(), i64::try_from(*count).ok(), "{script}");
        }
    }
}

#[test]
fn a_closure_cannot_reach_a_variable_it_would_capture() {
    for (what, script) in [
        ("read", "let a = 1; let f = || a; f.call()"),
        ("write", "let n = 0; let f = || n += 1; f.call(); n"),
        ("through a map", "let m = #{}; let f = || m.len(); f.call()"),
    ] {
        match refusal(script) {
            ScriptError::Runtime { reason } => {
                assert!(reason.contains(UNCAPTURED_VARIABLE), "{what}: {reason}")
            }
            other => panic!("{what}: {other:?}"),
        }
    }
}

#[test]
fn a_function_pointer_that_captures_nothing_still_runs() {
    for (what, script) in [
        ("anonymous", "let f = |x| x + 1; f.call(1)"),
        ("named", "fn up(x) { x + 1 } Fn(\"up\").call(1)"),
    ] {
        assert_eq!(int_result(what, script), 2, "{what}");
    }
}

#[test]
fn currying_is_refused_at_compile_time_everywhere() {
    for source in [
        "fn g(x, y) { x } Fn(\"g\").curry(1)",
        "fn g(x, y) { x } curry(Fn(\"g\"), 1)",
    ] {
        assert!(
            matches!(refusal(source), ScriptError::Compile { .. }),
            "{source}"
        );
        assert!(
            matches!(validate_syntax(source), Err(ScriptError::Compile { .. })),
            "{source}"
        );
        assert!(
            ConditionEvaluator::with_config(EngineConfig::default())
                .eval(source)
                .is_err(),
            "{source}"
        );
        assert!(
            MathEvaluator::with_config(EngineConfig::default())
                .eval(source)
                .is_err(),
            "{source}"
        );
    }
}

#[test]
fn currying_cannot_be_reached_through_a_function_pointer_by_name() {
    for source in [
        "fn g(x, y) { x } Fn(\"curry\").call(Fn(\"g\"), 1)",
        "fn g(x, y) { x } let name = \"cur\" + \"ry\"; Fn(name).call(Fn(\"g\"), 1)",
    ] {
        let refused = refusal(source);
        assert!(
            matches!(refused, ScriptError::Compile { .. }),
            "{source}: {refused:?}"
        );
    }
}
