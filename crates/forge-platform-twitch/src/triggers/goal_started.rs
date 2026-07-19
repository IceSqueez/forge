use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct GoalStartedDescriptor;

impl TriggerKindDescriptor for GoalStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.goal.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Goals
    }

    fn label(&self) -> &str {
        "Goal started"
    }

    fn summary(&self) -> &str {
        "Fires when a creator goal begins on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch goal started begin follower subscription"
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
            kind_prefix: Some("channel.goal.begin".to_owned()),
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
        let description = goal
            .and_then(|v| v.get("description"))
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
        let started_at = goal
            .and_then(|v| v.get("started_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("goal.id".to_owned(), Variant::String(goal_id))
            .set("goal.type".to_owned(), Variant::String(goal_type))
            .set("goal.description".to_owned(), Variant::String(description))
            .set(
                "goal.current_amount".to_owned(),
                Variant::Int(current_amount),
            )
            .set("goal.target_amount".to_owned(), Variant::Int(target_amount))
            .set("goal.started_at".to_owned(), Variant::String(started_at))
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
                        name: "goal.description".to_owned(),
                        kind: VariantKind::String,
                        label: "Goal description".to_owned(),
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
                        name: "goal.started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
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

    fn goal_begin_event() -> Event {
        let payload = serde_json::json!({
            "goal": {
                "id": "goal-1",
                "type": "follower",
                "description": "Road to 1k",
                "current_amount": 250,
                "target_amount": 1000,
                "started_at": "2026-06-13T18:00:00Z",
            },
        });
        Event::new(EventSource::Twitch, "channel.goal.begin", payload)
    }

    #[test]
    fn event_filter_targets_goal_begin_topic_from_twitch() {
        let filter = GoalStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.goal.begin"));
    }

    #[test]
    fn build_arg_stack_maps_amounts_as_int_and_metadata_as_string() {
        let stack = GoalStartedDescriptor.build_arg_stack(&goal_begin_event());
        assert_eq!(
            stack.get("goal.id"),
            Some(&Variant::String("goal-1".to_owned()))
        );
        assert_eq!(
            stack.get("goal.type"),
            Some(&Variant::String("follower".to_owned()))
        );
        assert_eq!(
            stack.get("goal.description"),
            Some(&Variant::String("Road to 1k".to_owned()))
        );
        assert_eq!(stack.get("goal.current_amount"), Some(&Variant::Int(250)));
        assert_eq!(stack.get("goal.target_amount"), Some(&Variant::Int(1000)));
        assert_eq!(
            stack.get("goal.started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
    }
}
