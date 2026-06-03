use async_trait::async_trait;

use crate::error::VTubeError;

#[async_trait]
pub trait VTubeSink: Send + Sync {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError>;

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError>;

    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError>;

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError>;

    async fn reset_params(&self) -> Result<(), VTubeError>;

    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        time_in_seconds: f64,
    ) -> Result<(), VTubeError>;
}
