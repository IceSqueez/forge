use async_trait::async_trait;
use forge_overlay::config::{DURATION_MAX_SECS, DURATION_MIN_SECS};
use forge_registry::references::OVERLAY_OPTIONS_KEY;
use forge_registry::{
    ConfigProblem, ConfigSpec, DynamicSelectField, FormRefinement, IntegerEditor, IntegerField,
    OptionalField, RunContext, StepTimer, SubActionCategory, SubActionRunner, ToggleField,
    TypedField, ValueBounds,
};
use forge_storage::{OverlayConfig, OverlayId};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, UserText};

use super::overlay_targets::{OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY};
use crate::overlay_service::{OverlayDelivery, OverlayDispatch, OverlayServiceCell};
use crate::overlay_shows::ShowEnd;

const DURATION_KEY: &str = "duration_secs";
const WAIT_KEY: &str = "wait_for_show";
const MILLIS_PER_SECOND: u64 = 1_000;

pub const CONTENT_SCHEMA_KEY: &str = "overlay.content_fields";

const TARGET: DynamicSelectField<String> =
    DynamicSelectField::new(OVERLAY_TARGET_KEY, "Overlay", OVERLAY_OPTIONS_KEY);

const DURATION: OptionalField<IntegerField> = OptionalField::shared(IntegerField {
    key: DURATION_KEY,
    label: "Override duration",
    editor: IntegerEditor::Slider { unit: "s" },
    default: DURATION_MIN_SECS,
    bounds: ValueBounds {
        min: Some(DURATION_MIN_SECS),
        max: Some(DURATION_MAX_SECS),
    },
});

const WAIT: ToggleField = ToggleField {
    key: WAIT_KEY,
    label: "Wait until the show ends",
    default: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TARGET, &DURATION, &WAIT]);

struct SendConfig {
    target: String,
    duration_ms: Option<u64>,
    wait_for_show: bool,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<SendConfig, ConfigProblem> {
    Ok(SendConfig {
        target: TARGET.read(config, args)?,
        duration_ms: DURATION.read(config, args)?.and_then(|seconds| {
            u64::try_from(seconds)
                .ok()
                .map(|seconds| seconds.saturating_mul(MILLIS_PER_SECOND))
        }),
        wait_for_show: WAIT.read(config, args)?,
    })
}

pub struct OverlaySendRunner {
    overlays: OverlayServiceCell,
}

impl OverlaySendRunner {
    pub fn new(overlays: OverlayServiceCell) -> Self {
        Self { overlays }
    }

