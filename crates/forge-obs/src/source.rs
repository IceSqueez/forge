use async_trait::async_trait;

use crate::ObsError;

pub struct SourceInfo {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub audio_db: Option<f32>,
}

#[async_trait]
pub trait ObsSource: Send + Sync {
    async fn scenes(&self) -> Result<Vec<String>, ObsError>;

    async fn current_scene(&self) -> Result<Option<String>, ObsError>;

    async fn sources(&self, scene: &str) -> Result<Vec<SourceInfo>, ObsError>;

    async fn audio_inputs(&self) -> Result<Vec<String>, ObsError>;

    async fn transitions(&self) -> Result<Vec<String>, ObsError>;

    async fn profiles(&self) -> Result<Vec<String>, ObsError>;

    async fn scene_collections(&self) -> Result<Vec<String>, ObsError>;
}
