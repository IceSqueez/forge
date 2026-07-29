mod expression_set;
mod hotkey_trigger;
mod item_load;
mod item_move;
mod item_pin;
mod item_throw;
mod item_unload_all;
mod lookup_current_model;
mod lookup_expressions;
mod lookup_hotkeys;
mod lookup_items;
mod lookup_parameters;
mod model_load;
mod model_move;
mod model_set_physics;
mod model_tint;
mod param_set;
mod params_reset;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use expression_set::ExpressionSetRunner;
pub use hotkey_trigger::HotkeyTriggerRunner;
pub use item_load::ItemLoadRunner;
pub use item_move::ItemMoveRunner;
pub use item_pin::ItemPinRunner;
pub use item_throw::ItemThrowRunner;
pub use item_unload_all::ItemUnloadAllRunner;
pub use lookup_current_model::LookupCurrentModelRunner;
pub use lookup_expressions::LookupExpressionsRunner;
pub use lookup_hotkeys::LookupHotkeysRunner;
pub use lookup_items::LookupItemsRunner;
pub use lookup_parameters::LookupParametersRunner;
pub use model_load::ModelLoadRunner;
pub use model_move::ModelMoveRunner;
pub use model_set_physics::ModelSetPhysicsRunner;
pub use model_tint::ModelTintRunner;
pub use param_set::ParamSetRunner;
pub use params_reset::ParamsResetRunner;

use crate::sink::VTubeSink;

pub fn register_vtube_sub_actions(
    reg: &mut SubActionRegistry,
    sink: Arc<dyn VTubeSink>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(HotkeyTriggerRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ExpressionSetRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ParamSetRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ModelLoadRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ParamsResetRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ModelMoveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemMoveRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupCurrentModelRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupHotkeysRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupExpressionsRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupParametersRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(LookupItemsRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemPinRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemLoadRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemUnloadAllRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ItemThrowRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ModelTintRunner::new(Arc::clone(&sink))))?;
    reg.register(Box::new(ModelSetPhysicsRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_platform_core::QuickActions;
    use forge_registry::FormField;

    use super::*;
    use crate::client::VTubeClient;
    use crate::runners::test_support::MockSink;

    fn registry() -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new())).unwrap();
        reg
    }

    fn field_key(field: &FormField) -> &'static str {
        match field {
            FormField::Text { key, .. }
            | FormField::TextArea { key, .. }
            | FormField::Code { key, .. }
            | FormField::Integer { key, .. }
            | FormField::Toggle { key, .. }
            | FormField::FilePicker { key, .. }
            | FormField::DateTime { key, .. }
            | FormField::Select { key, .. }
            | FormField::DynamicSelect { key, .. }
            | FormField::Optional { key, .. }
            | FormField::SubChain { key, .. }
            | FormField::CaseList { key, .. }
            | FormField::Slider { key, .. }
            | FormField::Swatch { key, .. } => key,
        }
    }

    #[test]
    fn all_expected_runner_ids_are_present() {
        let reg = registry();
        for id in &[
            "vtube.hotkey.trigger",
            "vtube.expression.set",
            "vtube.param.set",
            "vtube.model.load",
            "vtube.params.reset",
            "vtube.model.move",
            "vtube.model.tint",
            "vtube.model.set_physics",
            "vtube.item.move",
            "vtube.item.pin",
            "vtube.item.load",
            "vtube.item.throw",
            "vtube.item.unload_all",
            "vtube.lookup.current_model",
            "vtube.lookup.hotkeys",
            "vtube.lookup.expressions",
            "vtube.lookup.parameters",
            "vtube.lookup.items",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
        }
    }

    // Why: a quick action naming an unregistered runner, or presetting a key the runner never
    // reads, silently does nothing when the user clicks it - no error, no log, no effect.
    #[test]
    fn every_quick_action_targets_a_registered_runner_that_reads_its_keys() {
        let reg = registry();

        for action in VTubeClient::new_for_test("ws://127.0.0.1:8001/").actions() {
            let kind_id = &action.subaction_template.kind_id;
            let runner = reg.get(kind_id).unwrap_or_else(|| {
                panic!(
                    "quick action '{}' targets unknown runner '{kind_id}'",
                    action.label
                )
            });

            let mut read: BTreeSet<String> = runner.default_config().into_keys().collect();
            read.extend(
                runner
                    .config_fields()
                    .iter()
                    .map(|f| field_key(f).to_owned()),
            );

            let written = action
                .subaction_template
                .config
                .keys()
                .cloned()
                .chain(action.fields.iter().map(|f| f.key.clone()));

            for key in written {
                assert!(
                    read.contains(&key),
                    "quick action '{}' sets '{key}', which runner '{kind_id}' never reads",
                    action.label,
                );
            }
        }
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new())).unwrap();
        let result = register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new()));
        assert!(result.is_err());
    }
}
