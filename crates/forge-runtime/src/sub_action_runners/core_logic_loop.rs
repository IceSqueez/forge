use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ChainExecutor, ChainLayout, ChainSignal, ChoiceField, CodeField, CodeKind, CodeLanguage,
    ConfigChoice, ConfigProblem, ConfigSpec, ControlSignal, IntegerEditor, IntegerField,
    LOOP_KIND_ID, RunContext, StepChainField, StepTimer, StopMark, SubActionCategory, SubActionIo,
    SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{
    ArgStack, EventId, SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry,
    UserText, Variant, strip_var_decoration,
};

use super::core_logic_shared::{chain_at, retag};
use crate::ConditionGate;

const MAX_COUNT: i64 = 1000;
const DEFAULT_COUNT: i64 = 1;
const MAX_WHILE_ITERATIONS: i64 = 1_000_000;
const DEFAULT_MAX_ITERATIONS: i64 = 1000;
const MAX_FOREACH_ITERATIONS: i64 = 10_000;
const MAX_RECORDED_ITERATIONS: i64 = 100;
const LOOP_INDEX_VAR: &str = "loop.index";
const LOOP_ITEM_VAR: &str = "loop.item";
const LOOP_ITERATIONS_COMPLETED_VAR: &str = "loop.iterations_completed";
const LOOP_EXIT_REASON_VAR: &str = "loop.exit_reason";

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    Count,
    ForeachArray,
    While,
}

impl ConfigChoice for LoopMode {
    const ALL: &'static [Self] = &[Self::Count, Self::ForeachArray, Self::While];
    const WIRE_NAMES: &'static [&'static str] = &["count", "foreach_array", "while"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::ForeachArray => "foreach_array",
            Self::While => "while",
        }
    }
}

const MODE: ChoiceField<LoopMode> = ChoiceField {
    key: "mode",
    label: "Mode",
    default: LoopMode::Count,
};

const COUNT: IntegerField = IntegerField {
    key: "count",
    label: "Count",
    editor: IntegerEditor::Number,
    default: DEFAULT_COUNT,
    bounds: ValueBounds {
        min: Some(1),
        max: Some(MAX_COUNT),
    },
};

const ARRAY_SOURCE: TextField = TextField {
    key: "array_source",
    label: "Array Variable",
    editor: TextEditor::Line {
        placeholder: "my_list",
    },
    default: "",
    required: false,
};

const WHILE_CONDITION: CodeField = CodeField {
    key: "while_condition",
    label: "While Condition",
    language: CodeLanguage::Rhai,
    kind: CodeKind::Expression,
    default: "",
    required: false,
};

const MAX_ITERATIONS: IntegerField = IntegerField {
    key: "max_iterations",
    label: "Max Iterations",
    editor: IntegerEditor::Number,
    default: DEFAULT_MAX_ITERATIONS,
    bounds: ValueBounds {
        min: Some(0),
        max: Some(MAX_WHILE_ITERATIONS),
    },
};

const BODY: StepChainField = StepChainField {
    key: "body",
    label: "Body",
    layout: ChainLayout::Steps,
    seeds_empty_list: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &MODE,
    &COUNT,
    &ARRAY_SOURCE,
    &WHILE_CONDITION,
    &MAX_ITERATIONS,
    &BODY,
]);

struct LoopConfig {
    mode: LoopMode,
    count: i64,
    array_source: String,
    while_condition: String,
    max_iterations: i64,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<LoopConfig, ConfigProblem> {
    Ok(LoopConfig {
        mode: MODE.read(config, args)?,
        count: COUNT.read(config, args)?,
        array_source: strip_var_decoration(&ARRAY_SOURCE.read(config, args)?),
        while_condition: WHILE_CONDITION.read(config, args)?,
        max_iterations: MAX_ITERATIONS.read(config, args)?,
    })
}

pub struct CoreLogicLoopRunner {
    gate: Arc<ConditionGate>,
}

impl CoreLogicLoopRunner {
    pub fn new(gate: Arc<ConditionGate>) -> Self {
        Self { gate }
    }
}

enum IterOutcome {
    Continue(ArgStack),
    Break(ArgStack),
    Stop(ArgStack, StopMark),
    Fail(UserText),
    Abort(ArgStack),
}

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
            IterOutcome::Fail(UserText::unlocalized(format!("core.logic.loop: {e}"))),
            Vec::new(),
        ),
    }
}

