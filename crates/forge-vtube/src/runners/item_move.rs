use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, IntegerField, OptionalField, RunContext,
    StepTimer, SubActionCategory, SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionTelemetry};

use crate::runners::choices::FadeMode;
use crate::runners::fields::{
    DURATION, IMMEDIATE_SECONDS, ITEM_INSTANCE_ID, ITEM_SIZE, ROTATION, X, Y,
    optional_whole_number, read_non_empty,
};
use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const ORDER: OptionalField<IntegerField> = optional_whole_number(
    config_keys::ORDER,
    "Order",
    ValueBounds {
        min: Some(-1000),
        max: Some(1000),
    },
);

const FADE_MODE: ChoiceField<FadeMode> = ChoiceField {
    key: config_keys::FADE_MODE,
    label: "Fade Mode",
    default: FadeMode::Linear,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &ITEM_INSTANCE_ID,
    &X,
    &Y,
    &ITEM_SIZE,
    &ROTATION,
    &ORDER,
    &DURATION,
    &FADE_MODE,
]);

struct MoveTarget {
    item_instance_id: String,
    x: Option<f64>,
    y: Option<f64>,
    size: Option<f64>,
    rotation: Option<f64>,
    order: Option<i64>,
    duration: Option<f64>,
    fade_mode: FadeMode,
}

impl MoveTarget {
    fn is_empty(&self) -> bool {
        self.x.is_none()
            && self.y.is_none()
            && self.size.is_none()
            && self.rotation.is_none()
            && self.order.is_none()
    }
}

fn read_target(config: &SubActionConfig, args: &ArgStack) -> Result<MoveTarget, ConfigProblem> {
    Ok(MoveTarget {
        item_instance_id: read_non_empty(&ITEM_INSTANCE_ID, config, args)?,
        x: X.read(config, args)?,
        y: Y.read(config, args)?,
        size: ITEM_SIZE.read(config, args)?,
        rotation: ROTATION.read(config, args)?,
        order: ORDER.read(config, args)?,
        duration: DURATION.read(config, args)?,
        fade_mode: FADE_MODE.read(config, args)?,
    })
}

pub(crate) struct ItemMoveRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemMoveRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ItemMoveRunner {
    fn id(&self) -> &str {
        sub_action_ids::ITEM_MOVE
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Move Item"
    }

    fn summary(&self) -> &str {
        "Moves, resizes, or rotates a loaded VTube Studio item on screen."
    }

    fn search_text(&self) -> &str {
        "vtube item move position size rotate order fade vts"
    }

    fn icon_name(&self) -> &str {
        "arrows-move"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let target = match read_target(config, ctx.arg_stack) {
            Ok(target) => target,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if target.is_empty() {
            return (timer.success(), None);
        }
        let outcome = step_failure::outcome(
            &self
                .sink
                .move_item(
                    &target.item_instance_id,
                    target.x,
                    target.y,
                    target.size,
                    target.rotation,
                    target.order,
                    target.duration.unwrap_or(IMMEDIATE_SECONDS),
                    target.fade_mode.wire_name(),
                )
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn config_with_id(id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "item_instance_id".to_owned(),
            Variant::String(id.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_valid_id_no_positional_fields_is_noop() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config_with_id("item-1"), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
        assert!(
            !sink.was_called(),
            "sink must not be called when no positional field is set"
        );
    }

    #[tokio::test]
    async fn execute_empty_id_is_failed_without_calling_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&runner.default_config(), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(
            !sink.was_called(),
            "empty id must short-circuit before reaching the sink"
        );
    }

    #[tokio::test]
    async fn execute_failed_outcome_when_sink_errors() {
        let sink = Arc::new(MockSink::failing());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("size".to_owned(), Variant::Float(0.3));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
    }

    #[test]
    fn validate_config_accepts_string_id() {
        let runner = ItemMoveRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&config_with_id("item-1")).is_ok());
    }

    #[test]
    fn validate_config_rejects_non_string_id() {
        let runner = ItemMoveRunner::new(Arc::new(MockSink::new()));
        let int_id = BTreeMap::from([("item_instance_id".to_owned(), Variant::Int(7))]);
        assert!(runner.validate_config(&int_id).is_err());
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[tokio::test]
    async fn execute_forwards_resolved_id_and_positional_arg_to_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack =
            ArgStack::new().set("item".to_owned(), Variant::String("resolved-42".to_owned()));
        let mut config = config_with_id("%item%");
        config.insert("x".to_owned(), Variant::Float(0.5));
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(
            sink.strings_sent_to("move_item")
                .expect("sink must have been called")[0],
            "resolved-42",
            "item_instance_id must be interpolated before reaching the sink"
        );
        let x = sink
            .numbers_sent_to("move_item")
            .expect("sink must have been called")[0]
            .expect("x was set");
        assert!((x - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn execute_fails_an_unknown_fade_mode_without_moving_the_item() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("rotation".to_owned(), Variant::Float(10.0));
        config.insert("fade_mode".to_owned(), Variant::String("wobble".to_owned()));
        let stack = ArgStack::new();

        let (tel, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert!(
            matches!(&tel.outcome, SubActionOutcome::Failed(reason)
                if crate::runners::test_support::rejected_field(reason) == Some("fade_mode")),
            "{:?}",
            tel.outcome,
        );
        assert!(!sink.was_called());
    }

    #[tokio::test]
    async fn execute_valid_fade_mode_is_forwarded_unchanged() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("rotation".to_owned(), Variant::Float(10.0));
        config.insert("fade_mode".to_owned(), Variant::String("easeIn".to_owned()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        runner.execute(&config, &ctx).await;
        assert_eq!(
            sink.strings_sent_to("move_item")
                .expect("sink must have been called")[1],
            "easeIn",
            "a recognised fade_mode must pass through verbatim"
        );
    }
}
