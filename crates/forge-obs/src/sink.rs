use async_trait::async_trait;
use forge_types::Variant;

use crate::ObsError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneList {
    pub names: Vec<String>,
    pub current: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordStatus {
    pub active: bool,
    pub paused: bool,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamStatus {
    pub active: bool,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputSettings {
    pub settings: Variant,
    pub kind: String,
}

#[async_trait]
pub trait ObsSink: Send + Sync {
    async fn set_scene(&self, scene: &str) -> Result<(), ObsError>;

    async fn set_source_visible(
        &self,
        scene: &str,
        source: &str,
        visible: bool,
    ) -> Result<(), ObsError>;

    async fn set_source_locked(
        &self,
        scene: &str,
        source: &str,
        locked: bool,
    ) -> Result<(), ObsError>;

    async fn set_input_mute(&self, input: &str, mute: bool) -> Result<(), ObsError>;

    async fn start_record(&self) -> Result<(), ObsError>;

    async fn stop_record(&self) -> Result<(), ObsError>;

    async fn start_stream(&self) -> Result<(), ObsError>;

    async fn stop_stream(&self) -> Result<(), ObsError>;

    async fn set_preview_scene(&self, scene: &str) -> Result<(), ObsError>;

    async fn set_current_scene_transition(&self, name: &str) -> Result<(), ObsError>;

    async fn set_input_volume_db(&self, input: &str, db: f64) -> Result<(), ObsError>;

    async fn set_input_settings(
        &self,
        input: &str,
        settings: &serde_json::Map<String, serde_json::Value>,
        overlay: bool,
    ) -> Result<(), ObsError>;

    async fn pause_record(&self) -> Result<(), ObsError>;

    async fn resume_record(&self) -> Result<(), ObsError>;

    async fn toggle_record_pause(&self) -> Result<(), ObsError>;

    async fn send_stream_caption(&self, text: &str) -> Result<(), ObsError>;

    async fn start_replay_buffer(&self) -> Result<(), ObsError>;

    async fn stop_replay_buffer(&self) -> Result<(), ObsError>;

    async fn save_replay_buffer(&self) -> Result<(), ObsError>;

    async fn set_studio_mode(&self, enabled: bool) -> Result<(), ObsError>;

    async fn trigger_studio_transition(&self) -> Result<(), ObsError>;

    async fn get_scene_list(&self) -> Result<SceneList, ObsError>;

    async fn get_input_list(&self) -> Result<Vec<String>, ObsError>;

    async fn get_record_status(&self) -> Result<RecordStatus, ObsError>;

    async fn get_stream_status(&self) -> Result<StreamStatus, ObsError>;

    async fn get_current_scene(&self) -> Result<Option<String>, ObsError>;

    async fn get_input_settings(&self, input: &str) -> Result<InputSettings, ObsError>;

    async fn set_source_filter_enabled(
        &self,
        source: &str,
        filter: &str,
        enabled: bool,
    ) -> Result<(), ObsError>;

    async fn refresh_browser_source(&self, input: &str) -> Result<(), ObsError>;

    async fn restart_media_input(&self, input: &str) -> Result<(), ObsError>;

    async fn start_virtual_cam(&self) -> Result<(), ObsError>;

    async fn stop_virtual_cam(&self) -> Result<(), ObsError>;

    async fn save_source_screenshot(
        &self,
        source: &str,
        file_path: &str,
        format: &str,
    ) -> Result<(), ObsError>;

    async fn set_record_directory(&self, path: &str) -> Result<(), ObsError>;

    async fn set_current_profile(&self, name: &str) -> Result<(), ObsError>;

    async fn set_current_scene_collection(&self, name: &str) -> Result<(), ObsError>;
}