#[async_trait]
impl SubActionRunner for CoreLogicLoopRunner {
    fn id(&self) -> &str {
        LOOP_KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                LOOP_ITERATIONS_COMPLETED_VAR.to_owned(),
                LOOP_EXIT_REASON_VAR.to_owned(),
            ],
            body_locals: vec![LOOP_INDEX_VAR.to_owned(), LOOP_ITEM_VAR.to_owned()],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let LoopConfig {
            mode,
            count,
            array_source,
            while_condition,
            max_iterations,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let body = chain_at(config, BODY.key, ctx);
        let items: Vec<Variant> = ctx
            .arg_stack
            .get(&array_source)
            .and_then(Variant::as_array)
            .map(<[Variant]>::to_vec)
            .unwrap_or_default();

        let mut current = ctx.arg_stack.clone();
        let mut iterations: i64 = 0;
        let mut exit_reason = "completed";
        let mut failure: Option<UserText> = None;
        let mut unrecorded: Option<(StepTimer, i64)> = None;

        loop {
            if ctx.cancel.is_cancelled() {
                break;
            }

            let iter_stack = match mode {
                LoopMode::ForeachArray => {
                    let Some(item) = usize::try_from(iterations)
                        .ok()
                        .and_then(|position| items.get(position))
                    else {
                        break;
                    };
                    if iterations >= MAX_FOREACH_ITERATIONS {
                        exit_reason = "foreach_limit";
                        failure = Some(UserText::unlocalized(format!(
                            "core.logic.loop: foreach stopped after {MAX_FOREACH_ITERATIONS} iterations; the array has {} items",
                            items.len()
                        )));
                        break;
                    }
                    current
                        .clone()
                        .set(LOOP_INDEX_VAR.to_owned(), Variant::Int(iterations))
                        .set(LOOP_ITEM_VAR.to_owned(), item.clone())
                }
                LoopMode::While => {
                    if iterations >= max_iterations {
                        exit_reason = "max_iterations";
                        break;
                    }
                    match self
                        .gate
                        .evaluate_with_args(&while_condition, &current)
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(e) => {
                            failure = Some(UserText::unlocalized(format!("core.logic.loop: {e}")));
                            break;
                        }
                    }
                    current
                        .clone()
                        .set(LOOP_INDEX_VAR.to_owned(), Variant::Int(iterations))
                }
                LoopMode::Count => {
                    if iterations >= count {
                        break;
                    }
                    current
                        .clone()
                        .set(LOOP_INDEX_VAR.to_owned(), Variant::Int(iterations))
                }
            };

            let iter_no = iterations;
            if iter_no >= MAX_RECORDED_ITERATIONS && unrecorded.is_none() {
                unrecorded = Some((StepTimer::start(ctx, String::new()), iter_no));
            }
            let (iter_outcome, body_telemetry) =
                run_body(ctx.executor, &body, iter_stack, ctx.parent_event_id).await;
            if iter_no < MAX_RECORDED_ITERATIONS {
                ctx.telemetry
                    .extend(retag(body_telemetry, ctx.index, &format!("body#{iter_no}")));
            } else if let Some((_, last_iteration)) = unrecorded.as_mut() {
                *last_iteration = iter_no;
            }

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

        if let Some((timer, last_iteration)) = unrecorded {
            let mut summary = timer.skipped(UserText::unlocalized(format!(
                "core.logic.loop: steps of iterations {MAX_RECORDED_ITERATIONS}-{last_iteration} are not recorded"
            )));
            summary.kind = format!(
                "{}.body#{MAX_RECORDED_ITERATIONS}-{last_iteration}",
                ctx.index
            );
            summary.index = SubActionTelemetry::NESTED;
            ctx.telemetry.extend([summary]);
        }

        let stack = current
            .set(
                LOOP_ITERATIONS_COMPLETED_VAR.to_owned(),
                Variant::Int(iterations),
            )
            .set(
                LOOP_EXIT_REASON_VAR.to_owned(),
                Variant::String(exit_reason.to_owned()),
            );

        let outcome = match failure {
            Some(msg) => SubActionOutcome::Failed(msg),
            None => SubActionOutcome::Success,
        };

        (timer.finish(outcome), Some(stack))
    }
}
