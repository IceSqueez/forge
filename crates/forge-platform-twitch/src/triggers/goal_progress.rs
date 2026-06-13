use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

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

        ArgStack::new()
            .set("goal.id".to_owned(), Variant::String(goal_id))
            .set("goal.type".to_owned(), Variant::String(goal_type))
            .set(
                "goal.current_amount".to_owned(),
                Variant::Int(current_amount),
            )
            .set("goal.target_amount".to_owned(), Variant::Int(target_amount))
    }
}
