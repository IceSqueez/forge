use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{EventSource, GlobalDecremented, TypedEvent};
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use super::core_globals_shared::{AMOUNT, GLOBAL_BY_KEY, global_name};
use crate::core_messages;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_KEY, &AMOUNT]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, i64), ConfigProblem> {
    Ok((
        global_name(&GLOBAL_BY_KEY, config, args)?,
        AMOUNT.read(config, args)?,
    ))
}

pub(crate) struct CoreGlobalsDecrementRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsDecrementRunner {
    pub(crate) fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsDecrementRunner {
    fn id(&self) -> &str {
        "core.globals.decrement"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Decrement Global"
    }

    fn summary(&self) -> &str {
        "Decrement a numeric global variable by an amount"
    }

    fn search_text(&self) -> &str {
        "decrement global variable counter subtract minus"
    }

    fn icon_name(&self) -> &str {
        "minus"
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
        let (resolved_key, amount) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let Some(delta) = amount.checked_neg() else {
            return (
                timer.failed(core_messages::amount_not_negatable(amount).into()),
                None,
            );
        };

        let outcome = match self.globals.incr(&resolved_key, delta).await {
            Ok(new_val) => {
                let new_val_json = match &new_val {
                    Variant::Int(i) => serde_json::Value::from(*i),
                    _ => serde_json::Value::String(new_val.to_string()),
                };
                GlobalDecremented {
                    key: resolved_key.clone(),
                    delta: amount,
                    new_value: new_val_json,
                }
                .publish(
                    ctx.publisher,
                    EventSource::Core,
                    Some(ctx.parent_event_id),
                );
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MapGlobals;
    use crate::test_support::failure::failure_message;
    use forge_events::Event;
    use forge_events::EventPublisher;
    use forge_events::test_support::NullPublisher;
    use forge_types::EventId;
    use std::sync::Mutex;

    struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);
    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn cfg(key: &str, amount: i64) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("key".to_owned(), Variant::String(key.to_owned()));
        c.insert("amount".to_owned(), Variant::Int(amount));
        c
    }

    async fn run(globals: Arc<MapGlobals>, config: &SubActionConfig) -> SubActionOutcome {
        let runner = CoreGlobalsDecrementRunner::new(globals);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner.execute(config, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn decrement_applies_negated_amount_to_int_global() {
        for (start, amount, expected) in [(10, 3, 7), (10, -5, 15), (0, 1, -1)] {
            let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(start))]));
            let outcome = run(globals.clone(), &cfg("counter", amount)).await;
            assert!(
                matches!(outcome, SubActionOutcome::Success),
                "{start} - {amount} should succeed"
            );
            assert_eq!(
                globals.snapshot("counter"),
                Some(Variant::Int(expected)),
                "{start} - {amount} should store {expected}"
            );
        }
    }

    #[tokio::test]
    async fn decrement_missing_key_reports_failed() {
        let globals = Arc::new(MapGlobals::default());
        let outcome = run(globals, &cfg("ghost", 1)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn decrement_non_numeric_global_reports_failed() {
        let globals = Arc::new(MapGlobals::with([(
            "counter",
            Variant::String("nope".to_owned()),
        )]));
        let outcome = run(globals.clone(), &cfg("counter", 1)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            globals.snapshot("counter"),
            Some(Variant::String("nope".to_owned())),
            "value must be untouched on failure"
        );
    }

    #[tokio::test]
    async fn decrement_by_i64_min_fails_and_leaves_the_global_untouched() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(5))]));
        let outcome = run(globals.clone(), &cfg("counter", i64::MIN)).await;
        let reason = failure_message(&outcome);
        assert_eq!(reason.code(), &core_messages::AMOUNT_NOT_NEGATABLE);
        assert_eq!(
            reason.arg("amount"),
            Some(&forge_types::MessageArg::Int(i64::MIN))
        );
        assert_eq!(globals.snapshot("counter"), Some(Variant::Int(5)));
    }

    async fn capture(globals: Arc<MapGlobals>, config: &SubActionConfig) -> Vec<Event> {
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = CapturingPublisher(Arc::clone(&events));
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
        let runner = CoreGlobalsDecrementRunner::new(globals);
        let _ = runner.execute(config, &ctx).await;
        let captured = events.lock().unwrap();
        captured.clone()
    }

    #[tokio::test]
    async fn decrement_runner_emits_global_decremented_with_positive_delta() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(10))]));
        let events = capture(globals, &cfg("counter", 3)).await;
        let ev = &events[0];
        assert_eq!(ev.kind, "global.decremented");
        assert_eq!(ev.payload["key"], "counter");
        assert_eq!(ev.payload["delta"].as_i64(), Some(3));
        assert_eq!(ev.payload["new_value"].as_i64(), Some(7));
    }

    #[tokio::test]
    async fn decrement_never_emits_incremented_with_negative_delta() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(10))]));
        let events = capture(globals, &cfg("counter", 4)).await;
        assert!(
            !events.iter().any(|e| e.kind.contains("incr")),
            "decrement must never masquerade as an increment"
        );
        assert!(
            events
                .iter()
                .all(|e| e.payload["delta"].as_i64() != Some(-4)),
            "sign lives in the verb; delta stays positive"
        );
    }
}
