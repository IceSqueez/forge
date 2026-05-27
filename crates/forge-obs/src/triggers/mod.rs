mod scene_current_changed;

pub use scene_current_changed::SceneCurrentChangedDescriptor;

use forge_registry::{RegistryError, TriggerRegistry};

pub fn register_obs_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    reg.register(Box::new(SceneCurrentChangedDescriptor))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn register_obs_triggers_registers_one_descriptor() {
        let mut reg = TriggerRegistry::new();
        register_obs_triggers(&mut reg).unwrap();
        assert_eq!(reg.all().count(), 1);
    }

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
