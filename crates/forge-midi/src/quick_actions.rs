use forge_platform_core::{
    QuickAction, QuickActionAccent, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::{ConfigChoice, SubActionConfig};
use forge_types::{MessageCode, SubActionStep, Variant};

use crate::client::MidiClient;
use crate::decode::NOTE_ON_STATUS;
use crate::runners::{NOTE, VELOCITY};
use crate::vocabulary::config_keys::send;
use crate::vocabulary::limits::DATA_BYTE_MAX;
use crate::vocabulary::sub_action_ids;
use crate::vocabulary::{OutMessageKind, labels};

const CHANNEL_VOLUME_CONTROLLER: i64 = 7;
const RELEASE_VELOCITY: i64 = 0;

fn raw_note_on() -> Variant {
    Variant::Array(vec![
        Variant::Int(i64::from(NOTE_ON_STATUS)),
        Variant::Int(NOTE.default),
        Variant::Int(VELOCITY.default),
    ])
}

fn send_action(
    label: MessageCode,
    icon: &str,
    enabled: bool,
    config: SubActionConfig,
) -> QuickAction {
    QuickAction {
        label: label.into(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: None,
        destructive: false,
        accent: QuickActionAccent::Brand,
        subaction_template: SubActionStep {
            kind_id: sub_action_ids::SEND.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields: Vec::new(),
        collection: None,
    }
}

impl QuickActions for MidiClient {
    fn actions(&self) -> Vec<QuickAction> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        let first_port = snap.output_ports.first().map(|p| p.name.clone());
        drop(snap);

        let enabled = first_port.is_some();
        let preset = |kind: OutMessageKind, overrides: Vec<(&str, Variant)>| -> SubActionConfig {
            let port = first_port
                .clone()
                .map(|name| (send::PORT, Variant::String(name)));
            let kind = (
                send::MESSAGE_KIND,
                Variant::String(kind.wire_name().to_owned()),
            );
            port.into_iter()
                .chain([kind])
                .chain(overrides)
                .map(|(key, value)| (key.to_owned(), value))
                .collect()
        };

        vec![
            send_action(
                labels::SEND_NOTE_ON,
                "music",
                enabled,
                preset(OutMessageKind::NoteOn, Vec::new()),
            ),
            send_action(
                labels::SEND_NOTE_OFF,
                "music-off",
                enabled,
                preset(
                    OutMessageKind::NoteOff,
                    vec![(send::VELOCITY, Variant::Int(RELEASE_VELOCITY))],
                ),
            ),
            send_action(
                labels::SEND_CC,
                "sliders",
                enabled,
                preset(
                    OutMessageKind::ControlChange,
                    vec![
                        (send::CONTROLLER, Variant::Int(CHANNEL_VOLUME_CONTROLLER)),
                        (send::VALUE, Variant::Int(DATA_BYTE_MAX)),
                    ],
                ),
            ),
            send_action(
                labels::SEND_RAW,
                "code",
                enabled,
                preset(OutMessageKind::Raw, vec![(send::RAW_BYTES, raw_note_on())]),
            ),
        ]
    }
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::QuickActions;
    use forge_registry::{RunContext, SubActionRunner};
    use forge_types::{ArgStack, EventId, SubActionOutcome};

    use super::*;
    use crate::client::MidiClient;
    use crate::content::record_port_added;
    use crate::error::MidiError;
    use crate::events::{MidiOutMessage, MidiPortInfo, PortDirection};
    use crate::runners::MidiSendRunner;
    use crate::sink::MidiSink;
    use forge_types::UserText;

    const FULL_STORED_TEMPLATES: [(MessageCode, &str); 4] = [
        (
            labels::SEND_NOTE_ON,
            r#"{"channel":{"type":"int","value":0},"message_kind":{"type":"string","value":"note_on"},"note":{"type":"int","value":60},"port":{"type":"string","value":"Synth"},"velocity":{"type":"int","value":127}}"#,
        ),
        (
            labels::SEND_NOTE_OFF,
            r#"{"channel":{"type":"int","value":0},"message_kind":{"type":"string","value":"note_off"},"note":{"type":"int","value":60},"port":{"type":"string","value":"Synth"},"velocity":{"type":"int","value":0}}"#,
        ),
        (
            labels::SEND_CC,
            r#"{"channel":{"type":"int","value":0},"controller":{"type":"int","value":7},"message_kind":{"type":"string","value":"cc"},"port":{"type":"string","value":"Synth"},"value":{"type":"int","value":127}}"#,
        ),
        (
            labels::SEND_RAW,
            r#"{"message_kind":{"type":"string","value":"raw"},"port":{"type":"string","value":"Synth"},"raw_bytes":{"type":"array","value":[{"type":"int","value":144},{"type":"int","value":60},{"type":"int","value":127}]}}"#,
        ),
    ];

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    #[derive(Default)]
    struct RecordingSink {
        sent: Mutex<Vec<(String, MidiOutMessage)>>,
    }

    #[async_trait]
    impl MidiSink for RecordingSink {
        async fn send_output(
            &self,
            port_name: &str,
            message: &MidiOutMessage,
        ) -> Result<(), MidiError> {
            self.sent
                .lock()
                .unwrap()
                .push((port_name.to_owned(), message.clone()));
            Ok(())
        }
    }

    async fn sent_by(config: &SubActionConfig) -> Vec<(String, MidiOutMessage)> {
        let sink = Arc::new(RecordingSink::default());
        let runner = MidiSendRunner::new(Arc::clone(&sink) as Arc<dyn MidiSink>);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);
        let (telemetry, _) = runner.execute(config, &ctx).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        sink.sent.lock().unwrap().clone()
    }

    fn client_with_output_port() -> Arc<MidiClient> {
        let c = MidiClient::new_for_test();
        {
            let mut snap = c.content_state.lock().unwrap();
            record_port_added(
                &mut snap,
                MidiPortInfo {
                    name: "Synth".to_owned(),
                    direction: PortDirection::Output,
                },
            );
        }
        c
    }

    #[test]
    fn all_actions_disabled_when_no_output_ports() {
        let c = MidiClient::new_for_test();
        for action in c.actions() {
            assert!(
                !action.enabled,
                "{} must be disabled with no output ports",
                action.label.code_or_text()
            );
        }
    }

    #[test]
    fn actions_enabled_after_output_port_registered() {
        for action in client_with_output_port().actions() {
            assert!(
                action.enabled,
                "{} must be enabled",
                action.label.code_or_text()
            );
        }
    }

    #[tokio::test]
    async fn every_action_sends_what_its_full_stored_template_sent() {
        let actions = client_with_output_port().actions();
        let shown: Vec<UserText> = actions.iter().map(|a| a.label.clone()).collect();
        assert_eq!(
            shown,
            FULL_STORED_TEMPLATES
                .iter()
                .map(|(label, _)| UserText::from(label.clone()))
                .collect::<Vec<_>>()
        );

        for (action, (label, stored)) in actions.iter().zip(FULL_STORED_TEMPLATES) {
            let stored: SubActionConfig = serde_json::from_str(stored).unwrap();
            let template = &action.subaction_template;

            assert_eq!(template.kind_id, "midi.send", "{label}");
            assert_eq!(
                sent_by(&template.config).await,
                sent_by(&stored).await,
                "{label}"
            );
        }
    }

    #[test]
    fn every_action_template_validates_once_a_port_exists() {
        let runner = MidiSendRunner::new(Arc::new(RecordingSink::default()));
        let problems: BTreeMap<String, String> = client_with_output_port()
            .actions()
            .into_iter()
            .filter_map(|action| {
                let error = runner
                    .validate_config(&action.subaction_template.config)
                    .err()?;
                Some((action.label.code_or_text(), error.to_string()))
            })
            .collect();

        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn every_code_shown_with_and_without_ports_is_catalogued_in_both_languages() {
        use crate::emitted_codes::{emitted_codes, uncatalogued};
        use crate::vocabulary::messages::MIDI_MESSAGES;
        use forge_platform_core::BuiltinContent;

        for client in [MidiClient::new_for_test(), client_with_output_port()] {
            let emitted = emitted_codes(&serde_json::json!((client.actions(), client.sections())));

            assert!(!emitted.is_empty());
            assert_eq!(
                uncatalogued(&emitted, &[MIDI_MESSAGES, forge_types::COMMON_MESSAGES]),
                Vec::<String>::new()
            );
        }
    }
}
