use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.alert";

pub struct AlertOverlayKind;

impl OverlayKindDescriptor for AlertOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Alert"
    }

    fn summary(&self) -> &str {
        "Shows a banner for a few seconds each time an action sends one"
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Transient
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn config_schema_version(&self) -> u32 {
        2
    }

    fn default_config(&self) -> OverlayConfig {
        let mut defaults =
            config::shared_defaults("mauve", "Rubik", "top", "slide-up", metrics::ALERT_SIZING);
        defaults.insert(
            config::HEADLINE.to_owned(),
            config::text("Thanks for the sub!"),
        );
        defaults.insert(
            config::SUBLINE.to_owned(),
            config::text("%sub_cumulative_months% months subscribed"),
        );
        defaults.insert(config::DURATION.to_owned(), forge_types::Variant::Int(5));
        defaults.insert(config::ICON.to_owned(), config::text(config::DEFAULT_ICON));
        defaults
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = config::shared_fields(metrics::ALERT_SIZING);
        fields.push(config::icon_field());
        fields.push(config::duration_field());
        fields.push(config::sound_field());
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/alert/index.html"),
            style: include_str!("../../assets/alert/overlay.css"),
            behavior: include_str!("../../assets/alert/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::BadgeBanner, config)
    }
}
