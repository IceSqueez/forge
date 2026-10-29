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
    use forge_registry::FormField;
    use forge_types::{TriggerConfig, Variant};

    use super::*;

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
                display.ends_with(", device=Launchkey"),
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
                !display.contains("device="),
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

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_midi_triggers(&mut reg).unwrap();
        let result = register_midi_triggers(&mut reg);
        assert!(result.is_err());
    }
}
