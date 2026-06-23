mod expression_state_changed;
mod face_found;
mod face_lost;
mod hotkey_triggered;
mod item_added;
mod item_removed;
mod model_config_changed;
mod model_loaded;
mod model_unloaded;

pub use expression_state_changed::ExpressionStateChangedDescriptor;
pub use face_found::FaceFoundDescriptor;
pub use face_lost::FaceLostDescriptor;
pub use hotkey_triggered::HotkeyTriggeredDescriptor;
pub use item_added::ItemAddedDescriptor;
pub use item_removed::ItemRemovedDescriptor;
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
    reg.register(Box::new(FaceFoundDescriptor))?;
    reg.register(Box::new(FaceLostDescriptor))?;
    reg.register(Box::new(ItemAddedDescriptor))?;
    reg.register(Box::new(ItemRemovedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn all_expected_trigger_ids_are_registered() {
        let mut reg = TriggerRegistry::new();
        register_vtube_triggers(&mut reg).expect("registration must succeed");
        for id in [
            "vtube.model.loaded",
            "vtube.model.unloaded",
            "vtube.model.config_changed",
            "vtube.hotkey.triggered",
            "vtube.expression.state_changed",
            "vtube.tracking.face_found",
            "vtube.tracking.face_lost",
            "vtube.item.added",
            "vtube.item.removed",
        ] {
            assert!(reg.get(id).is_some(), "missing trigger: {id}");
        }
    }
}
