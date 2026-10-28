use async_trait::async_trait;
use forge_types::{ClipId, OutputDevice};

use crate::error::SoundboardError;

#[async_trait]
pub trait SoundboardControl: Send + Sync {
    async fn play(
        &self,
        clip_id: ClipId,
        output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundboardError>;

    async fn stop(&self, clip_id: ClipId) -> Result<(), SoundboardError>;

    async fn stop_all(&self) -> Result<(), SoundboardError>;

    async fn set_master_volume(&self, gain: f32) -> Result<(), SoundboardError>;
}
