#[derive(Debug, Clone)]
pub(crate) struct SourceInfo {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub audio_db: Option<f32>,
    pub kind: Option<String>,
}
