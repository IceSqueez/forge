mod expression_set;
mod hotkey_trigger;
mod item_move;
mod model_load;
mod model_move;
mod param_set;
mod params_reset;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use expression_set::ExpressionSetRunner;
pub use hotkey_trigger::HotkeyTriggerRunner;
pub use item_move::ItemMoveRunner;
pub use model_load::ModelLoadRunner;
pub use model_move::ModelMoveRunner;
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
    reg.register(Box::new(ItemMoveRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::MockSink;

    #[test]
    fn all_expected_runner_ids_are_present() {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new())).unwrap();
        for id in &[
            "vtube.hotkey.trigger",
            "vtube.expression.set",
            "vtube.param.set",
            "vtube.model.load",
            "vtube.params.reset",
            "vtube.model.move",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
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
