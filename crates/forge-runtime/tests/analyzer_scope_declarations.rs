#![allow(clippy::expect_used)]

use std::sync::Arc;

use forge_events::{Event, EventPublisher};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, StepHealth, SubActionRegistry, TriggerCategory,
    TriggerKindDescriptor, TriggerRegistry, TriggerVariables, analyze,
};
use forge_runtime::sub_action_runners::{
    CoreArgsSetRunner, CoreFileWriteRunner, CoreLogicLoopRunner, CoreLogicWaitUntilRunner,
    CoreMathEvaluateRunner, CoreTimeNowRunner, ServerBroadcastRunner,
};
use forge_runtime::{ConditionGate, Config};
use forge_types::{
    Action, ActionId, DeclaredVariable, ExecutionMode, PermissionRung, PlatformId, PlatformScope,
    QueueId, SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId,
    Variant, VariantKind,
};

const SEED_TRIGGER: &str = "stub.seed";

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct SeedTrigger;

impl TriggerKindDescriptor for SeedTrigger {
    fn id(&self) -> &str {
        SEED_TRIGGER
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        SEED_TRIGGER
    }

    fn summary(&self) -> &str {
        SEED_TRIGGER
    }

    fn search_text(&self) -> &str {
        SEED_TRIGGER
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn condition_display(&self, _: &TriggerConfig) -> String {
        String::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(TriggerVariables::new().event_specific(
            DeclaredVariable {
                name: "seed".to_owned(),
                kind: VariantKind::String,
                label: "Seed".to_owned(),
                synthesis: None,
            },
            |_| Variant::String(String::new()),
        ))
    }
}

fn sub_actions() -> SubActionRegistry {
    let gate = Arc::new(ConditionGate::new(&Config::default()));
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(CoreLogicLoopRunner::new(Arc::clone(&gate))))
        .expect("the loop runner registers");
    reg.register(Box::new(CoreLogicWaitUntilRunner::new(gate)))
        .expect("the wait-until runner registers");
    reg.register(Box::new(CoreTimeNowRunner))
        .expect("the time runner registers");
    reg.register(Box::new(CoreFileWriteRunner))
        .expect("the file-write runner registers");
    reg.register(Box::new(ServerBroadcastRunner::new(Arc::new(
        NullPublisher,
    ))))
    .expect("the broadcast runner registers");
    reg.register(Box::new(CoreArgsSetRunner))
        .expect("the argument runner registers");
    reg.register(Box::new(CoreMathEvaluateRunner::new()))
        .expect("the math runner registers");
    reg
}

fn triggers() -> TriggerRegistry {
    let mut reg = TriggerRegistry::new();
    reg.register(Box::new(SeedTrigger))
        .expect("the seed trigger registers");
    reg
}

fn step(kind_id: &str, config: &[(&str, Variant)]) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: config
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn consuming(names: &[&str]) -> SubActionStep {
    let template = names
        .iter()
        .map(|name| format!("%{name}%"))
        .collect::<String>();
    step("chat.send", &[("message", Variant::String(template))])
}

fn health_of(steps: Vec<SubActionStep>) -> Vec<StepHealth> {
    let action = Action {
        id: ActionId::new(),
        name: "scope".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    };
    let seed = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: SEED_TRIGGER.to_owned(),
        name: SEED_TRIGGER.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: false,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::default(),
    };
    analyze(
        &action,
        &[seed],
        &vec![None; action.sub_actions.len()],
        &sub_actions(),
        &triggers(),
    )
}

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

#[test]
fn every_variable_a_core_step_writes_is_in_scope_for_the_steps_after_it() {
    for (producer, written) in [
        (
            step("core.logic.loop", &[]),
            vec!["loop.iterations_completed", "loop.exit_reason"],
        ),
        (
            step("core.logic.wait_until", &[]),
            vec!["wait.elapsed_ms", "wait.timed_out"],
        ),
        (
            step("core.time.now", &[]),
            vec!["time.formatted", "time.unix_seconds"],
        ),
        (step("core.file.write", &[]), vec!["file.bytes_written"]),
        (
            step("server.broadcast", &[]),
            vec!["broadcast.delivered_count"],
        ),
        (
            step("core.args.set", &[("name", s("greeting"))]),
            vec!["greeting"],
        ),
        (
            step("core.math.evaluate", &[("into_var", s("total"))]),
            vec!["total"],
        ),
    ] {
        let kind_id = producer.kind_id.clone();
        let health = health_of(vec![producer, consuming(&written)]);
        assert_eq!(health[1], StepHealth::default(), "{kind_id}");
    }
}

#[test]
fn a_loop_body_sees_the_index_and_item_the_loop_sets_each_iteration() {
    let body = Variant::Array(vec![Variant::Object(SubActionConfig::from([
        ("kind_id".to_owned(), s("chat.send")),
        (
            "config".to_owned(),
            Variant::Object(SubActionConfig::from([(
                "message".to_owned(),
                s("%loop.index%%loop.item%"),
            )])),
        ),
    ]))]);

    let health = health_of(vec![step("core.logic.loop", &[("body", body)])]);

    assert_eq!(health, vec![StepHealth::default()]);
}
