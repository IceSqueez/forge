mod send;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use send::MidiSendRunner;

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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sink::tests::NoopSink;

    #[test]
    fn midi_send_runner_id_is_correct() {
        let mut reg = SubActionRegistry::new();
        let sink: Arc<dyn MidiSink> = Arc::new(NoopSink);
        reg.register(Box::new(MidiSendRunner::new(sink))).unwrap();
        assert!(reg.get("midi.send").is_some());
    }
}
