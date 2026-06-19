mod scene_collection_changed;
mod scene_collection_changing;
mod scene_current_changed;
mod scene_list_changed;
mod scene_preview_changed;
mod stream_started;
mod stream_starting;
mod stream_status_changed;
mod stream_stopped;
mod stream_stopping;

pub use scene_collection_changed::SceneCollectionChangedDescriptor;
pub use scene_collection_changing::SceneCollectionChangingDescriptor;
pub use scene_current_changed::SceneCurrentChangedDescriptor;
pub use scene_list_changed::SceneListChangedDescriptor;
pub use scene_preview_changed::ScenePreviewChangedDescriptor;
pub use stream_started::StreamStartedDescriptor;
pub use stream_starting::StreamStartingDescriptor;
pub use stream_status_changed::StreamStatusChangedDescriptor;
pub use stream_stopped::StreamStoppedDescriptor;
pub use stream_stopping::StreamStoppingDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_obs_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(SceneCurrentChangedDescriptor))?;
    reg.register(Box::new(ScenePreviewChangedDescriptor))?;
    reg.register(Box::new(SceneListChangedDescriptor))?;
    reg.register(Box::new(SceneCollectionChangingDescriptor))?;
    reg.register(Box::new(SceneCollectionChangedDescriptor))?;
    reg.register(Box::new(StreamStartingDescriptor))?;
    reg.register(Box::new(StreamStartedDescriptor))?;
    reg.register(Box::new(StreamStoppingDescriptor))?;
    reg.register(Box::new(StreamStoppedDescriptor))?;
    reg.register(Box::new(StreamStatusChangedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn all_expected_trigger_ids_are_registered() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        for id in [
            "obs.scenes.current_changed",
            "obs.scenes.preview_changed",
            "obs.scenes.list_changed",
            "obs.collection.changing",
            "obs.collection.current_changed",
        ] {
            assert!(reg.get(id).is_some(), "missing trigger: {id}");
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        let result = register_obs_triggers(&mut reg);
        assert!(result.is_err());
    }
}
