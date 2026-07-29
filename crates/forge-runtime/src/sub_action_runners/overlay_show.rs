use async_trait::async_trait;
use forge_overlay::config::{DURATION_MAX_SECS, DURATION_MIN_SECS};
use forge_registry::{
    FormField, FormRefinement, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_storage::{OverlayConfig, OverlayId};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::overlay_service::OverlayServiceCell;

const OVERLAY_KEY: &str = "overlay_id";
const DURATION_KEY: &str = "duration_secs";

/// Names the catalog a host resolves `overlay_id` in to get the target type's content fields.
pub const CONTENT_SCHEMA_KEY: &str = "overlay.content_fields";

pub struct OverlayShowRunner {
    overlays: OverlayServiceCell,
}

impl OverlayShowRunner {
    pub fn new(overlays: OverlayServiceCell) -> Self {
        Self { overlays }
    }

    async fn show(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let target = ctx
            .arg_stack
            .interpolate(config.str(OVERLAY_KEY).unwrap_or_default());
        let identity = target.trim();
        if identity.is_empty() {
            return SubActionOutcome::Failed("overlay.show: no overlay is selected".to_owned());
        }

        let Some(overlays) = self.overlays.get() else {
            return SubActionOutcome::Failed(
                "overlay.show: the overlay service is not running".to_owned(),
            );
        };

        match overlays
            .show(
                &OverlayId::new(identity),
                &supplied_content(config),
                ctx.arg_stack,
                duration_override(config),
            )
            .await
        {
            Ok(true) => SubActionOutcome::Success,
            Ok(false) => {
                tracing::debug!(overlay = %identity, "overlay content had no page to reach");
                SubActionOutcome::Success
            }
            Err(error) => SubActionOutcome::Failed(format!("overlay.show: {error}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for OverlayShowRunner {
    fn id(&self) -> &str {
        "overlay.show"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Overlay
    }

    fn label(&self) -> &str {
        "Show on Overlay"
    }

    fn summary(&self) -> &str {
        "Send content to a browser-source overlay"
    }

    fn search_text(&self) -> &str {
        "overlay show alert browser source display banner goal ticker"
    }

    fn icon_name(&self) -> &str {
        "layout"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(OVERLAY_KEY.to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: OVERLAY_KEY,
                label: "Overlay",
                options_key: "overlay.ids",
            },
            FormField::Optional {
                key: DURATION_KEY,
                label: "Override duration",
                inner: Box::new(FormField::Slider {
                    key: DURATION_KEY,
                    label: "Override duration",
                    min: DURATION_MIN_SECS,
                    max: DURATION_MAX_SECS,
                    unit: "s",
                }),
            },
        ]
    }

    fn config_refinement(&self) -> Option<FormRefinement> {
        Some(FormRefinement {
            selector_key: OVERLAY_KEY,
            schema_key: CONTENT_SCHEMA_KEY,
        })
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str(OVERLAY_KEY).map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "overlay.show");
        let outcome = self.show(config, ctx).await;
        (timer.finish(outcome), None)
    }
}

/// Everything the runner does not own is a content candidate; the target kind's content group
/// decides which of them survive.
fn supplied_content(config: &SubActionConfig) -> OverlayConfig {
    config
        .iter()
        .filter(|(key, _)| key.as_str() != OVERLAY_KEY && key.as_str() != DURATION_KEY)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// The optional field holds a bool while the override is off, so only a whole number counts.
fn duration_override(config: &SubActionConfig) -> Option<u64> {
    let Some(Variant::Int(seconds)) = config.get(DURATION_KEY) else {
        return None;
    };
    u64::try_from(*seconds)
        .ok()
        .filter(|seconds| *seconds > 0)
        .map(|seconds| seconds.saturating_mul(1000))
}
