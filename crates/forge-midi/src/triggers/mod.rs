mod cc;
mod device_connected;
mod device_disconnected;
mod note_off;
mod note_on;
mod pitch_bend;
mod program_change;

pub use cc::MidiCcDescriptor;
pub use device_connected::MidiDeviceConnectedDescriptor;
pub use device_disconnected::MidiDeviceDisconnectedDescriptor;
pub use note_off::MidiNoteOffDescriptor;
pub use note_on::MidiNoteOnDescriptor;
pub use pitch_bend::MidiPitchBendDescriptor;
pub use program_change::MidiProgramChangeDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_midi_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(MidiNoteOnDescriptor))?;
    reg.register(Box::new(MidiNoteOffDescriptor))?;
    reg.register(Box::new(MidiCcDescriptor))?;
    reg.register(Box::new(MidiPitchBendDescriptor))?;
    reg.register(Box::new(MidiProgramChangeDescriptor))?;
    reg.register(Box::new(MidiDeviceConnectedDescriptor))?;
    reg.register(Box::new(MidiDeviceDisconnectedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
