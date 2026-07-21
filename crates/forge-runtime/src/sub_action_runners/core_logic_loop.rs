use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ChainExecutor, ChainSignal, CodeLanguage, ControlSignal, FormField, RegistryError, RunContext,
    StepTimer, StopMark, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{
    ArgStack, EventId, SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry,
    Variant,
};

use super::core_logic_shared::{decode_chain, retag};
use crate::ConditionGate;

const MAX_COUNT: i64 = 1000;
const MAX_WHILE_ITERATIONS: i64 = 1_000_000;

pub struct CoreLogicLoopRunner {
    gate: Arc<ConditionGate>,
}

impl CoreLogicLoopRunner {
    pub fn new(gate: Arc<ConditionGate>) -> Self {
        Self { gate }
    }
}

/// What a single body run tells the loop to do next. `Continue` carries the
/// threaded arg-stack forward; `Break`/`Stop`/`Abort` end the loop while keeping
/// the body's mutations; `Fail` ends it with the child error.
enum IterOutcome {
    Continue(ArgStack),
    Break(ArgStack),
    Stop(ArgStack, StopMark),
    Fail(String),
    Abort(ArgStack),
}

/// Runs one loop body pass, returning the iteration verdict alongside the body's
/// per-step telemetry so the caller can re-tag and splice it into the flat list.
async fn run_body(
    executor: &dyn ChainExecutor,
    body: &[SubActionStep],
    iter_stack: ArgStack,
    parent_event_id: EventId,
) -> (IterOutcome, Vec<SubActionTelemetry>) {
    match executor
        .run_child_chain(body, &iter_stack, parent_event_id)
        .await
    {
        Ok(child) => {
            let outcome = match child.signal {
                ChainSignal::Completed | ChainSignal::Continue => {
                    IterOutcome::Continue(child.arg_stack)
                }
                ChainSignal::Break => IterOutcome::Break(child.arg_stack),
                ChainSignal::Stop(mark) => IterOutcome::Stop(child.arg_stack, mark),
                ChainSignal::Error(msg) => IterOutcome::Fail(msg),
                ChainSignal::Aborted => IterOutcome::Abort(child.arg_stack),
            };
            (outcome, child.telemetry)
        }
        Err(e) => (
            IterOutcome::Fail(format!("core.logic.loop: {e}")),
            Vec::new(),
        ),
    }
}

#[async_trait]
impl SubActionRunner for CoreLogicLoopRunner {
    fn id(&self) -> &str {
        "core.logic.loop"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Loop"
    }

    fn summary(&self) -> &str {
        "Repeat a sub-chain a fixed count, over an array, or while a condition holds"
    }

    fn search_text(&self) -> &str {
        "loop repeat for each foreach while count iterate flow control"
    }

    fn icon_name(&self) -> &str {
        "repeat"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("mode".to_owned(), Variant::String("count".to_owned()));
        cfg.insert("count".to_owned(), Variant::Int(1));
        cfg.insert("array_source".to_owned(), Variant::String(String::new()));
        cfg.insert("while_condition".to_owned(), Variant::String(String::new()));
        cfg.insert("max_iterations".to_owned(), Variant::Int(1000));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: "mode",
                label: "Mode",
                options: &["count", "foreach_array", "while"],
            },
            FormField::Integer {
                key: "count",
                label: "Count",
                min: 1,
                max: MAX_COUNT,
            },
            FormField::Text {
                key: "array_source",
                label: "Array Variable",
                placeholder: "my_list",
            },
            FormField::Code {
                key: "while_condition",
                label: "While Condition",
                language: CodeLanguage::Rhai,
            },
            FormField::Integer {
                key: "max_iterations",
                label: "Max Iterations",
                min: 0,
                max: MAX_WHILE_ITERATIONS,
            },
            FormField::SubChain {
                key: "body",
                label: "Body",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let mode = config.str("mode").unwrap_or("count");
        let body = decode_chain(config, "body");

        let count = config.int("count").unwrap_or(1).clamp(0, MAX_COUNT);
        let max_iterations = config
            .int("max_iterations")
            .unwrap_or(1000)
            .clamp(0, MAX_WHILE_ITERATIONS);
        let while_condition = config.str("while_condition").unwrap_or_default().to_owned();
        let items: Vec<Variant> = {
            let source_template = config.str("array_source").unwrap_or_default();
            let source =
                forge_types::strip_var_decoration(&ctx.arg_stack.interpolate(source_template));
            ctx.arg_stack
                .get(&source)
                .and_then(Variant::as_array)
                .map(<[Variant]>::to_vec)
                .unwrap_or_default()
        };

        let mut current = ctx.arg_stack.clone();
        let mut iterations: i64 = 0;
        let mut exit_reason = "completed";
        let mut failure: Option<String> = None;

        loop {
            if ctx.cancel.is_cancelled() {
                break;
            }

            let iter_stack = match mode {
                "foreach_array" => {
                    let Some(item) = items.get(iterations as usize) else {
                        break;
                    };
                    current
                        .clone()
                        .set("loop.index".to_owned(), Variant::Int(iterations))
                        .set("loop.item".to_owned(), item.clone())
                }
                "while" => {
                    if iterations >= max_iterations {
                        exit_reason = "max_iterations";
                        break;
                    }
                    let expr = current.interpolate(&while_condition);
                    if !self.gate.evaluate(&expr).await.unwrap_or(false) {
                        break;
                    }
                    current
                        .clone()
                        .set("loop.index".to_owned(), Variant::Int(iterations))
                }
                _ => {
                    if iterations >= count {
                        break;
                    }
                    current
                        .clone()
                        .set("loop.index".to_owned(), Variant::Int(iterations))
                }
            };

            let iter_no = iterations;
            let (iter_outcome, body_telemetry) =
                run_body(ctx.executor, &body, iter_stack, ctx.parent_event_id).await;
            ctx.telemetry
                .extend(retag(body_telemetry, ctx.index, &format!("body#{iter_no}")));

            match iter_outcome {
                IterOutcome::Continue(stack) => {
                    current = stack;
                    iterations += 1;
                }
                IterOutcome::Break(stack) => {
                    current = stack;
                    iterations += 1;
                    exit_reason = "break";
                    break;
                }
                IterOutcome::Stop(stack, mark) => {
                    current = stack;
                    iterations += 1;
                    ctx.control.set(ControlSignal::Stop(mark));
                    break;
                }
                IterOutcome::Fail(msg) => {
                    failure = Some(msg);
                    break;
                }
                IterOutcome::Abort(stack) => {
                    current = stack;
                    break;
                }
            }
        }

        let stack = current
            .set(
                "loop.iterations_completed".to_owned(),
                Variant::Int(iterations),
            )
            .set(
                "loop.exit_reason".to_owned(),
                Variant::String(exit_reason.to_owned()),
            );

        let outcome = match failure {
            Some(msg) => SubActionOutcome::Failed(msg),
            None => SubActionOutcome::Success,
        };

        (timer.finish(outcome), Some(stack))
    }
}
