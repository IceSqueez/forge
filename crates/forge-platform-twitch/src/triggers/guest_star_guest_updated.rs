use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct GuestStarGuestUpdatedDescriptor;

impl TriggerKindDescriptor for GuestStarGuestUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.guest_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star guest updated"
    }

    fn summary(&self) -> &str {
        "Fires when a Guest Star guest changes state in the session"
    }

    fn search_text(&self) -> &str {
        "twitch guest star guest update state slot invited accepted ready backstage live removed"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        // state_filter="" fires on every state transition.
        // Named states from Twitch Guest Star BETA docs cover the catalog's
        // conceptual split (guest_joined=>"live", guest_left=>"removed",
        // slot_stripped=>"removed", etc.) as a single configurable filter.
        vec![FormField::Text {
            key: "state_filter",
            label: "Guest state (empty = any)",
            placeholder: "e.g. live",
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let state = config
            .get("state_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("");
        if state.is_empty() {
            "any state".to_owned()
        } else {
            format!("state = {}", state)
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.guest_star_guest.update".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let filter = config
            .get("state_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("");

        if filter.is_empty() {
            return true;
        }

        let event_state = event
            .payload
            .get("guest_star")
            .and_then(|gs| gs.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        event_state == filter
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let guest_star = event.payload.get("guest_star");
        let guest = event.payload.get("guest");

        let session_id = guest_star
            .and_then(|gs| gs.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let slot_id = guest_star
            .and_then(|gs| gs.get("slot_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let state = guest_star
            .and_then(|gs| gs.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let guest_login = guest
            .and_then(|g| g.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let guest_id = guest
            .and_then(|g| g.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "guest_star.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set("guest_star.slot_id".to_owned(), Variant::String(slot_id))
            .set("guest_star.state".to_owned(), Variant::String(state))
            .set("guest.login".to_owned(), Variant::String(guest_login))
            .set("guest.id".to_owned(), Variant::String(guest_id))
    }
}
