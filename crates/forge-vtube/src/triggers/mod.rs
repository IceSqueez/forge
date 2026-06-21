mod expression_state_changed;
mod hotkey_triggered;
mod model_config_changed;
mod model_loaded;
mod model_unloaded;

pub use expression_state_changed::ExpressionStateChangedDescriptor;
pub use hotkey_triggered::HotkeyTriggeredDescriptor;
pub use model_config_changed::ModelConfigChangedDescriptor;
pub use model_loaded::ModelLoadedDescriptor;
pub use model_unloaded::ModelUnloadedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_vtube_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(ModelLoadedDescriptor))?;
    reg.register(Box::new(ModelUnloadedDescriptor))?;
    reg.register(Box::new(ModelConfigChangedDescriptor))?;
    reg.register(Box::new(HotkeyTriggeredDescriptor))?;
    reg.register(Box::new(ExpressionStateChangedDescriptor))?;
    Ok(())
}
