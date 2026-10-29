use forge_platform_core::{
    QuickAction, QuickActionAccent, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_registry::{ConfigChoice, SubActionConfig};
use forge_types::{SubActionStep, Variant};

use crate::client::MidiClient;
use crate::decode::NOTE_ON_STATUS;
use crate::runners::{NOTE, VELOCITY};
use crate::vocabulary::OutMessageKind;
use crate::vocabulary::config_keys::send;
use crate::vocabulary::limits::DATA_BYTE_MAX;
use crate::vocabulary::sub_action_ids;

const CHANNEL_VOLUME_CONTROLLER: i64 = 7;
const RELEASE_VELOCITY: i64 = 0;

fn raw_note_on() -> Variant {
    Variant::Array(vec![
        Variant::Int(i64::from(NOTE_ON_STATUS)),
        Variant::Int(NOTE.default),
        Variant::Int(VELOCITY.default),
    ])
}

fn send_action(label: &str, icon: &str, enabled: bool, config: SubActionConfig) -> QuickAction {
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: None,
        group_icon: None,
        group_accent: None,
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
                "Send Note On",
                "music",
                enabled,
                preset(OutMessageKind::NoteOn, Vec::new()),
            ),
            send_action(
                "Send Note Off",
                "music-off",
                enabled,
                preset(
                    OutMessageKind::NoteOff,
                    vec![(send::VELOCITY, Variant::Int(RELEASE_VELOCITY))],
                ),
            ),
            send_action(
                "Send CC",
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
                "Send Raw",
                "code",
                enabled,
                preset(OutMessageKind::Raw, vec![(send::RAW_BYTES, raw_note_on())]),
            ),
        ]
    }
}
#[cfg(test)]
mod tests {
    use forge_platform_core::QuickActions;

    use super::*;
    use crate::client::MidiClient;
    use crate::content::record_port_added;
    use crate::events::{MidiPortInfo, PortDirection};

    #[test]
    fn all_actions_disabled_when_no_output_ports() {
        let c = MidiClient::new_for_test();
        for action in c.actions() {
            assert!(
                !action.enabled,
                "{} must be disabled with no output ports",
                action.label
            );
        }
    }

    #[test]
    fn actions_enabled_after_output_port_registered() {
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
        for action in c.actions() {
            assert!(action.enabled, "{} must be enabled", action.label);
        }
    }

    #[test]
    fn send_note_on_action_has_correct_kind_id() {
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
        let actions = c.actions();
        let note_on = actions.iter().find(|a| a.label == "Send Note On").unwrap();
        assert_eq!(note_on.subaction_template.kind_id, "midi.send");
        assert_eq!(
            note_on.subaction_template.config.get("message_kind"),
            Some(&Variant::String("note_on".to_owned()))
        );
    }
}
