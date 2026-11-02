use std::sync::Arc;

use forge_events::{Delivery, Event};
use forge_net::TaskOwner;
use forge_overlay::RowRemoval;
use forge_types::ChatModerationPayload;

use crate::delivery::CHAT_OVERLAYS;
use crate::event_flow::bus::EventBus;
use crate::overlays::overlay_service::OverlayServiceHandle;

pub fn spawn_chat_removal_feed(
    bus: &Arc<EventBus>,
    overlays: OverlayServiceHandle,
    tasks: &TaskOwner,
) {
    let mut subscription = bus.subscribe_observer(CHAT_OVERLAYS);
    tasks.spawn_until_stopped(async move {
        loop {
            match subscription.next().await {
                Delivery::Event(event) => {
                    if let Some(removal) = removal_of(&event) {
                        overlays.remove_chat_rows(&removal).await;
                    }
                }
                Delivery::Skipped(missed) => {
                    tracing::warn!(missed, "chat overlay removals fell behind the bus");
                }
                Delivery::Closed => break,
            }
        }
    });
}

pub fn removal_of(event: &Event) -> Option<RowRemoval> {
    let platform = event.source.to_platform_id()?;
    let value = event.payload.get(ChatModerationPayload::KEY)?;
    let payload: ChatModerationPayload = serde_json::from_value(value.clone()).ok()?;
    RowRemoval::from_moderation(platform, &payload.action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_overlay::RowRemovalScope;
    use forge_types::{ChatModerationAction, PlatformId, UserRemoval};

    fn moderation_event(source: EventSource, action: ChatModerationAction) -> Event {
        let mut payload = serde_json::json!({});
        ChatModerationPayload { action }.attach(&mut payload);
        Event::new(source, "moderation", payload)
    }

    #[test]
    fn each_platform_event_maps_to_a_removal_for_its_own_platform() {
        for (source, platform) in [
            (EventSource::Twitch, PlatformId::Twitch),
            (EventSource::YouTube, PlatformId::YouTube),
            (EventSource::Kick, PlatformId::Kick),
        ] {
            let event = moderation_event(
                source,
                ChatModerationAction::DeleteMessage {
                    message_id: "m-1".to_owned(),
                },
            );

            assert_eq!(
                removal_of(&event),
                Some(RowRemoval {
                    platform,
                    scope: RowRemovalScope::Message {
                        message_id: "m-1".to_owned()
                    },
                })
            );
        }
    }

    #[test]
    fn a_timeout_a_ban_and_a_clear_each_map_to_their_scope() {
        let removed = |removal| {
            moderation_event(
                EventSource::Twitch,
                ChatModerationAction::RemoveUser {
                    user_id: Some("u-1".to_owned()),
                    user_name: "viewer".to_owned(),
                    removal,
                },
            )
        };
        let user_scope = Some(RowRemoval {
            platform: PlatformId::Twitch,
            scope: RowRemovalScope::User {
                user_id: "u-1".to_owned(),
            },
        });

        assert_eq!(removal_of(&removed(UserRemoval::Timeout)), user_scope);
        assert_eq!(removal_of(&removed(UserRemoval::Ban)), user_scope);
        assert_eq!(
            removal_of(&moderation_event(
                EventSource::Twitch,
                ChatModerationAction::ClearChat
            )),
            Some(RowRemoval {
                platform: PlatformId::Twitch,
                scope: RowRemovalScope::Platform,
            })
        );
    }

    #[test]
    fn events_without_a_platform_source_or_a_moderation_envelope_map_to_nothing() {
        assert_eq!(
            removal_of(&moderation_event(
                EventSource::Core,
                ChatModerationAction::ClearChat
            )),
            None
        );
        assert_eq!(
            removal_of(&Event::new(
                EventSource::Twitch,
                "chat.message",
                serde_json::json!({})
            )),
            None
        );
    }
}
