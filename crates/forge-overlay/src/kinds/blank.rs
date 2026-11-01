use forge_types::MessageCode;

use crate::catalog::descriptor::{DeliveryDisposition, OverlayConfig, OverlayKindDescriptor};
use crate::files::assets::PageAssets;
use crate::messages;
use crate::render::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.blank";

pub struct BlankOverlayKind;

impl OverlayKindDescriptor for BlankOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> MessageCode {
        messages::BLANK_LABEL
    }

    fn summary(&self) -> MessageCode {
        messages::BLANK_SUMMARY
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

    fn preview(&self, config: &OverlayConfig, _: forge_types::NamesLanguage) -> PreviewComposition {
        compose(PreviewShape::Blank, self.sizing(), config)
    }

    fn has_visual_page(&self) -> bool {
        false
    }
}
