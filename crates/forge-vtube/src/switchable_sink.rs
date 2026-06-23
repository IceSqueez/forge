// Lets runner registration happen at boot (before async client connect) while still
// forwarding calls to the real client once it arrives.
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use forge_types::Variant;

use crate::client::VTubeClient;
use crate::error::VTubeError;
use crate::sink::VTubeSink;

pub struct SwitchableVTubeSink {
    inner: RwLock<Option<Arc<VTubeClient>>>,
}

impl SwitchableVTubeSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(None),
        })
    }

    pub fn install(&self, client: Arc<VTubeClient>) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub fn clear(&self) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    // Clone the Arc out before any await so the sync RwLock guard is never held
    // across an async call.
    fn get(&self) -> Result<Arc<VTubeClient>, VTubeError> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard.clone().ok_or(VTubeError::NotConnected)
    }
}

impl Default for SwitchableVTubeSink {
    fn default() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }
}

#[async_trait]
impl VTubeSink for SwitchableVTubeSink {
    async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError> {
        let client = self.get()?;
        client.trigger_hotkey(hotkey_id).await
    }

    async fn set_expression(&self, expression_file: &str, active: bool) -> Result<(), VTubeError> {
        let client = self.get()?;
        client.set_expression(expression_file, active).await
    }

    async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError> {
        let client = self.get()?;
        client.set_param(param_id, value).await
    }

    async fn load_model(&self, model_id: &str) -> Result<(), VTubeError> {
        let client = self.get()?;
        client.load_model(model_id).await
    }

    async fn reset_params(&self) -> Result<(), VTubeError> {
        let client = self.get()?;
        client.reset_params().await
    }

    async fn move_model(
        &self,
        x: Option<f64>,
        y: Option<f64>,
        rotation: Option<f64>,
        time_in_seconds: f64,
    ) -> Result<(), VTubeError> {
        let client = self.get()?;
        client.move_model(x, y, rotation, time_in_seconds).await
    }

    async fn move_item(
        &self,
        item_instance_id: &str,
        x: Option<f64>,
        y: Option<f64>,
        size: Option<f64>,
        rotation: Option<f64>,
        order: Option<i64>,
        time_in_seconds: f64,
        fade_mode: &str,
    ) -> Result<(), VTubeError> {
        let client = self.get()?;
        client
            .move_item(
                item_instance_id,
                x,
                y,
                size,
                rotation,
                order,
                time_in_seconds,
                fade_mode,
            )
            .await
    }

    async fn get_current_model(&self) -> Result<Variant, VTubeError> {
        let client = self.get()?;
        client.get_current_model().await
    }

    async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
        let client = self.get()?;
        client.get_hotkeys().await
    }

    async fn get_expressions(&self) -> Result<Variant, VTubeError> {
        let client = self.get()?;
        client.get_expressions().await
    }

    async fn get_parameters(&self) -> Result<Variant, VTubeError> {
        let client = self.get()?;
        client.get_parameters().await
    }

    async fn get_items(&self) -> Result<Variant, VTubeError> {
        let client = self.get()?;
        client.get_items().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Forwarding to an installed client is not unit-testable: `install` takes a
    // concrete `Arc<VTubeClient>`, which always opens a WebSocket supervisor.

    #[tokio::test]
    async fn every_call_on_an_empty_sink_returns_not_connected() {
        let sink = SwitchableVTubeSink::new();
        assert!(matches!(
            sink.trigger_hotkey("hk").await,
            Err(VTubeError::NotConnected)
        ));
        assert!(matches!(
            sink.set_expression("file.exp3.json", true).await,
            Err(VTubeError::NotConnected)
        ));
        assert!(matches!(
            sink.set_param("ParamAngleX", 0.5).await,
            Err(VTubeError::NotConnected)
        ));
        assert!(matches!(
            sink.load_model("model").await,
            Err(VTubeError::NotConnected)
        ));
        assert!(matches!(
            sink.reset_params().await,
            Err(VTubeError::NotConnected)
        ));
        assert!(matches!(
            sink.move_model(Some(0.0), None, None, 0.5).await,
            Err(VTubeError::NotConnected)
        ));
    }

    #[tokio::test]
    async fn clear_on_an_empty_sink_is_idempotent() {
        let sink = SwitchableVTubeSink::new();
        sink.clear();
        sink.clear();
        assert!(matches!(
            sink.trigger_hotkey("hk").await,
            Err(VTubeError::NotConnected)
        ));
    }
}
