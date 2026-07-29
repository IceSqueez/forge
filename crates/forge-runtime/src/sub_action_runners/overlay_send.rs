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

pub struct OverlaySendRunner {
    overlays: OverlayServiceCell,
}

impl OverlaySendRunner {
    pub fn new(overlays: OverlayServiceCell) -> Self {
        Self { overlays }
    }

    async fn send(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let target = ctx
            .arg_stack
            .interpolate(config.str(OVERLAY_KEY).unwrap_or_default());
        let identity = target.trim();
        if identity.is_empty() {
            return SubActionOutcome::Failed("overlay.send: no overlay is selected".to_owned());
        }

        let Some(overlays) = self.overlays.get() else {
            return SubActionOutcome::Failed(
                "overlay.send: the overlay service is not running".to_owned(),
            );
        };

        match overlays
            .send_to(
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
            Err(error) => SubActionOutcome::Failed(format!("overlay.send: {error}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for OverlaySendRunner {
    fn id(&self) -> &str {
        "overlay.send"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Overlay
    }

    fn label(&self) -> &str {
        "Send to Overlay"
    }

    fn summary(&self) -> &str {
        "Send content to a browser-source overlay"
    }

    fn search_text(&self) -> &str {
        "overlay send show alert browser source display banner goal ticker"
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
        let timer = StepTimer::start(ctx, "overlay.send");
        let outcome = self.send(config, ctx).await;
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};

    use super::*;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn config(pairs: &[(&str, Variant)]) -> SubActionConfig {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    #[test]
    fn an_override_duration_becomes_milliseconds_only_for_a_positive_whole_number_of_seconds() {
        for (held, expected, label) in [
            (
                Some(Variant::Int(5)),
                Some(5_000),
                "a duration inside the slider range",
            ),
            (
                Some(Variant::Int(1)),
                Some(1_000),
                "the shortest duration offered",
            ),
            (
                Some(Variant::Int(15)),
                Some(15_000),
                "the longest duration offered",
            ),
            (Some(Variant::Int(0)), None, "a duration of no time at all"),
            (Some(Variant::Int(-1)), None, "a duration below zero"),
            (
                Some(Variant::Int(i64::MAX)),
                Some(u64::MAX),
                "a duration large enough to overflow the conversion",
            ),
            (
                Some(Variant::Bool(false)),
                None,
                "the toggle while the override is off",
            ),
            (
                Some(Variant::Bool(true)),
                None,
                "the toggle switched on before a number was picked",
            ),
            (
                Some(Variant::String("5".to_owned())),
                None,
                "seconds written as text",
            ),
            (None, None, "no override field at all"),
        ] {
            let cfg = held.map_or_else(SubActionConfig::new, |value| {
                config(&[(DURATION_KEY, value)])
            });

            assert_eq!(duration_override(&cfg), expected, "{label}");
        }
    }

    #[test]
    fn the_runners_own_fields_are_never_offered_to_the_overlay_as_content() {
        let cfg = config(&[
            (OVERLAY_KEY, Variant::String("goal-box".to_owned())),
            (DURATION_KEY, Variant::Int(5)),
            ("value", Variant::String("42".to_owned())),
            ("target", Variant::Int(100)),
        ]);

        let supplied = supplied_content(&cfg);

        assert_eq!(
            supplied.keys().cloned().collect::<BTreeSet<String>>(),
            BTreeSet::from(["value".to_owned(), "target".to_owned()]),
            "the step's own selector or timer reached the overlay as a content field"
        );
        assert_eq!(
            supplied.get("target"),
            Some(&Variant::Int(100)),
            "a content candidate must reach the funnel with the value the step holds"
        );
    }

    #[tokio::test]
    async fn a_step_that_cannot_reach_an_overlay_fails_rather_than_reporting_success() {
        let runner = OverlaySendRunner::new(OverlayServiceCell::new());
        let stack = ArgStack::new();

        for (cfg, label) in [
            (
                SubActionConfig::new(),
                "a step saved before an overlay was picked",
            ),
            (
                config(&[(OVERLAY_KEY, Variant::String(String::new()))]),
                "an overlay selection cleared back to nothing",
            ),
            (
                config(&[(OVERLAY_KEY, Variant::String("   ".to_owned()))]),
                "an overlay selection holding only blank space",
            ),
            (
                config(&[(OVERLAY_KEY, Variant::String("goal-box".to_owned()))]),
                "an overlay service that is not running yet",
            ),
        ] {
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);

            let (telemetry, _) = runner.execute(&cfg, &ctx).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "{label} produced {:?}, so the run history shows a delivery that never happened",
                telemetry.outcome
            );
        }
    }
}
