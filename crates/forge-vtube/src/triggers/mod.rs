mod expression_state_changed;
mod face_found;
mod face_lost;
mod families;
mod hotkey_triggered;
mod item_added;
mod item_removed;
mod model_config_changed;
mod model_loaded;
mod model_unloaded;

use forge_registry::EventTriggerSpec;
use forge_registry::{RegistryError, TriggerRegistry};

const TRIGGERS: &[&EventTriggerSpec] = &[
    &model_loaded::MODEL_LOADED,
    &model_unloaded::MODEL_UNLOADED,
    &model_config_changed::MODEL_CONFIG_CHANGED,
    &hotkey_triggered::HOTKEY_TRIGGERED,
    &expression_state_changed::EXPRESSION_STATE_CHANGED,
    &face_found::FACE_FOUND,
    &face_lost::FACE_LOST,
    &item_added::ITEM_ADDED,
    &item_removed::ITEM_REMOVED,
];

pub fn register_vtube_triggers(reg: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::VTUBE_INTEGRATION.id);
    for spec in TRIGGERS {
        reg.register(Box::new((*spec).clone()))?;
    }
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
