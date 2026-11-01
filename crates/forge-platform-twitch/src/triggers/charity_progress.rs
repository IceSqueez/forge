use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::{ActorDeclaration, TriggerCategory};

use super::families;

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_CHARITY_PROGRESS,
    category: TriggerCategory::Charity,
    label: "Charity campaign progress",
    summary: "Fires on each charity campaign progress update",
    search_text: "twitch charity campaign progress update",
    icon: "heart",
    kind: event_kinds::CHANNEL_CHARITY_CAMPAIGN_PROGRESS,
    actors: ActorDeclaration::Actorless,
    variables: families::charity_variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    #[test]
    fn event_filter_targets_progress_topic_on_twitch_source() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.charity_campaign.progress")
        );
    }
}