    async fn send(
        &self,
        config: &SubActionConfig,
        send: SendConfig,
        ctx: &RunContext<'_>,
    ) -> SubActionOutcome {
        let identity = send.target.as_str();
        let Some(overlays) = self.overlays.get() else {
            return SubActionOutcome::Failed(UserText::unlocalized(
                "overlay.send: the overlay service is not running",
            ));
        };

        match overlays
            .send_to(
                &OverlayId::new(identity),
                &supplied_content(config),
                ctx.arg_stack,
                send.duration_ms,
                Some(ctx.parent_event_id),
            )
            .await
        {
            Ok(OverlayDispatch::Applied(delivery)) => delivered(identity, delivery),
            Ok(OverlayDispatch::Queued(ticket)) if send.wait_for_show => {
                show_outcome(identity, ticket.finished().await)
            }
            Ok(OverlayDispatch::Queued(_)) => SubActionOutcome::Success,
            Err(error) => {
                SubActionOutcome::Failed(UserText::unlocalized(format!("overlay.send: {error}")))
            }
        }
    }
}

fn delivered(identity: &str, delivery: OverlayDelivery) -> SubActionOutcome {
    if delivery == OverlayDelivery::NoPage {
        tracing::debug!(overlay = %identity, "overlay content had no page to reach");
    }
    SubActionOutcome::Success
}

fn show_outcome(identity: &str, end: ShowEnd) -> SubActionOutcome {
    match end {
        ShowEnd::Shown(delivery) => delivered(identity, delivery),
        ShowEnd::Cleared => SubActionOutcome::Failed(UserText::unlocalized(
            "overlay.send: the overlay's show queue was cleared before this show ran",
        )),
        ShowEnd::Withdrawn => SubActionOutcome::Failed(UserText::unlocalized(
            "overlay.send: the overlay was disabled or deleted before the show ran",
        )),
    }
}

#[async_trait]
impl SubActionRunner for OverlaySendRunner {
    fn id(&self) -> &str {
        OVERLAY_SEND_KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn config_refinement(&self) -> Option<FormRefinement> {
        Some(FormRefinement {
            selector_key: OVERLAY_TARGET_KEY,
            schema_key: CONTENT_SCHEMA_KEY,
        })
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, OVERLAY_SEND_KIND_ID);
        let send = match read_config(config, ctx.arg_stack) {
            Ok(send) => send,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.send(config, send, ctx).await;
        (timer.finish(outcome), None)
    }
}

fn supplied_content(config: &SubActionConfig) -> OverlayConfig {
    config
        .iter()
        .filter(|(key, _)| ![OVERLAY_TARGET_KEY, DURATION_KEY, WAIT_KEY].contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};

    use super::*;
    use forge_types::Variant;

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

    fn override_ms(cfg: &SubActionConfig) -> Result<Option<u64>, ConfigProblem> {
        let mut targeted = cfg.clone();
        targeted.insert(
            OVERLAY_TARGET_KEY.to_owned(),
            Variant::String("goal-box".to_owned()),
        );
        read_config(&targeted, &ArgStack::new()).map(|read| read.duration_ms)
    }

    #[test]
    fn an_override_duration_inside_the_slider_range_becomes_milliseconds() {
        for (held, expected) in [
            (
                Variant::Int(DURATION_MIN_SECS),
                DURATION_MIN_SECS as u64 * 1_000,
            ),
            (Variant::Int(5), 5_000),
            (
                Variant::Int(DURATION_MAX_SECS),
                DURATION_MAX_SECS as u64 * 1_000,
            ),
            (Variant::String("5".to_owned()), 5_000),
        ] {
            let cfg = config(&[(DURATION_KEY, held.clone())]);
            assert_eq!(override_ms(&cfg), Ok(Some(expected)), "{held:?}");
        }
    }

    #[test]
    fn an_unset_override_leaves_the_overlay_its_own_duration() {
        for cfg in [
            SubActionConfig::new(),
            config(&[(DURATION_KEY, Variant::String(String::new()))]),
        ] {
            assert_eq!(override_ms(&cfg), Ok(None), "{cfg:?}");
        }
    }

    #[test]
    fn an_override_switched_off_by_an_older_editor_leaves_the_overlay_its_own_duration() {
        let cfg = config(&[(DURATION_KEY, Variant::Bool(false))]);

        assert_eq!(override_ms(&cfg), Ok(None));
    }

    #[test]
    fn an_override_duration_outside_the_slider_range_is_a_config_problem() {
        for held in [DURATION_MIN_SECS - 1, DURATION_MAX_SECS + 1, i64::MAX] {
            let cfg = config(&[(DURATION_KEY, Variant::Int(held))]);
            assert_eq!(
                override_ms(&cfg).unwrap_err(),
                ConfigProblem::field(
                    DURATION_KEY,
                    forge_registry::ConfigReason::OutOfRange(ValueBounds::between(
                        DURATION_MIN_SECS,
                        DURATION_MAX_SECS
                    ))
                ),
                "{held}"
            );
        }
    }

    #[test]
    fn the_runners_own_fields_are_never_offered_to_the_overlay_as_content() {
        let cfg = config(&[
            (OVERLAY_TARGET_KEY, Variant::String("goal-box".to_owned())),
            (DURATION_KEY, Variant::Int(5)),
            (WAIT_KEY, Variant::Bool(true)),
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
                config(&[(OVERLAY_TARGET_KEY, Variant::String(String::new()))]),
                "an overlay selection cleared back to nothing",
            ),
            (
                config(&[(OVERLAY_TARGET_KEY, Variant::String("   ".to_owned()))]),
                "an overlay selection holding only blank space",
            ),
            (
                config(&[(OVERLAY_TARGET_KEY, Variant::String("goal-box".to_owned()))]),
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
