use forge_types::OverlayCredential;

use crate::catalog::descriptor::OverlayConfig;
use crate::files::media::OverlayMedia;
use crate::render::sample::SampleContext;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayInstance {
    pub id: String,
    pub display_name: String,
    pub kind_id: String,
    pub config: OverlayConfig,
    pub source_overrides: Vec<String>,
    pub credential: Option<OverlayCredential>,
    pub media: OverlayMedia,
    pub sample: SampleContext,
}
