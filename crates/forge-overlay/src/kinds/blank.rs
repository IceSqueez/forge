use crate::assets::PageAssets;
use crate::descriptor::{DeliveryDisposition, OverlayConfig, OverlayKindDescriptor};
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.blank";

pub struct BlankOverlayKind;

impl OverlayKindDescriptor for BlankOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Blank"
    }

    fn summary(&self) -> &str {
        "Draws nothing on the canvas; it only plays the audio forge sends it"
    }

    fn icon_name(&self) -> &str {
        "volume"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Transient
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn config_schema_version(&self) -> u32 {
        1
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/blank/index.html"),
            style: include_str!("../../assets/blank/overlay.css"),
            behavior: include_str!("../../assets/blank/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::Blank, config)
    }

    fn has_visual_page(&self) -> bool {
        false
    }
}
