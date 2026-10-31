mod expression_state_changed;
mod face_found;
mod face_lost;
mod hotkey_triggered;
mod item_added;
mod item_removed;
mod model_config_changed;
mod model_loaded;
mod model_unloaded;

pub(crate) use expression_state_changed::ExpressionStateChangedDescriptor;
pub(crate) use face_found::FaceFoundDescriptor;
pub(crate) use face_lost::FaceLostDescriptor;
pub(crate) use hotkey_triggered::HotkeyTriggeredDescriptor;
pub(crate) use item_added::ItemAddedDescriptor;
pub(crate) use item_removed::ItemRemovedDescriptor;
pub(crate) use model_config_changed::ModelConfigChangedDescriptor;
pub(crate) use model_loaded::ModelLoadedDescriptor;
pub(crate) use model_unloaded::ModelUnloadedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_vtube_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::VTUBE_INTEGRATION.id);
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
