#![allow(clippy::expect_used)]
mod common;
use forge_events::test_support::NullPublisher;
use std::sync::Arc;

use forge_events::Event;
use forge_registry::{
    EventFilter, Finding, FormField, KindPlatformContract, StepHealth, SubActionRegistry,
    SubActionRunner, TriggerCategory, TriggerKindDescriptor, TriggerRegistry, TriggerVariables,
    analyze,
};
use forge_runtime::{ConditionGate, Config};
use forge_runtime::{
    CoreArgsSetRunner, CoreFileWriteRunner, CoreLogicLoopRunner, CoreLogicWaitUntilRunner,
    CoreMathEvaluateRunner, CoreTimeFormatRunner, CoreTimeNowRunner, CoreTimeParseRunner,
    ServerBroadcastRunner,
};
use forge_types::{
    ActionId, DeclaredVariable, PermissionRung, PlatformId, PlatformScope, QueueId,
    SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
    VariantKind,
};

const SEED_TRIGGER: &str = "stub.seed";

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
    reg.register(Box::new(CoreTimeFormatRunner))
        .expect("the time format runner registers");
    reg.register(Box::new(CoreTimeParseRunner))
        .expect("the time parse runner registers");
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
    let action = common::sequential_action(ActionId::new(), "scope", QueueId::new(), steps);
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

fn configured(runner: &dyn SubActionRunner, overrides: &[(&str, Variant)]) -> SubActionStep {
    let mut config = runner.default_config();
    for (key, value) in overrides {
        config.insert((*key).to_owned(), value.clone());
    }
    SubActionStep {
        config,
        ..step(runner.id(), &[])
    }
}

fn hour_of(name: &str) -> SubActionStep {
    step("chat.send", &[("message", s(&format!("%{name}:HH%")))])
}

#[test]
fn a_date_format_on_a_formatted_time_text_warns_while_a_captured_time_does_not() {
    for (producer, expected, case) in [
        (
            configured(&CoreTimeFormatRunner, &[("into_var", s("shown"))]),
            StepHealth {
                findings: vec![Finding::DateFormatOnNonDatetime("shown".to_owned())],
            },
            "the text a format step writes",
        ),
        (
            configured(&CoreTimeNowRunner, &[("into_var", s("shown"))]),
            StepHealth::default(),
            "the datetime a current-time step captures",
        ),
        (
            configured(&CoreTimeParseRunner, &[("into_var", s("shown"))]),
            StepHealth::default(),
            "the datetime a parse step reads",
        ),
    ] {
        let health = health_of(vec![producer, hour_of("shown")]);
        assert_eq!(health[1], expected, "{case}");
    }
}

#[test]
fn a_time_step_with_an_unusable_pattern_reports_invalid_step_config() {
    for (runner, overrides, case) in [
        (
            &CoreTimeFormatRunner as &dyn SubActionRunner,
            vec![("format_string", s("qq"))],
            "a format step with an invalid pattern",
        ),
        (
            &CoreTimeFormatRunner,
            vec![("format_string", s("[unix_timestamp]"))],
            "a format step with an untranslatable legacy pattern",
        ),
        (
            &CoreTimeParseRunner,
            vec![("format", s("qq"))],
            "a parse step with an invalid pattern",
        ),
        (
            &CoreTimeNowRunner,
            vec![("format", s("custom")), ("custom_format_string", s("qq"))],
            "a custom current-time step with an invalid pattern",
        ),
    ] {
        let placed = configured(runner, &overrides);
        let message = runner
            .validate_config(&placed.config)
            .expect_err("the fixture config is invalid")
            .into_config_problem();
        assert_eq!(
            health_of(vec![placed]),
            vec![StepHealth {
                findings: vec![Finding::InvalidStepConfig(message)],
            }],
            "{case}"
        );
    }
}

#[test]
fn a_usable_or_unused_time_pattern_raises_no_step_finding() {
    for (runner, overrides, case) in [
        (
            &CoreTimeFormatRunner as &dyn SubActionRunner,
            vec![
                ("source", s("2024-01-15T00:00:00Z")),
                ("format_string", s("[day].[month].[year] [hour]:[minute]")),
            ],
            "a format step with a translatable legacy pattern",
        ),
        (
            &CoreTimeParseRunner,
            vec![("format", s("[year]-[month]-[day]"))],
            "a parse step with a translatable legacy pattern",
        ),
        (
            &CoreTimeNowRunner,
            vec![
                ("format", s("custom")),
                ("custom_format_string", s("HH:mm")),
            ],
            "a custom current-time step with a valid pattern",
        ),
        (
            &CoreTimeNowRunner,
            vec![("format", s("iso8601")), ("custom_format_string", s("qq"))],
            "a non-custom current-time step keeping a stale invalid pattern",
        ),
    ] {
        assert_eq!(
            health_of(vec![configured(runner, &overrides)]),
            vec![StepHealth::default()],
            "{case}"
        );
    }
}
