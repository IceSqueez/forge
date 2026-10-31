use crate::client::ObsClient;

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub audio_db: Option<f32>,
    pub kind: Option<String>,
}

impl ObsClient {
    pub fn scene_names(&self) -> Vec<String> {
        self.mirror.read(|catalog| catalog.scenes.clone())
    }

    pub fn scene_items(&self, scene: &str) -> Vec<SourceInfo> {
        self.mirror
            .read(|catalog| catalog.sources.get(scene).cloned())
            .unwrap_or_default()
    }
}
