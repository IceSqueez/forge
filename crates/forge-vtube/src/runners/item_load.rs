use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, IntegerField, NumberField, OptionalField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, ToggleField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use crate::runners::fields::{
    ITEM_FILE_NAME, ITEM_SIZE, ROTATION, X, Y, optional_number, optional_whole_number,
    read_non_empty,
};
use crate::runners::step_failure;
use crate::sink::{VTubeSink, result_keys};
use crate::vocabulary::{config_keys, sub_action_ids, variables};

const FADE_TIME: OptionalField<NumberField> =
    optional_number(config_keys::FADE_TIME, "Fade Time (s)", "0.5");

const ORDER: OptionalField<IntegerField> = optional_whole_number(
    config_keys::ORDER,
    "Order",
    ValueBounds {
        min: Some(0),
        max: Some(1000),
    },
);

const UNLOAD_ON_DISCONNECT: ToggleField = ToggleField {
    key: config_keys::UNLOAD_ON_DISCONNECT,
    label: "Unload When Plugin Disconnects",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &ITEM_FILE_NAME,
    &X,
    &Y,
    &ITEM_SIZE,
    &ROTATION,
    &FADE_TIME,
    &ORDER,
    &UNLOAD_ON_DISCONNECT,
]);

struct Placement {
    file_name: String,
    x: Option<f64>,
    y: Option<f64>,
    size: Option<f64>,
    rotation: Option<f64>,
    fade_time: Option<f64>,
    order: Option<i64>,
    unload_on_disconnect: bool,
}

fn read_placement(config: &SubActionConfig, args: &ArgStack) -> Result<Placement, ConfigProblem> {
    Ok(Placement {
        file_name: read_non_empty(&ITEM_FILE_NAME, config, args)?,
        x: X.read(config, args)?,
        y: Y.read(config, args)?,
        size: ITEM_SIZE.read(config, args)?,
        rotation: ROTATION.read(config, args)?,
        fade_time: FADE_TIME.read(config, args)?,
        order: ORDER.read(config, args)?,
        unload_on_disconnect: UNLOAD_ON_DISCONNECT.read(config, args)?,
    })
}

pub(crate) struct ItemLoadRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemLoadRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ItemLoadRunner {
    fn id(&self) -> &str {
        sub_action_ids::ITEM_LOAD
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Load Item"
    }

    fn summary(&self) -> &str {
        "Loads an item file into the VTube Studio scene."
    }

    fn search_text(&self) -> &str {
        "vtube item load spawn png image sprite vts"
    }

    fn icon_name(&self) -> &str {
        "image"
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
        let placement = match read_placement(config, ctx.arg_stack) {
            Ok(placement) => placement,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        match self
            .sink
            .load_item(
                &placement.file_name,
                placement.x,
                placement.y,
                placement.size,
                placement.rotation,
                placement.fade_time,
                placement.order,
                placement.unload_on_disconnect,
            )
            .await
        {
            Ok(variant) => {
                let mut stack = ctx.arg_stack.clone();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get(result_keys::INSTANCE_ID) {
                        stack = stack.set(variables::item::INSTANCE_ID.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(result_keys::FILE_NAME) {
                        stack = stack.set(variables::item::FILE_NAME.to_owned(), v.clone());
                    }
                }
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.finish(step_failure::failed(&e)), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_empty_file_name_is_failed_without_calling_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemLoadRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let config = BTreeMap::from([("file_name".to_owned(), Variant::String(String::new()))]);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(!sink.was_called());
    }

    #[tokio::test]
    async fn execute_valid_file_name_surfaces_instance_id() {
        let runner = ItemLoadRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let config = BTreeMap::from([(
            "file_name".to_owned(),
            Variant::String("crown.png".to_owned()),
        )]);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let out = extra.expect("success must surface an arg stack");
        assert_eq!(
            out.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-new-1".to_owned()))
        );
    }

    #[tokio::test]
    async fn execute_failed_outcome_when_sink_errors() {
        let runner = ItemLoadRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let config = BTreeMap::from([(
            "file_name".to_owned(),
            Variant::String("crown.png".to_owned()),
        )]);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(extra.is_none());
    }

    #[test]
    fn validate_config_rejects_missing_file_name() {
        let runner = ItemLoadRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }
}
