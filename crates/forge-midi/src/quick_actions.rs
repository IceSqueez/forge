use std::collections::BTreeMap;

use forge_platform_core::{QuickAction, QuickActionAccent, QuickActions, SectionIcon};
use forge_types::{SubActionStep, Variant};

use crate::client::MidiClient;

impl QuickActions for MidiClient {
    fn actions(&self) -> Vec<QuickAction> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        let has_output = !snap.output_ports.is_empty();
        let first_port = snap
            .output_ports
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        drop(snap);

        let enabled = has_output;

        vec![
            QuickAction {
                label: "Send Note On".to_owned(),
                icon: SectionIcon::new("music"),
                enabled,
                locked_reason: None,
                group: None,
                group_icon: None,
                group_accent: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "midi.send".to_owned(),
                    config: BTreeMap::from([
                        ("port".to_owned(), Variant::String(first_port.clone())),
                        (
                            "message_kind".to_owned(),
                            Variant::String("note_on".to_owned()),
                        ),
                        ("note".to_owned(), Variant::Int(60)),
                        ("velocity".to_owned(), Variant::Int(127)),
                        ("channel".to_owned(), Variant::Int(0)),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Send Note Off".to_owned(),
                icon: SectionIcon::new("music-off"),
                enabled,
                locked_reason: None,
                group: None,
                group_icon: None,
                group_accent: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "midi.send".to_owned(),
                    config: BTreeMap::from([
                        ("port".to_owned(), Variant::String(first_port.clone())),
                        (
                            "message_kind".to_owned(),
                            Variant::String("note_off".to_owned()),
                        ),
                        ("note".to_owned(), Variant::Int(60)),
                        ("velocity".to_owned(), Variant::Int(0)),
                        ("channel".to_owned(), Variant::Int(0)),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Send CC".to_owned(),
                icon: SectionIcon::new("sliders"),
                enabled,
                locked_reason: None,
                group: None,
                group_icon: None,
                group_accent: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "midi.send".to_owned(),
                    config: BTreeMap::from([
                        ("port".to_owned(), Variant::String(first_port.clone())),
                        ("message_kind".to_owned(), Variant::String("cc".to_owned())),
                        ("controller".to_owned(), Variant::Int(7)),
                        ("value".to_owned(), Variant::Int(127)),
                        ("channel".to_owned(), Variant::Int(0)),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Send Raw".to_owned(),
                icon: SectionIcon::new("code"),
                enabled,
                locked_reason: None,
                group: None,
                group_icon: None,
                group_accent: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "midi.send".to_owned(),
                    config: BTreeMap::from([
                        ("port".to_owned(), Variant::String(first_port)),
                        ("message_kind".to_owned(), Variant::String("raw".to_owned())),
                        (
                            "raw_bytes".to_owned(),
                            Variant::Array(vec![
                                Variant::Int(0x90),
                                Variant::Int(60),
                                Variant::Int(127),
                            ]),
                        ),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
