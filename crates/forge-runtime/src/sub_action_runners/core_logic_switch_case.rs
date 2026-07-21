use async_trait::async_trait;
use forge_registry::{
    CodeLanguage, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionStep, SubActionTelemetry, Variant};

use super::core_logic_shared::{decode_chain, decode_steps, propagate, retag};

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
        "Run the sub-chain whose case matches an expression"
    }

    fn search_text(&self) -> &str {
        "switch case match expression branch select flow control"
    }

    fn icon_name(&self) -> &str {
        "list-checks"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("expression".to_owned(), Variant::String(String::new()));
        cfg.insert("cases".to_owned(), Variant::Array(Vec::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Code {
                key: "expression",
                label: "Expression",
                language: CodeLanguage::Rhai,
            },
            FormField::CaseList {
                key: "cases",
                label: "Cases",
            },
            FormField::SubChain {
                key: "default_chain",
                label: "Default",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("expression").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let template = config.str("expression").unwrap_or_default();
        let value = ctx.arg_stack.interpolate(template);

        let (matched_index, steps) = select_case(config, &value);
        let arm = if matched_index >= 0 {
            format!("case{matched_index}")
        } else {
            "default".to_owned()
        };

        let base = ctx.arg_stack.clone().set(
            "switch.matched_case_index".to_owned(),
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
                    "switch.matched_case_index".to_owned(),
                    Variant::Int(matched_index),
                );
                (timer.finish(outcome), Some(stack))
            }
            Err(e) => (timer.failed(format!("core.logic.switch_case: {e}")), None),
        }
    }
}

/// Resolves the switch selector to a `(matched_index, chain)` pair: the first case
/// whose `match` value (a single value or any element of a value list) equals the
/// selector by display form wins, otherwise the default chain runs with index -1.
fn select_case(config: &SubActionConfig, value: &str) -> (i64, Vec<SubActionStep>) {
    let cases = config.get("cases").and_then(Variant::as_array);
    if let Some(cases) = cases {
        for (index, case) in cases.iter().enumerate() {
            let Some(case) = case.as_object() else {
                continue;
            };
            if case_matches(case.get("match"), value) {
                return (index as i64, decode_steps(case.get("chain")));
            }
        }
    }
    (-1, decode_chain(config, "default_chain"))
}

fn case_matches(candidate: Option<&Variant>, value: &str) -> bool {
    match candidate {
        Some(Variant::Array(items)) => items.iter().any(|item| item.to_string() == value),
        Some(single) => single.to_string() == value,
        None => false,
    }
}
