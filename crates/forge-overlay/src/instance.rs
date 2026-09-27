use crate::descriptor::OverlayConfig;
use crate::media::OverlayMedia;
use crate::sample::SampleContext;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayInstance {
    pub id: String,
    pub display_name: String,
    pub kind_id: String,
    pub config: OverlayConfig,
    pub source_overrides: Vec<String>,
    pub credential: Option<String>,
    pub media: OverlayMedia,
    pub sample: SampleContext,
}
