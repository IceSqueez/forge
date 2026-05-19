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
}
