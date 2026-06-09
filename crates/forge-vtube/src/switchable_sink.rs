// Lets runner registration happen at boot (before async client connect) while still
// forwarding calls to the real client once it arrives.
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

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
        if let Ok(mut guard) = self.inner.write() {
            *guard = Some(client);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = None;
        }
    }

    // Clone the Arc out before any await so the sync RwLock guard is never held
    // across an async call.
    fn get(&self) -> Result<Arc<VTubeClient>, VTubeError> {
        self.inner
            .read()
            .ok()
            .and_then(|g| g.clone())
            .ok_or(VTubeError::NotConnected)
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
}
