mod script_event_custom;

pub use script_event_custom::ScriptEventCustomDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_core_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(ScriptEventCustomDescriptor))?;
    Ok(())
}
