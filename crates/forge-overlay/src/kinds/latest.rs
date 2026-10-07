use forge_types::LATEST_DONATION_SLOT;

use crate::assets::PageAssets;
use crate::base;
use crate::config::{self, SPEECH, SPEECH_VOICE};
use crate::descriptor::{
    ContentFeed, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::latest::{SLOT_OPTIONS, latest_content, sample_latest_value};
use crate::metrics;
use crate::motion::{self, MotionAxes, MotionProfile};
use crate::preview::{PreviewComposition, PreviewShape, compose};
use crate::source_box::{ContentMargins, DesignSize};

pub const KIND_ID: &str = "overlay.latest";

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

const DESIGN_WIDTH_PX: u32 = 600;
const DESIGN_HEIGHT_PX: u32 = 96;

const SPEECH_KEYS: [&str; 2] = [SPEECH, SPEECH_VOICE];

pub struct LatestOverlayKind;

impl OverlayKindDescriptor for LatestOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Latest"
    }

    fn summary(&self) -> &str {
        "Keeps the latest value of a slot, such as the last donation, on screen across restarts"
    }

    fn icon_name(&self) -> &str {
        "history"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Replace
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn content_feed(&self) -> ContentFeed {
        ContentFeed::LatestSlot
    }

    fn config_schema_version(&self) -> u32 {
        CONFIG_SCHEMA_VERSION
    }

    fn motion(&self) -> MotionProfile {
        MotionProfile {
            axes: MotionAxes::ENTRANCE_AND_TEXT,
            entrance: motion::SLIDE_UP,
            entrance_ms: motion::DEFAULT_ENTRANCE_MS,
            exit: motion::NO_MOTION,
            exit_ms: motion::DEFAULT_EXIT_MS,
            reveal_target_id: motion::STAGE_ID,
        }
    }

    fn default_design_size(&self) -> DesignSize {
        DesignSize {
            width: DESIGN_WIDTH_PX,
            height: DESIGN_HEIGHT_PX,
        }
    }

    fn default_margins(&self) -> ContentMargins {
        ContentMargins::NONE
    }

    fn look_defaults(&self) -> OverlayConfig {
        let mut defaults = config::shared_style_defaults("peach", "Inter", metrics::LATEST_SIZING);
        defaults.extend([
            (config::SLOT.to_owned(), config::text(LATEST_DONATION_SLOT)),
            (config::PLATFORM.to_owned(), config::text("")),
            (config::LABEL.to_owned(), config::text("Last donation")),
            (config::HEADLINE.to_owned(), config::text("%user_name%")),
            (
                config::SUBLINE.to_owned(),
                config::text("%amount_formatted%"),
            ),
            (
                config::PLACEHOLDER.to_owned(),
                config::text("No donations yet"),
            ),
        ]);
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = vec![
            config::slot_field(SLOT_OPTIONS),
            config::platform_scope_field(),
            config::label_field(),
            config::headline_field("%user_name%"),
            config::subline_field("%amount_formatted%"),
            config::placeholder_field(),
        ];
        fields.extend(config::shared_style_fields());
        fields
    }

    fn default_config(&self) -> OverlayConfig {
        let mut defaults = base::base_defaults(
            self.delivery_disposition(),
            self.default_display_secs(),
            self.default_design_size(),
            self.default_margins(),
        );
        defaults.extend(self.look_defaults());
        defaults.extend(motion::motion_defaults(self.motion()));
        for key in SPEECH_KEYS {
            defaults.remove(key);
        }
        defaults
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = self.look_fields();
        fields.extend(motion::motion_fields(self.motion().axes));
        fields.extend(
            base::base_fields(self.delivery_disposition())
                .into_iter()
                .filter(|sectioned| !SPEECH_KEYS.contains(&sectioned.field.key())),
        );
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/latest/index.html"),
            style: include_str!("../../assets/latest/overlay.css"),
            behavior: include_str!("../../assets/latest/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        let sample = sample_latest_value(config::read_str(config, config::SLOT));
        let mut shown = config.clone();
        shown.extend(latest_content(self, config, Some(&sample)));
        compose(
            PreviewShape::LatestCard,
            metrics::element_sizing(KIND_ID),
            &shown,
        )
    }
}
