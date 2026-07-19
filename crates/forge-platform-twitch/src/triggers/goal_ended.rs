use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct GoalEndedDescriptor;

impl TriggerKindDescriptor for GoalEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.goal.ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Goals
    }

    fn label(&self) -> &str {
        "Goal ended"
    }

    fn summary(&self) -> &str {
        "Fires when a creator goal ends, whether achieved or not"
    }

    fn search_text(&self) -> &str {
        "twitch goal ended completed achieved follower subscription"
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
            kind_prefix: Some("channel.goal.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let goal = event.payload.get("goal");

        let goal_id = goal
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let goal_type = goal
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let current_amount = goal
            .and_then(|v| v.get("current_amount"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let target_amount = goal
            .and_then(|v| v.get("target_amount"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let is_achieved = goal
            .and_then(|v| v.get("is_achieved"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let ended_at = goal
            .and_then(|v| v.get("ended_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("goal.id".to_owned(), Variant::String(goal_id))
            .set("goal.type".to_owned(), Variant::String(goal_type))
            .set(
                "goal.current_amount".to_owned(),
                Variant::Int(current_amount),
            )
            .set("goal.target_amount".to_owned(), Variant::Int(target_amount))
            .set("goal.is_achieved".to_owned(), Variant::Bool(is_achieved))
            .set("goal.ended_at".to_owned(), Variant::String(ended_at))
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
                    DeclaredVariable {
                        name: "goal.is_achieved".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Goal achieved".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "goal.ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ended at".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal_end_event(is_achieved: bool) -> Event {
        let payload = serde_json::json!({
            "goal": {
                "id": "goal-3",
                "type": "follower",
                "current_amount": 1000,
                "target_amount": 1000,
                "is_achieved": is_achieved,
                "ended_at": "2026-06-13T19:00:00Z",
            },
        });
        Event::new(EventSource::Twitch, "channel.goal.end", payload)
    }

    #[test]
    fn event_filter_targets_goal_end_topic_from_twitch() {
        let filter = GoalEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.goal.end"));
    }

    #[test]
    fn build_arg_stack_maps_amounts_as_int_and_is_achieved_as_bool() {
        let stack = GoalEndedDescriptor.build_arg_stack(&goal_end_event(true));
        assert_eq!(
            stack.get("goal.id"),
            Some(&Variant::String("goal-3".to_owned()))
        );
        assert_eq!(stack.get("goal.current_amount"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("goal.target_amount"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("goal.is_achieved"), Some(&Variant::Bool(true)));
        assert_eq!(
            stack.get("goal.ended_at"),
            Some(&Variant::String("2026-06-13T19:00:00Z".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_preserves_unachieved_goal_as_bool_false() {
        let stack = GoalEndedDescriptor.build_arg_stack(&goal_end_event(false));
        assert_eq!(stack.get("goal.is_achieved"), Some(&Variant::Bool(false)));
    }
}
