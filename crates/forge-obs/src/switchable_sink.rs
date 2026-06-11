// Lets runner registration happen at boot (before async client connect) while still
// forwarding calls to the real client once it arrives.
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::client::ObsClient;
use crate::error::ObsError;
use crate::sink::ObsSink;
use forge_types::Variant;

pub struct SwitchableObsSink {
    inner: RwLock<Option<Arc<ObsClient>>>,
}

impl SwitchableObsSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(None),
        })
    }

    pub fn install(&self, client: Arc<ObsClient>) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub fn clear(&self) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    // Clone the Arc out before any await so the sync RwLock guard is never held
    // across an async call.
    fn get(&self) -> Result<Arc<ObsClient>, ObsError> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard.clone().ok_or(ObsError::Disconnected)
    }
}

impl Default for SwitchableObsSink {
    fn default() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }
}

#[async_trait]
impl ObsSink for SwitchableObsSink {
    async fn set_scene(&self, scene: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_scene(scene).await
    }

    async fn set_source_visible(
        &self,
        scene: &str,
        source: &str,
        visible: bool,
    ) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_source_visible(scene, source, visible).await
    }

    async fn set_input_mute(&self, input: &str, mute: bool) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_input_mute(input, mute).await
    }

    async fn start_record(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.start_record().await
    }

    async fn stop_record(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.stop_record().await
    }

    async fn start_stream(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.start_stream().await
    }

    async fn stop_stream(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.stop_stream().await
    }

    async fn raw_request(
        &self,
        request_type: &str,
        payload: &Variant,
    ) -> Result<Variant, ObsError> {
        let client = self.get()?;
        client.raw_request(request_type, payload).await
    }
}
