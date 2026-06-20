use async_trait::async_trait;
use forge_types::Variant;

use crate::ObsError;

#[async_trait]
pub trait ObsSink: Send + Sync {
    async fn set_scene(&self, scene: &str) -> Result<(), ObsError>;

    async fn set_source_visible(
        &self,
        scene: &str,
        source: &str,
        visible: bool,
    ) -> Result<(), ObsError>;

    async fn set_input_mute(&self, input: &str, mute: bool) -> Result<(), ObsError>;

    async fn start_record(&self) -> Result<(), ObsError>;

    async fn stop_record(&self) -> Result<(), ObsError>;

    async fn start_stream(&self) -> Result<(), ObsError>;

    async fn stop_stream(&self) -> Result<(), ObsError>;

    async fn raw_request(&self, request_type: &str, payload: &Variant)
    -> Result<Variant, ObsError>;

    async fn set_preview_scene(&self, scene: &str) -> Result<(), ObsError>;

    async fn set_current_scene_transition(&self, name: &str) -> Result<(), ObsError>;

    async fn set_input_volume_db(&self, input: &str, db: f64) -> Result<(), ObsError>;

    async fn set_input_settings(
        &self,
        input: &str,
        settings: &Variant,
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

    async fn get_scene_list(&self) -> Result<Variant, ObsError>;

    async fn get_input_list(&self) -> Result<Variant, ObsError>;

    async fn get_record_status(&self) -> Result<Variant, ObsError>;

    async fn get_stream_status(&self) -> Result<Variant, ObsError>;

    async fn get_input_settings(&self, input: &str) -> Result<Variant, ObsError>;
}
