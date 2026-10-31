#![allow(clippy::panic, clippy::expect_used)]
use forge_script::{Engine, EngineConfig};

const WORKER_STACK: usize = 2 << 20;
const SHADOWED_CONSTANTS: usize = 40_000;

fn survives_on_worker_stack(script: String) {
    let joined = std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(move || {
            let outcome = Engine::with_config(EngineConfig::default()).eval_script(&script);
            drop(outcome);
        })
        .expect("a thread")
        .join();

    assert!(joined.is_ok());
}

#[test]
fn a_long_chain_of_shadowed_constants_does_not_abort_the_process() {
    let chain = "const a = [[[[a]]]]; ".repeat(SHADOWED_CONSTANTS);
    survives_on_worker_stack(format!("const a = 0; {chain}0"));
}

#[test]
fn nesting_results_through_the_full_call_depth_does_not_abort_the_process() {
    survives_on_worker_stack("fn r(n) { if n == 0 { 0 } else { [r(n - 1)] } } r(60)".to_owned());
}
