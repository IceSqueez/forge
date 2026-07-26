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

    // sync RwLock guard must not cross an .await; clone the Arc out first.
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

    async fn set_preview_scene(&self, scene: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_preview_scene(scene).await
    }

    async fn set_current_scene_transition(&self, name: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_current_scene_transition(name).await
    }

    async fn set_input_volume_db(&self, input: &str, db: f64) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_input_volume_db(input, db).await
    }

    async fn set_input_settings(
        &self,
        input: &str,
        settings: &Variant,
        overlay: bool,
    ) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_input_settings(input, settings, overlay).await
    }

    async fn pause_record(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.pause_record().await
    }

    async fn resume_record(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.resume_record().await
    }

    async fn toggle_record_pause(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.toggle_record_pause().await
    }

    async fn send_stream_caption(&self, text: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.send_stream_caption(text).await
    }

    async fn start_replay_buffer(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.start_replay_buffer().await
    }

    async fn stop_replay_buffer(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.stop_replay_buffer().await
    }

    async fn save_replay_buffer(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.save_replay_buffer().await
    }

    async fn set_studio_mode(&self, enabled: bool) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_studio_mode(enabled).await
    }

    async fn trigger_studio_transition(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.trigger_studio_transition().await
    }

    async fn get_scene_list(&self) -> Result<Variant, ObsError> {
        let client = self.get()?;
        client.get_scene_list().await
    }

    async fn get_input_list(&self) -> Result<Variant, ObsError> {
        let client = self.get()?;
        client.get_input_list().await
    }

    async fn get_record_status(&self) -> Result<Variant, ObsError> {
        let client = self.get()?;
        client.get_record_status().await
    }

    async fn get_stream_status(&self) -> Result<Variant, ObsError> {
        let client = self.get()?;
        client.get_stream_status().await
    }

    async fn get_input_settings(&self, input: &str) -> Result<Variant, ObsError> {
        let client = self.get()?;
        client.get_input_settings(input).await
    }

    async fn get_current_scene(&self) -> Result<Option<String>, ObsError> {
        let client = self.get()?;
        client.get_current_scene().await
    }

    async fn set_source_filter_enabled(
        &self,
        source: &str,
        filter: &str,
        enabled: bool,
    ) -> Result<(), ObsError> {
        let client = self.get()?;
        client
            .set_source_filter_enabled(source, filter, enabled)
            .await
    }

    async fn refresh_browser_source(&self, input: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.refresh_browser_source(input).await
    }

    async fn restart_media_input(&self, input: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.restart_media_input(input).await
    }

    async fn start_virtual_cam(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.start_virtual_cam().await
    }

    async fn stop_virtual_cam(&self) -> Result<(), ObsError> {
        let client = self.get()?;
        client.stop_virtual_cam().await
    }

    async fn save_source_screenshot(
        &self,
        source: &str,
        file_path: &str,
        format: &str,
    ) -> Result<(), ObsError> {
        let client = self.get()?;
        client
            .save_source_screenshot(source, file_path, format)
            .await
    }

    async fn set_record_directory(&self, path: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_record_directory(path).await
    }

    async fn set_current_profile(&self, name: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_current_profile(name).await
    }

    async fn set_current_scene_collection(&self, name: &str) -> Result<(), ObsError> {
        let client = self.get()?;
        client.set_current_scene_collection(name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;

    #[tokio::test]
    async fn every_method_on_an_empty_sink_returns_disconnected() {
        let sink = SwitchableObsSink::new();
        let payload = Variant::Object(Default::default());

        let results: Vec<Result<(), ObsError>> = vec![
            sink.set_scene("scene").await,
            sink.set_source_visible("scene", "src", true).await,
            sink.set_input_mute("mic", true).await,
            sink.start_record().await,
            sink.stop_record().await,
            sink.start_stream().await,
            sink.stop_stream().await,
        ];
        for result in results {
            assert!(
                matches!(result, Err(ObsError::Disconnected)),
                "expected Disconnected, got {result:?}",
            );
        }

        let raw = sink.raw_request("GetVersion", &payload).await;
        assert!(
            matches!(raw, Err(ObsError::Disconnected)),
            "raw_request: expected Disconnected, got {raw:?}",
        );
    }
}
