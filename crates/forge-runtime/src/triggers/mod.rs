mod script_event_custom;
mod timer_tick;

pub use script_event_custom::ScriptEventCustomDescriptor;
pub use timer_tick::{TIMER_TICK_KIND, TimerSchedule, TimerTickDescriptor};

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_core_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(ScriptEventCustomDescriptor))?;
    reg.register(Box::new(TimerTickDescriptor))?;
    Ok(())
}
