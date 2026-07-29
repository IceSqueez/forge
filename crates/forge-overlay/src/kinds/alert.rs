use forge_registry::FormField;

use crate::config;
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
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
        "Shows a banner for a few seconds when the bound event fires"
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn config_schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> OverlayConfig {
        let mut defaults = config::shared_defaults("mauve", "Rubik", "top", "slide-up");
        defaults.insert(
            config::HEADLINE.to_owned(),
            config::text("%user% just subscribed!"),
        );
        defaults.insert(
            config::SUBLINE.to_owned(),
            config::text("Tier %tier% · %months% months"),
        );
        defaults.insert(config::DURATION.to_owned(), forge_types::Variant::Int(5));
        defaults
    }

    fn config_fields(&self) -> Vec<FormField> {
        let mut fields = config::shared_fields();
        fields.push(config::duration_field());
        fields.push(config::sound_field());
        fields
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::BadgeBanner, config)
    }
}
