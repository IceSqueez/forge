use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::goal as fields;

pub(crate) struct GoalProgressDescriptor;

impl TriggerKindDescriptor for GoalProgressDescriptor {
    fn id(&self) -> &str {
        "twitch.goal.progress"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Goals
    }

    fn label(&self) -> &str {
        "Goal progress"
    }

    fn summary(&self) -> &str {
        "Fires when a creator goal receives a progress update"
    }

    fn search_text(&self) -> &str {
        "twitch goal progress update follower subscription"
    }

    fn icon_name(&self) -> &str {
        "flag"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.goal.progress".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let goal = event.payload.get(fields::GOAL);

        let goal_id = goal
            .and_then(|v| v.get(fields::GOAL_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let goal_type = goal
            .and_then(|v| v.get(fields::GOAL_TYPE))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let current_amount = goal
            .and_then(|v| v.get(fields::CURRENT_AMOUNT))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let target_amount = goal
            .and_then(|v| v.get(fields::TARGET_AMOUNT))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set("goal.id".to_owned(), Variant::String(goal_id))
            .set("goal.type".to_owned(), Variant::String(goal_type))
            .set(
                "goal.current_amount".to_owned(),
                Variant::Int(current_amount),
            )
            .set("goal.target_amount".to_owned(), Variant::Int(target_amount))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "goal.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Goal ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "goal.type".to_owned(),
                        kind: VariantKind::String,
                        label: "Goal type".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "goal.current_amount".to_owned(),
                        kind: VariantKind::Int,
                        label: "Current amount".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    DeclaredVariable {
                        name: "goal.target_amount".to_owned(),
                        kind: VariantKind::Int,
                        label: "Target amount".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal_progress_event() -> Event {
        let payload = serde_json::json!({
            "goal": {
                "id": "goal-2",
                "type": "subscription",
                "current_amount": 42,
                "target_amount": 100,
            },
        });
        Event::new(EventSource::Twitch, "channel.goal.progress", payload)
    }

    #[test]
    fn event_filter_targets_goal_progress_topic_from_twitch() {
        let filter = GoalProgressDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.goal.progress"));
    }

    #[test]
    fn build_arg_stack_maps_progress_amounts_as_int() {
        let stack = GoalProgressDescriptor.build_arg_stack(&goal_progress_event());
        assert_eq!(
            stack.get("goal.id"),
            Some(&Variant::String("goal-2".to_owned()))
        );
        assert_eq!(
            stack.get("goal.type"),
            Some(&Variant::String("subscription".to_owned()))
        );
        assert_eq!(stack.get("goal.current_amount"), Some(&Variant::Int(42)));
        assert_eq!(stack.get("goal.target_amount"), Some(&Variant::Int(100)));
    }
}
