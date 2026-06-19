mod scene_collection_changed;
mod scene_collection_changing;
mod scene_current_changed;
mod scene_list_changed;
mod scene_preview_changed;

pub use scene_collection_changed::SceneCollectionChangedDescriptor;
pub use scene_collection_changing::SceneCollectionChangingDescriptor;
pub use scene_current_changed::SceneCurrentChangedDescriptor;
pub use scene_list_changed::SceneListChangedDescriptor;
pub use scene_preview_changed::ScenePreviewChangedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_obs_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(SceneCurrentChangedDescriptor))?;
    reg.register(Box::new(ScenePreviewChangedDescriptor))?;
    reg.register(Box::new(SceneListChangedDescriptor))?;
    reg.register(Box::new(SceneCollectionChangingDescriptor))?;
    reg.register(Box::new(SceneCollectionChangedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn registered_descriptor_id_is_correct() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        assert!(reg.get("obs.scenes.current_changed").is_some());
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        let result = register_obs_triggers(&mut reg);
        assert!(result.is_err());
    }
}
