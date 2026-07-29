use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.goal";

pub struct GoalOverlayKind;

impl OverlayKindDescriptor for GoalOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Goal"
    }

    fn summary(&self) -> &str {
        "Tracks progress toward a target an action keeps updating"
    }

    fn icon_name(&self) -> &str {
        "target-arrow"
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
        let mut defaults = config::shared_style_defaults("green", "Inter", "bottom", "fade");
        defaults.insert(config::SOUND.to_owned(), config::text(""));
        defaults.insert(config::LABEL.to_owned(), config::text("Sub goal"));
        defaults.insert(config::VALUE.to_owned(), config::text("42"));
        defaults.insert(config::TARGET.to_owned(), config::text("100"));
        defaults
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = vec![
            config::label_field(),
            config::value_field(),
            config::target_field(),
        ];
        fields.extend(config::shared_style_fields());
        fields.push(config::sound_field());
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/goal/index.html"),
            style: include_str!("../../assets/goal/overlay.css"),
            behavior: include_str!("../../assets/goal/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::Strip, config)
    }
}
