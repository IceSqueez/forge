use std::sync::Arc;

use forge_registry::CancelSignal;
use forge_runtime::{ChainEngine, ChainRun};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionStep, Variant};

pub(crate) async fn run_top(engine: &Arc<ChainEngine>, steps: Vec<SubActionStep>) -> ChainRun {
    engine
        .run_sequential(
            &steps,
            &ArgStack::new(),
            EventId::new(),
            &CancelSignal::new(),
        )
        .await
}

pub(crate) fn step(kind: &str, config: SubActionConfig) -> SubActionStep {
    SubActionStep {
        kind_id: kind.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub(crate) fn chain_step(kind: &str, config: SubActionConfig) -> Variant {
    let mut m = SubActionConfig::new();
    m.insert("kind_id".to_owned(), Variant::String(kind.to_owned()));
    m.insert("config".to_owned(), Variant::Object(config));
    m.insert("enabled".to_owned(), Variant::Bool(true));
    Variant::Object(m)
}

pub(crate) fn inline(steps: Vec<Variant>) -> Variant {
    Variant::Array(steps)
}

pub(crate) fn args_set(name: &str, value: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("name".to_owned(), Variant::String(name.to_owned()));
    c.insert("value".to_owned(), Variant::String(value.to_owned()));
    c
}

pub(crate) fn loop_cfg(count: i64, body: Variant) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("mode".to_owned(), Variant::String("count".to_owned()));
    c.insert("count".to_owned(), Variant::Int(count));
    c.insert("body".to_owned(), body);
    c
}

pub(crate) fn if_cfg(condition: &str, then_chain: Variant, else_chain: Variant) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "condition".to_owned(),
        Variant::String(condition.to_owned()),
    );
    c.insert("then_chain".to_owned(), then_chain);
    c.insert("else_chain".to_owned(), else_chain);
    c
}

pub(crate) fn case(match_val: Variant, chain: Variant) -> Variant {
    let mut m = SubActionConfig::new();
    m.insert("match".to_owned(), match_val);
    m.insert("chain".to_owned(), chain);
    Variant::Object(m)
}

pub(crate) fn switch_cfg(
    expression: &str,
    cases: Vec<Variant>,
    default_chain: Variant,
) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "expression".to_owned(),
        Variant::String(expression.to_owned()),
    );
    c.insert("cases".to_owned(), Variant::Array(cases));
    c.insert("default_chain".to_owned(), default_chain);
    c
}
