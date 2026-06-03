mod expression_set;
mod hotkey_trigger;
mod model_load;
mod model_move;
mod param_set;
mod params_reset;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use expression_set::ExpressionSetRunner;
pub use hotkey_trigger::HotkeyTriggerRunner;
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
    reg.register(Box::new(ModelMoveRunner::new(sink)))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use async_trait::async_trait;

    struct MockSink;

    #[async_trait]
    impl VTubeSink for MockSink {
        async fn trigger_hotkey(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_param(&self, _: &str, _: f64) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn load_model(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn reset_params(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn move_model(
            &self,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
    }

    #[test]
    fn register_vtube_sub_actions_registers_six_runners() {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink)).unwrap();
        assert_eq!(reg.all().count(), 6);
    }

    #[test]
    fn all_expected_runner_ids_are_present() {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink)).unwrap();
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
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink)).unwrap();
        let result = register_vtube_sub_actions(&mut reg, Arc::new(MockSink));
        assert!(result.is_err());
    }
}
