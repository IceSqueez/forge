use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

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
}
