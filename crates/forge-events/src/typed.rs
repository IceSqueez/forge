use forge_types::EventId;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Event, EventPublisher, EventSource};

pub trait TypedEvent: Serialize + DeserializeOwned {
    const KIND: &'static str;

    fn into_event(self, source: EventSource, caused_by: Option<EventId>) -> Event {
        let payload = serde_json::to_value(&self).unwrap_or_else(|error| {
            tracing::error!(
                kind = Self::KIND,
                category = ?error.classify(),
                "event payload could not be encoded"
            );
            serde_json::Value::Null
        });
        match caused_by {
            Some(parent) => Event::caused_by(source, Self::KIND, payload, parent),
            None => Event::new(source, Self::KIND, payload),
        }
    }

    fn publish(
        self,
        publisher: &dyn EventPublisher,
        source: EventSource,
        caused_by: Option<EventId>,
    ) {
        publisher.publish(self.into_event(source, caused_by));
    }

    fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Self::KIND {
            return None;
        }
        match serde_json::from_value(event.payload.clone()) {
            Ok(payload) => Some(payload),
            Err(error) => {
                tracing::warn!(
                    kind = Self::KIND,
                    event = %event.id,
                    category = ?error.classify(),
                    "event payload does not match its kind"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::sync::Mutex;

    use forge_types::{ActionId, IntegrationId, QueueId, ScriptId, TriggerInstanceId};
    use serde_json::json;
    use time::OffsetDateTime;

    use super::*;
    use crate::{
        ActingSender, ActionDone, ActionSkipped, ActionStarted, ChatSendFailed, ChatSendRequest,
        ChatSent, CommandMatched, ConnectionState, GlobalArrayAppended, GlobalArrayRemoved,
        GlobalDecremented, GlobalDeleted, GlobalIncremented, GlobalSet, GlobalToggled,
        LatestChanged, ManualTestRun, NowPlaying, PlatformConnectionChanged, PlaybackState,
        QuickActionDone, RequestFailed, RunOutcome, ScriptFailed, ScriptHttpCall, ScriptHttpMethod,
        ScriptLogLevel, ScriptLogged, ScriptReloaded, ScriptStarted, SenderRole, SubActionDone,
        SubActionRun, TimerTick, WhisperSent,
    };

    fn decodes_back<T: TypedEvent + Clone + PartialEq + Debug>(value: T) {
        let cause = EventId::new();
        for (source, caused_by) in [
            (EventSource::Twitch, Some(cause)),
            (EventSource::Core, None),
        ] {
            let event = value.clone().into_event(source, caused_by);
            assert_eq!(
                (
                    event.kind.as_str(),
                    event.source,
                    event.caused_by,
                    T::from_event(&event)
                ),
                (T::KIND, source, caused_by, Some(value.clone())),
            );
        }
    }

    #[test]
    fn every_typed_event_decodes_back_from_the_event_it_builds() {
        decodes_back(ActionStarted {
            action_id: ActionId::new(),
            action_name: "Raid alert".to_owned(),
            sub_action_count: 3,
        });
        decodes_back(ActionDone {
            action_id: ActionId::new(),
            action_name: "Raid alert".to_owned(),
            outcome: RunOutcome::Failed,
            total_ms: 12,
            manual_test: true,
            reason: Some("boom".to_owned()),
            reason_text: Some(forge_types::VerbatimText::new("boom").into()),
        });
        decodes_back(ActionSkipped {
            action_id: ActionId::new(),
            reason: "queue held".to_owned(),
            queue_id: QueueId::new(),
        });
        decodes_back(QuickActionDone {
            kind: "obs.scene.switch".to_owned(),
            outcome: RunOutcome::Success,
            label: "Switch scene".to_owned(),
            builtin_id: "obs".to_owned(),
        });
        decodes_back(SubActionRun {
            step_index: 0,
            kind: "core.globals.set".to_owned(),
        });
        decodes_back(SubActionDone {
            step_index: 1,
            kind: "core.globals.set".to_owned(),
            outcome: RunOutcome::Skipped,
            duration_ms: 4,
            message: Some("common.integration_disabled".to_owned()),
            message_text: Some(forge_types::UserText::from(
                forge_types::integration_disabled_message(&forge_types::IntegrationId::new("obs")),
            )),
            integration: Some("obs".to_owned()),
        });
        decodes_back(GlobalSet {
            key: "counter".to_owned(),
            new_value: json!(42),
            prev_value: json!(null),
            persisted: Some(false),
            via: Some("ws_api".to_owned()),
        });
        decodes_back(GlobalIncremented {
            key: "counter".to_owned(),
            delta: 2,
            new_value: json!(44),
        });
        decodes_back(GlobalDecremented {
            key: "counter".to_owned(),
            delta: -2,
            new_value: json!(40),
        });
        decodes_back(GlobalToggled {
            key: "flag".to_owned(),
            new_value: true,
            prev_value: false,
        });
        decodes_back(GlobalArrayAppended {
            key: "list".to_owned(),
            new_length: 2,
            element: json!("b"),
        });
        decodes_back(GlobalArrayRemoved {
            key: "list".to_owned(),
            new_length: 0,
            element: json!({ "nested": [1, 2] }),
        });
        decodes_back(GlobalDeleted {
            key: "gone".to_owned(),
        });
        decodes_back(ScriptStarted {
            script_id: Some(ScriptId::new()),
            script_name: Some("greeter".to_owned()),
            is_inline: false,
        });
        decodes_back(ScriptFailed {
            script_id: None,
            script_name: None,
            is_inline: true,
            error_type: "runtime".to_owned(),
            message: "boom".to_owned(),
        });
        decodes_back(ScriptLogged {
            level: ScriptLogLevel::Warn,
            message: "line".to_owned(),
            script_id: None,
        });
        decodes_back(ScriptHttpCall {
            method: ScriptHttpMethod::Post,
            url_normalized: "https://example.com/path".to_owned(),
            status: 503,
            duration_ms: 70,
            truncated: true,
        });
        decodes_back(ScriptReloaded {
            script_id: ScriptId::new(),
            script_name: "greeter".to_owned(),
        });
        decodes_back(ChatSendRequest {
            message: "hello".to_owned(),
            reply_to_message_id: Some("m-1".to_owned()),
            whisper_to_login: Some("viewer".to_owned()),
            target: Some("twitch".to_owned()),
            sender: Some("bot".to_owned()),
        });
        decodes_back(ChatSent {
            channel: "twitch".to_owned(),
            message: "hello".to_owned(),
            reply_to_message_id: None,
            sent_as: Some(ActingSender {
                role: SenderRole::Bot,
                login: "forgebot".to_owned(),
            }),
        });
        decodes_back(ChatSendFailed {
            channel: "kick".to_owned(),
            error: "not connected".to_owned(),
        });
        decodes_back(WhisperSent {
            channel: "twitch".to_owned(),
            whisper_to_login: "viewer".to_owned(),
            sent_as: Some(ActingSender {
                role: SenderRole::Streamer,
                login: "caster".to_owned(),
            }),
        });
        decodes_back(CommandMatched {
            command: "!so".to_owned(),
            kind_id: "twitch.chat.command".to_owned(),
        });
        decodes_back(ManualTestRun {
            action_id: ActionId::new(),
        });
        decodes_back(RequestFailed {
            endpoint: "helix.users".to_owned(),
            status_code: 429,
            body_snippet: "slow down".to_owned(),
            retry_after_secs: None,
        });
        decodes_back(PlatformConnectionChanged {
            platform_id: "obs".to_owned(),
            state: ConnectionState::Reconnecting,
            expected: false,
        });
        decodes_back(TimerTick {
            instance_id: TriggerInstanceId::new(),
            interval_minutes: 15,
            message_count: 7,
        });
        decodes_back(LatestChanged {
            slot: "donation".to_owned(),
            platform: None,
            merged_changed: true,
            cleared: false,
        });
        decodes_back(NowPlaying {
            producer: IntegrationId::from_static("companion"),
            state: PlaybackState::Stopped,
            title: String::new(),
            artist: String::new(),
            album: None,
            art: None,
            occurred_at: OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
        });
    }

    #[test]
    fn chat_trigger_and_test_run_kinds_keep_their_wire_kind_and_payload_shape() {
        let action_id = ActionId::new();
        let cases = [
            (
                WhisperSent {
                    channel: "twitch".to_owned(),
                    whisper_to_login: "viewer".to_owned(),
                    sent_as: None,
                }
                .into_event(EventSource::Twitch, None),
                "chat.whisper.sent",
                json!({ "channel": "twitch", "whisper_to_login": "viewer" }),
            ),
            (
                WhisperSent {
                    channel: "twitch".to_owned(),
                    whisper_to_login: "viewer".to_owned(),
                    sent_as: Some(ActingSender {
                        role: SenderRole::Bot,
                        login: "forgebot".to_owned(),
                    }),
                }
                .into_event(EventSource::Twitch, None),
                "chat.whisper.sent",
                json!({
                    "channel": "twitch",
                    "whisper_to_login": "viewer",
                    "sent_as": { "role": "bot", "login": "forgebot" },
                }),
            ),
            (
                CommandMatched {
                    command: "!so".to_owned(),
                    kind_id: "twitch.chat.command".to_owned(),
                }
                .into_event(EventSource::Core, None),
                "command.matched",
                json!({ "command": "!so", "kind_id": "twitch.chat.command" }),
            ),
            (
                ManualTestRun { action_id }.into_event(EventSource::Core, None),
                "test.run",
                json!({ "action_id": action_id.to_string() }),
            ),
        ];

        for (event, kind, payload) in cases {
            assert_eq!((event.kind.as_str(), event.payload), (kind, payload));
        }
    }

    #[test]
    fn decoding_rejects_an_event_of_another_kind_even_when_the_payload_shape_matches() {
        let incremented = GlobalIncremented {
            key: "counter".to_owned(),
            delta: 1,
            new_value: json!(1),
        }
        .into_event(EventSource::Core, None);
        let appended = GlobalArrayAppended {
            key: "list".to_owned(),
            new_length: 1,
            element: json!(1),
        }
        .into_event(EventSource::Core, None);

        assert_eq!(
            (
                GlobalDecremented::from_event(&incremented),
                GlobalArrayRemoved::from_event(&appended),
            ),
            (None, None)
        );
    }

    #[test]
    fn decoding_a_same_kind_event_with_a_malformed_payload_yields_none() {
        for payload in [
            serde_json::Value::Null,
            json!({}),
            json!({ "endpoint": "helix.users", "status_code": "bad", "body_snippet": "", "retry_after_secs": null }),
            json!({ "endpoint": "helix.users", "status_code": 70_000, "body_snippet": "", "retry_after_secs": null }),
        ] {
            let event = Event::new(EventSource::Twitch, RequestFailed::KIND, payload.clone());
            assert_eq!(RequestFailed::from_event(&event), None, "payload {payload}");
        }
    }

    #[derive(Default)]
    struct RecordingPublisher(Mutex<Vec<Event>>);

    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn publish_hands_the_built_event_to_the_publisher() {
        let publisher = RecordingPublisher::default();
        let cause = EventId::new();
        let deleted = GlobalDeleted {
            key: "gone".to_owned(),
        };

        deleted
            .clone()
            .publish(&publisher, EventSource::Rhai, Some(cause));

        let published = publisher.0.into_inner().unwrap();
        let decoded: Vec<_> = published
            .iter()
            .map(|event| {
                (
                    event.source,
                    event.caused_by,
                    GlobalDeleted::from_event(event),
                )
            })
            .collect();
        assert_eq!(
            decoded,
            vec![(EventSource::Rhai, Some(cause), Some(deleted))]
        );
    }
}
