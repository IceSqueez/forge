use async_trait::async_trait;
use forge_registry::{
    CASE_CHAIN_KEY, CASE_MATCH_KEY, ChainLayout, ConfigSpec, RunContext, StepChainField, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionStep, SubActionTelemetry, Variant};

use super::core_logic_shared::{chain_at, propagate, retag, runnable_steps};
use crate::core_messages;

const MATCHED_CASE_INDEX_VAR: &str = "switch.matched_case_index";
const NO_CASE_MATCHED: i64 = -1;

const EXPRESSION: TextField = TextField {
    key: "expression",
    label: "Value",
    editor: TextEditor::Line {
        placeholder: "%user_level%",
    },
    default: "",
    required: true,
};

const CASES: StepChainField = StepChainField {
    key: "cases",
    label: "Cases",
    layout: ChainLayout::Cases,
    seeds_empty_list: true,
};

const DEFAULT_CHAIN: StepChainField = StepChainField {
    key: "default_chain",
    label: "Default",
    layout: ChainLayout::Steps,
    seeds_empty_list: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&EXPRESSION, &CASES, &DEFAULT_CHAIN]);

pub struct CoreLogicSwitchCaseRunner;

#[async_trait]
impl SubActionRunner for CoreLogicSwitchCaseRunner {
    fn id(&self) -> &str {
        "core.logic.switch_case"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Switch / Case"
    }

    fn summary(&self) -> &str {
        "Run the sub-chain whose case matches a value"
    }

    fn search_text(&self) -> &str {
        "switch case match value branch select flow control"
    }

    fn icon_name(&self) -> &str {
        "list-checks"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let value = match EXPRESSION.read(config, ctx.arg_stack) {
            Ok(value) => value,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let (matched_index, steps) = select_case(config, &value, ctx);
        let arm = if matched_index == NO_CASE_MATCHED {
            "default".to_owned()
        } else {
            format!("case{matched_index}")
        };

        let base = ctx.arg_stack.clone().set(
            MATCHED_CASE_INDEX_VAR.to_owned(),
            Variant::Int(matched_index),
        );

        match ctx
            .executor
            .run_child_chain(&steps, &base, ctx.parent_event_id)
            .await
        {
            Ok(child) => {
                ctx.telemetry
                    .extend(retag(child.telemetry, ctx.index, &arm));
                let outcome = propagate(child.signal, ctx);
                let stack = child.arg_stack.set(
                    MATCHED_CASE_INDEX_VAR.to_owned(),
                    Variant::Int(matched_index),
                );
                (timer.finish(outcome), Some(stack))
            }
            Err(e) => (
                timer.failed(core_messages::condition_failed(e).into()),
                None,
            ),
        }
    }
}

fn select_case(
    config: &SubActionConfig,
    value: &str,
    ctx: &RunContext<'_>,
) -> (i64, Vec<SubActionStep>) {
    let cases = config.get(CASES.key).and_then(Variant::as_array);
    if let Some(cases) = cases {
        for (index, case) in cases.iter().enumerate() {
            let Some(case) = case.as_object() else {
                continue;
            };
            if case_matches(case.get(CASE_MATCH_KEY), value) {
                return (
                    i64::try_from(index).unwrap_or(i64::MAX),
                    runnable_steps(case.get(CASE_CHAIN_KEY), CASE_CHAIN_KEY, ctx),
                );
            }
        }
    }
    (NO_CASE_MATCHED, chain_at(config, DEFAULT_CHAIN.key, ctx))
}

fn case_matches(candidate: Option<&Variant>, value: &str) -> bool {
    match candidate {
        Some(Variant::Array(items)) => items.iter().any(|item| item.to_string() == value),
        Some(single) => single.to_string() == value,
        None => false,
    }
}
