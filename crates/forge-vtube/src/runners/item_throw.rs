use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, NumberField, OptionalField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};

use crate::runners::fields::{ITEM_FILE_NAME, ITEM_SIZE, optional_number, read_non_empty};
use crate::sink::{VTubeSink, result_keys};
use crate::vocabulary::{config_keys, sub_action_ids, variables};

const DEFAULT_THROW_SECONDS: f64 = 0.4;
const SPAWN_FADE_SECONDS: f64 = 0.0;
const LINEAR_FADE: &str = "linear";
const MISSING_INSTANCE_ID: &str = "VTS did not return an item instance id";

const FROM_X: OptionalField<NumberField> =
    optional_number(config_keys::FROM_X, "From X", "-1.0 to 1.0");

const FROM_Y: OptionalField<NumberField> =
    optional_number(config_keys::FROM_Y, "From Y", "-1.0 to 1.0");

const TO_X: OptionalField<NumberField> = optional_number(config_keys::TO_X, "To X", "-1.0 to 1.0");

const TO_Y: OptionalField<NumberField> = optional_number(config_keys::TO_Y, "To Y", "-1.0 to 1.0");

const THROW_DURATION: OptionalField<NumberField> =
    optional_number(config_keys::DURATION, "Throw Duration (s)", "0.4");

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &ITEM_FILE_NAME,
    &FROM_X,
    &FROM_Y,
    &TO_X,
    &TO_Y,
    &ITEM_SIZE,
    &THROW_DURATION,
]);

struct Trajectory {
    file_name: String,
    from_x: Option<f64>,
    from_y: Option<f64>,
    to_x: Option<f64>,
    to_y: Option<f64>,
    size: Option<f64>,
    duration: Option<f64>,
}

fn read_trajectory(config: &SubActionConfig, args: &ArgStack) -> Result<Trajectory, ConfigProblem> {
    Ok(Trajectory {
        file_name: read_non_empty(&ITEM_FILE_NAME, config, args)?,
        from_x: FROM_X.read(config, args)?,
        from_y: FROM_Y.read(config, args)?,
        to_x: TO_X.read(config, args)?,
        to_y: TO_Y.read(config, args)?,
        size: ITEM_SIZE.read(config, args)?,
        duration: THROW_DURATION.read(config, args)?,
    })
}

pub struct ItemThrowRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemThrowRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ItemThrowRunner {
    fn id(&self) -> &str {
        sub_action_ids::ITEM_THROW
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Throw Item"
    }

    fn summary(&self) -> &str {
        "Spawns an item and animates it moving across the VTube Studio scene."
    }

    fn search_text(&self) -> &str {
        "vtube item throw spawn fling toss animate vts"
    }

    fn icon_name(&self) -> &str {
        "wind"
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
        let trajectory = match read_trajectory(config, ctx.arg_stack) {
            Ok(trajectory) => trajectory,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let loaded = self
            .sink
            .load_item(
                &trajectory.file_name,
                trajectory.from_x,
                trajectory.from_y,
                trajectory.size,
                None,
                Some(SPAWN_FADE_SECONDS),
                None,
                true,
            )
            .await;

        let instance_id = match loaded {
            Ok(Variant::Object(ref map)) => match map.get(result_keys::INSTANCE_ID) {
                Some(Variant::String(id)) if !id.is_empty() => id.clone(),
                _ => return (timer.failed(MISSING_INSTANCE_ID), None),
            },
            Ok(_) => return (timer.failed(MISSING_INSTANCE_ID), None),
            Err(e) => return (timer.failed(e.to_string()), None),
        };

        let moved = self
            .sink
            .move_item(
                &instance_id,
                trajectory.to_x,
                trajectory.to_y,
                None,
                None,
                None,
                trajectory.duration.unwrap_or(DEFAULT_THROW_SECONDS),
                LINEAR_FADE,
            )
            .await;

        let stack = ctx.arg_stack.clone().set(
            variables::item::INSTANCE_ID.to_owned(),
            Variant::String(instance_id),
        );

        (
            timer.finish(SubActionOutcome::from_result(&moved)),
            Some(stack),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_empty_file_name_is_failed_without_calling_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemThrowRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let config = BTreeMap::from([("file_name".to_owned(), Variant::String(String::new()))]);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(!sink.was_called());
    }

    #[tokio::test]
    async fn execute_valid_file_name_loads_then_moves() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemThrowRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let config = BTreeMap::from([(
            "file_name".to_owned(),
            Variant::String("crown.png".to_owned()),
        )]);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let out = extra.expect("success must surface the spawned item id");
        assert_eq!(
            out.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-new-1".to_owned()))
        );
    }

    #[tokio::test]
    async fn a_failed_move_still_surfaces_the_id_of_the_item_it_left_in_the_scene() {
        let runner = ItemThrowRunner::new(Arc::new(MockSink::failing_item_move()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let config = BTreeMap::from([(
            "file_name".to_owned(),
            Variant::String("crown.png".to_owned()),
        )]);

        let (tel, extra) = runner.execute(&config, &ctx).await;

        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        let out = extra.expect("a failed move must still hand back the spawned item id");
        assert_eq!(
            out.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-new-1".to_owned()))
        );
    }

    #[tokio::test]
    async fn execute_failed_outcome_when_load_fails() {
        let runner = ItemThrowRunner::new(Arc::new(MockSink::failing()));
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
}
