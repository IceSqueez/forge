use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::{ActorDeclaration, TriggerCategory};

use super::families;

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_CHARITY_STOPPED,
    category: TriggerCategory::Charity,
    label: "Charity campaign stopped",
    summary: "Fires when the broadcaster ends a charity campaign",
    search_text: "twitch charity campaign stop end finish",
    icon: "heart",
    kind: event_kinds::CHANNEL_CHARITY_CAMPAIGN_STOP,
    actors: ActorDeclaration::Actorless,
    variables: families::charity_variables,
    ..TwitchTriggerSpec::BASE
};
