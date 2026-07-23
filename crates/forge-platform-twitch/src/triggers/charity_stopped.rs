use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, VariableSchema};

use super::charity_started::{build_charity_lifecycle_arg_stack, build_charity_lifecycle_schema};

pub(crate) struct CharityStoppedDescriptor;

impl TriggerKindDescriptor for CharityStoppedDescriptor {
    fn id(&self) -> &str {
        "twitch.support.charity_stopped"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Charity
    }

    fn label(&self) -> &str {
        "Charity campaign stopped"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster ends a charity campaign"
    }

    fn search_text(&self) -> &str {
        "twitch charity campaign stop end finish"
    }

    fn icon_name(&self) -> &str {
        "heart"
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
            kind_prefix: Some("twitch.channel.charity_campaign.stop".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_charity_lifecycle_arg_stack(event)
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some(build_charity_lifecycle_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_targets_stop_topic_on_twitch_source() {
        let filter = CharityStoppedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.charity_campaign.stop")
        );
    }
}
