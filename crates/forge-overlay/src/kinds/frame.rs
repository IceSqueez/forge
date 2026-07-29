use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.frame";

pub struct FrameOverlayKind;

impl OverlayKindDescriptor for FrameOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Frame"
    }

    fn summary(&self) -> &str {
        "Borders a camera or capture area and labels its corner"
    }

    fn icon_name(&self) -> &str {
        "frame"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Replace
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn config_schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> OverlayConfig {
        let mut defaults = config::shared_defaults("peach", "Inter", "center", "fade");
        defaults.insert(config::HEADLINE.to_owned(), config::text(""));
        defaults.insert(config::SUBLINE.to_owned(), config::text("LIVE"));
        defaults
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = config::shared_fields();
        fields.push(config::sound_field());
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/frame/index.html"),
            style: include_str!("../../assets/frame/overlay.css"),
            behavior: include_str!("../../assets/frame/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::BorderedFrame, config)
    }
}
