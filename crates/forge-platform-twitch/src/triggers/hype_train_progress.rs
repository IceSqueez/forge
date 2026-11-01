use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, FormField, SubActionConfigExt, TriggerCategory};
use forge_types::{SynthesisHint, TriggerConfig, UserText, Variant};

use crate::vocabulary::fields::hype_train as fields;

fn default_config() -> TriggerConfig {
    let mut cfg = TriggerConfig::new();
    cfg.insert("min_level".to_owned(), Variant::Int(1));
    cfg
}

fn config_fields() -> Vec<FormField> {
    vec![FormField::Integer {
        key: "min_level",
        label: "Minimum level",
        min: 1,
        max: super::HYPE_TRAIN_LEVEL_MAX,
    }]
}

fn condition_display(config: &TriggerConfig) -> UserText {
    let min = match config.get("min_level") {
        Some(Variant::Int(n)) => *n,
        _ => 1,
    };
    messages::condition_level_at_least(min).into()
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    let min_level = config.int("min_level").unwrap_or(1);

    let level = event
        .payload
        .get(fields::HYPE)
        .and_then(|h| h.get(fields::LEVEL))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    level >= min_level
}

const VARIABLES: &[Variable] = &[
    Variable::nested_text("hype.id", fields::HYPE, fields::HYPE_ID),
    Variable::nested_int("hype.level", fields::HYPE, fields::LEVEL).synthesis(
        SynthesisHint::BoundedInt {
            min: 1,
            max: super::HYPE_TRAIN_LEVEL_MAX,
        },
    ),
    Variable::nested_int("hype.goal", fields::HYPE, fields::GOAL).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_int("hype.progress", fields::HYPE, fields::PROGRESS).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_int("hype.total", fields::HYPE, fields::TOTAL).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_HYPE_TRAIN_PROGRESS,
    category: TriggerCategory::Hype,
    label: "Hype Train progress",
    summary: "Fires on each Hype Train progress update at or above the configured level",
    search_text: "twitch hype train progress level update",
    icon: "train",
    kind: event_kinds::CHANNEL_HYPE_TRAIN_PROGRESS,
    actors: ActorDeclaration::Actorless,
    variables,
    matches,
    default_config,
    config_fields,
    condition_display,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn progress_event(level: i64) -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-3",
                "level": level,
                "goal": 1000,
                "progress": 600,
                "total": 600,
            }
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.hype_train.progress",
            payload,
        )
    }

    fn config_with_min_level(min: i64) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("min_level".to_owned(), Variant::Int(min));
        cfg
    }

    #[test]
    fn matches_trigger_fires_only_when_level_at_or_above_min_level() {
        let cases = [
            ("min below event level", config_with_min_level(1), true),
            (
                "min equals event level (boundary, >=)",
                config_with_min_level(3),
                true,
            ),
            ("min one above event level", config_with_min_level(4), false),
        ];
        for (name, cfg, expected) in cases {
            assert_eq!(
                SPEC.matches_trigger(&cfg, &progress_event(3)),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn matches_trigger_uses_default_min_level_of_one_when_config_absent() {
        let cfg = SPEC.default_config();
        assert!(SPEC.matches_trigger(&cfg, &progress_event(3)));
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_progress_fields() {
        let stack = SPEC.build_arg_stack(&progress_event(3));
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-3".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(3)));
        assert_eq!(stack.get("hype.goal"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("hype.progress"), Some(&Variant::Int(600)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(600)));
    }
}
