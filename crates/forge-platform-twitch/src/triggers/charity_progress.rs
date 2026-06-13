use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig};

use super::charity_started::build_charity_lifecycle_arg_stack;

pub(crate) struct CharityProgressDescriptor;

impl TriggerKindDescriptor for CharityProgressDescriptor {
    fn id(&self) -> &str {
        "twitch.support.charity_progress"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Charity campaign progress"
    }

    fn summary(&self) -> &str {
        "Fires on each charity campaign progress update"
    }

    fn search_text(&self) -> &str {
        "twitch charity campaign progress update"
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
            kind_prefix: Some("channel.charity_campaign.progress".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_charity_lifecycle_arg_stack(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_targets_progress_topic_on_twitch_source() {
        let filter = CharityProgressDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.charity_campaign.progress")
        );
    }
}
