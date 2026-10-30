mod cc;
mod config_read;
mod device_connected;
mod device_disconnected;
mod input;
mod note_fields;
mod note_off;
mod note_on;
mod pitch_bend;
mod port;
mod program_change;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_midi_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::MIDI_INTEGRATION.id);
    reg.register(Box::new(note_on::NOTE_ON_TRIGGER))?;
    reg.register(Box::new(note_off::NOTE_OFF_TRIGGER))?;
    reg.register(Box::new(cc::CONTROL_CHANGE_TRIGGER))?;
    reg.register(Box::new(pitch_bend::PITCH_BEND_TRIGGER))?;
    reg.register(Box::new(program_change::PROGRAM_CHANGE_TRIGGER))?;
    reg.register(Box::new(device_connected::DEVICE_CONNECTED_TRIGGER))?;
    reg.register(Box::new(device_disconnected::DEVICE_DISCONNECTED_TRIGGER))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_events::{Event, EventSource};
    use forge_registry::{FormField, joined_condition};
    use forge_types::{MessageArg, MessageCode, MessageRef, TriggerConfig, UserText, Variant};

    use super::*;
    use crate::vocabulary::messages;

    const INPUT_TRIGGER_IDS: [&str; 5] = [
        "midi.input.note_on",
        "midi.input.note_off",
        "midi.input.control_change",
        "midi.input.pitch_bend",
        "midi.input.program_change",
    ];

    fn registry() -> TriggerRegistry {
        let mut reg = TriggerRegistry::new();
        register_midi_triggers(&mut reg).unwrap();
        reg
    }

    fn message_mentions(message: &MessageRef, part: &MessageRef) -> bool {
        message == part
            || message.args().any(|(_, value)| match value {
                MessageArg::Message(nested) => message_mentions(nested, part),
                _ => false,
            })
    }

    fn mentions(text: &UserText, part: &MessageRef) -> bool {
        matches!(text, UserText::Message(message) if message_mentions(message, part))
    }

    fn input_event(kind: &str, port_name: &str) -> Event {
        Event::new(
            EventSource::Midi,
            kind,
            serde_json::json!({
                "note": 60,
                "velocity": 100,
                "controller": 7,
                "value": 64,
                "program": 10,
                "channel": 0,
                "port_name": port_name,
            }),
        )
    }

    fn device_config(name: &str) -> TriggerConfig {
        BTreeMap::from([("device".to_owned(), Variant::String(name.to_owned()))])
    }

    #[test]
    fn every_input_descriptor_offers_an_optional_device_field() {
        let reg = registry();
        for id in INPUT_TRIGGER_IDS {
            let fields = reg.get(id).unwrap().config_fields();
            assert!(
                fields
                    .iter()
                    .any(|f| matches!(f, FormField::Optional { key: "device", .. })),
                "{id} exposes no optional device field"
            );
        }
    }

    #[test]
    fn every_input_descriptor_matches_only_events_from_the_configured_device() {
        let reg = registry();
        let config = device_config("Launchkey");
        for id in INPUT_TRIGGER_IDS {
            let d = reg.get(id).unwrap();
            assert!(
                d.matches_trigger(&config, &input_event(id, "Launchkey")),
                "{id} rejected an event from its configured device"
            );
            assert!(
                !d.matches_trigger(&config, &input_event(id, "Keystation")),
                "{id} ignored the configured device filter"
            );
        }
    }

    #[test]
    fn every_input_descriptor_appends_the_device_to_its_condition_display() {
        let reg = registry();
        let config = device_config("Launchkey");
        for id in INPUT_TRIGGER_IDS {
            let display = reg.get(id).unwrap().condition_display(&config);
            assert!(
                mentions(&display, &messages::condition_device("Launchkey")),
                "{id} rendered {display:?}"
            );
        }
    }

    #[test]
    fn every_input_descriptor_treats_an_empty_device_string_as_any_device() {
        let reg = registry();
        let config = device_config("");
        for id in INPUT_TRIGGER_IDS {
            let d = reg.get(id).unwrap();
            for port in ["Launchkey", "Keystation"] {
                assert!(
                    d.matches_trigger(&config, &input_event(id, port)),
                    "{id} filtered out {port} on an empty device string"
                );
            }
        }
    }

    #[test]
    fn every_input_descriptor_omits_an_empty_device_from_its_condition_display() {
        let reg = registry();
        let config = device_config("");
        for id in INPUT_TRIGGER_IDS {
            let display = reg.get(id).unwrap().condition_display(&config);
            assert!(
                !mentions(&display, &messages::condition_device("")),
                "{id} rendered an empty device filter as {display:?}"
            );
        }
    }

    #[test]
    fn all_trigger_ids_are_present() {
        let mut reg = TriggerRegistry::new();
        register_midi_triggers(&mut reg).unwrap();
        for id in &[
            "midi.input.note_on",
            "midi.input.note_off",
            "midi.input.control_change",
            "midi.input.pitch_bend",
            "midi.input.program_change",
            "midi.device.connected",
            "midi.device.disconnected",
        ] {
            assert!(reg.get(id).is_some(), "missing trigger: {id}");
        }
    }

    struct InputCase {
        id: &'static str,
        number_key: Option<&'static str>,
        number: i64,
        any_number: Option<MessageCode>,
        stack: &'static [(&'static str, i64)],
    }

    const INPUT_CASES: [InputCase; 5] = [
        InputCase {
            id: "midi.input.note_on",
            number_key: Some("note"),
            number: 60,
            any_number: Some(messages::CONDITION_ANY_NOTE),
            stack: &[
                ("midi.note", 60),
                ("midi.velocity", 100),
                ("midi.channel", 1),
            ],
        },
        InputCase {
            id: "midi.input.note_off",
            number_key: Some("note"),
            number: 60,
            any_number: Some(messages::CONDITION_ANY_NOTE),
            stack: &[
                ("midi.note", 60),
                ("midi.velocity", 100),
                ("midi.channel", 1),
            ],
        },
        InputCase {
            id: "midi.input.control_change",
            number_key: Some("controller"),
            number: 7,
            any_number: Some(messages::CONDITION_ANY_CONTROLLER),
            stack: &[
                ("midi.controller", 7),
                ("midi.value", 64),
                ("midi.channel", 1),
            ],
        },
        InputCase {
            id: "midi.input.pitch_bend",
            number_key: None,
            number: 0,
            any_number: None,
            stack: &[("midi.value", 64), ("midi.channel", 1)],
        },
        InputCase {
            id: "midi.input.program_change",
            number_key: Some("program"),
            number: 10,
            any_number: Some(messages::CONDITION_ANY_PROGRAM),
            stack: &[("midi.program", 10), ("midi.channel", 1)],
        },
    ];

    fn channel_event(kind: &str, channel: i64) -> Event {
        Event::new(
            EventSource::Midi,
            kind,
            serde_json::json!({
                "note": 60,
                "velocity": 100,
                "controller": 7,
                "value": 64,
                "program": 10,
                "channel": channel,
                "port_name": "Pad",
            }),
        )
    }

    fn int_config(key: &str, value: i64) -> TriggerConfig {
        BTreeMap::from([(key.to_owned(), Variant::Int(value))])
    }

    #[test]
    fn every_input_kind_matches_an_unfiltered_event_of_its_own_kind_only() {
        let reg = registry();
        for case in &INPUT_CASES {
            let d = reg.get(case.id).unwrap();
            for other in &INPUT_CASES {
                assert_eq!(
                    d.matches_trigger(&BTreeMap::new(), &input_event(other.id, "Pad")),
                    case.id == other.id,
                    "{} against a {} event",
                    case.id,
                    other.id
                );
            }
        }
    }

    #[test]
    fn every_input_kind_rejects_port_events_of_the_same_device() {
        let reg = registry();
        for case in &INPUT_CASES {
            let d = reg.get(case.id).unwrap();
            for port_kind in ["midi.port.added", "midi.port.removed"] {
                assert!(!d.matches_trigger(&BTreeMap::new(), &input_event(port_kind, "Pad")));
            }
        }
    }

    #[test]
    fn numbered_kinds_filter_on_exact_number_and_pitch_bend_ignores_it() {
        let reg = registry();
        for case in &INPUT_CASES {
            let d = reg.get(case.id).unwrap();
            let event = input_event(case.id, "Pad");
            let Some(key) = case.number_key else {
                assert!(
                    d.matches_trigger(&int_config("note", 1), &event),
                    "{}",
                    case.id
                );
                continue;
            };
            assert!(
                d.matches_trigger(&int_config(key, case.number), &event),
                "{}",
                case.id
            );
            assert!(
                !d.matches_trigger(&int_config(key, case.number + 1), &event),
                "{}",
                case.id
            );
        }
    }

    #[test]
    fn numbered_kinds_never_match_a_negative_or_out_of_range_number() {
        let reg = registry();
        for case in INPUT_CASES.iter().filter(|c| c.number_key.is_some()) {
            let key = case.number_key.unwrap();
            let d = reg.get(case.id).unwrap();
            let event = input_event(case.id, "Pad");
            for bad in [
                -1,
                case.number - 256,
                case.number + 256,
                case.number + (1 << 32),
                i64::MIN,
                i64::MAX,
            ] {
                assert!(
                    !d.matches_trigger(&int_config(key, bad), &event),
                    "{} matched number {bad}",
                    case.id
                );
            }
        }
    }

    #[test]
    fn channel_filter_matches_exactly_and_never_matches_out_of_range_values() {
        let reg = registry();
        for case in &INPUT_CASES {
            let d = reg.get(case.id).unwrap();
            let on_channel_zero = channel_event(case.id, 0);
            assert!(d.matches_trigger(&int_config("channel", 0), &on_channel_zero));
            assert!(!d.matches_trigger(&int_config("channel", 1), &on_channel_zero));
            for bad in [-1, 16, 256, 1 << 32, i64::MIN, i64::MAX] {
                assert!(
                    !d.matches_trigger(&int_config("channel", bad), &on_channel_zero),
                    "{} matched channel {bad}",
                    case.id
                );
            }
            let on_channel_fifteen = channel_event(case.id, 15);
            assert!(d.matches_trigger(&int_config("channel", 15), &on_channel_fifteen));
            assert!(!d.matches_trigger(&int_config("channel", 16), &on_channel_fifteen));
        }
    }

    #[test]
    fn numeric_filters_never_match_an_event_missing_the_field() {
        let reg = registry();
        for case in INPUT_CASES.iter().filter(|c| c.number_key.is_some()) {
            let key = case.number_key.unwrap();
            let bare = Event::new(
                EventSource::Midi,
                case.id,
                serde_json::json!({ "channel": 0, "port_name": "Pad" }),
            );
            let d = reg.get(case.id).unwrap();
            assert!(
                !d.matches_trigger(&int_config(key, case.number), &bare),
                "{}",
                case.id
            );
            assert!(!d.matches_trigger(
                &int_config("channel", 0),
                &Event::new(
                    EventSource::Midi,
                    case.id,
                    serde_json::json!({ "port_name": "Pad" }),
                )
            ));
        }
    }

    #[test]
    fn every_input_kind_exposes_exactly_its_own_variables_on_the_arg_stack() {
        let reg = registry();
        for case in &INPUT_CASES {
            let stack = reg
                .get(case.id)
                .unwrap()
                .build_arg_stack(&channel_event(case.id, 1));
            for (name, value) in case.stack {
                assert_eq!(
                    stack.get(name),
                    Some(&Variant::Int(*value)),
                    "{} {name}",
                    case.id
                );
            }
            assert_eq!(
                stack.get("midi.port"),
                Some(&Variant::String("Pad".to_owned())),
                "{}",
                case.id
            );
            for foreign in [
                "midi.note",
                "midi.velocity",
                "midi.controller",
                "midi.program",
                "midi.value",
            ] {
                let expected = case.stack.iter().any(|(name, _)| *name == foreign);
                assert_eq!(
                    stack.get(foreign).is_some(),
                    expected,
                    "{} {foreign}",
                    case.id
                );
            }
        }
    }

    #[test]
    fn every_input_kind_condition_display_names_its_number_and_channel() {
        let reg = registry();
        for case in &INPUT_CASES {
            let d = reg.get(case.id).unwrap();
            let unfiltered = case
                .any_number
                .clone()
                .map(MessageRef::new)
                .into_iter()
                .chain([messages::CONDITION_ANY_CHANNEL.into()]);
            assert_eq!(
                d.condition_display(&BTreeMap::new()),
                joined_condition(unfiltered),
                "{}",
                case.id
            );
            let with_channel = d.condition_display(&int_config("channel", 2));
            assert!(
                mentions(&with_channel, &messages::condition_channel(2)),
                "{}",
                case.id
            );
        }
    }

    #[test]
    fn port_triggers_match_only_their_own_port_event() {
        let reg = registry();
        let connected = reg.get("midi.device.connected").unwrap();
        let disconnected = reg.get("midi.device.disconnected").unwrap();
        let port = |kind: &str| {
            Event::new(
                EventSource::Midi,
                kind,
                serde_json::json!({ "port_name": "Pad", "direction": "input" }),
            )
        };
        assert!(connected.matches_trigger(&BTreeMap::new(), &port("midi.port.added")));
        assert!(!connected.matches_trigger(&BTreeMap::new(), &port("midi.port.removed")));
        assert!(disconnected.matches_trigger(&BTreeMap::new(), &port("midi.port.removed")));
        assert!(!disconnected.matches_trigger(&BTreeMap::new(), &port("midi.port.added")));
        assert!(!connected.matches_trigger(&BTreeMap::new(), &port("midi.input.note_on")));
    }

    #[test]
    fn port_triggers_filter_on_direction_and_treat_empty_as_any() {
        let reg = registry();
        for (id, kind) in [
            ("midi.device.connected", "midi.port.added"),
            ("midi.device.disconnected", "midi.port.removed"),
        ] {
            let d = reg.get(id).unwrap();
            let output = Event::new(
                EventSource::Midi,
                kind,
                serde_json::json!({ "port_name": "Synth", "direction": "output" }),
            );
            let direction = |text: &str| {
                BTreeMap::from([("direction".to_owned(), Variant::String(text.to_owned()))])
            };
            assert!(d.matches_trigger(&direction("output"), &output), "{id}");
            assert!(!d.matches_trigger(&direction("input"), &output), "{id}");
            assert!(d.matches_trigger(&direction(""), &output), "{id}");
        }
    }

    #[test]
    fn port_triggers_expose_name_and_direction_and_omit_what_the_event_lacks() {
        let reg = registry();
        for (id, kind) in [
            ("midi.device.connected", "midi.port.added"),
            ("midi.device.disconnected", "midi.port.removed"),
        ] {
            let d = reg.get(id).unwrap();
            let full = d.build_arg_stack(&Event::new(
                EventSource::Midi,
                kind,
                serde_json::json!({ "port_name": "Pad", "direction": "input" }),
            ));
            assert_eq!(
                full.get("midi.device.name"),
                Some(&Variant::String("Pad".to_owned())),
                "{id}"
            );
            assert_eq!(
                full.get("midi.device.direction"),
                Some(&Variant::String("input".to_owned())),
                "{id}"
            );
            let nameless = d.build_arg_stack(&Event::new(
                EventSource::Midi,
                kind,
                serde_json::json!({ "direction": "input" }),
            ));
            assert_eq!(nameless.get("midi.device.name"), None, "{id}");
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_midi_triggers(&mut reg).unwrap();
        let result = register_midi_triggers(&mut reg);
        assert!(result.is_err());
    }
}
