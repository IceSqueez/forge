mod byte_field;
mod send;
mod step_failure;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub(crate) use send::MidiSendRunner;
pub(crate) use send::{NOTE, VELOCITY};

use crate::sink::MidiSink;

pub fn register_midi_sub_actions(
    reg: &mut SubActionRegistry,
    sink: Arc<dyn MidiSink>,
) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::MIDI_INTEGRATION.id);
    reg.register(Box::new(MidiSendRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::tests::NoopSink;

    #[test]
    fn registration_adds_the_send_runner_owned_by_the_midi_integration() {
        let mut reg = SubActionRegistry::new();
        let sink: Arc<dyn MidiSink> = Arc::new(NoopSink);

        register_midi_sub_actions(&mut reg, sink).unwrap();

        assert!(reg.get("midi.send").is_some());
        assert_eq!(
            reg.owning_integration("midi.send"),
            Some(&crate::MIDI_INTEGRATION.id)
        );
    }
}
