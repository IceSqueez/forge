mod cc;
mod note_off;
mod note_on;

pub use cc::MidiCcDescriptor;
pub use note_off::MidiNoteOffDescriptor;
pub use note_on::MidiNoteOnDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_midi_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(MidiNoteOnDescriptor))?;
    reg.register(Box::new(MidiNoteOffDescriptor))?;
    reg.register(Box::new(MidiCcDescriptor))?;
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
