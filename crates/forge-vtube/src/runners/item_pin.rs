use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, NumberField, OptionalField, RunContext,
    StepTimer, SubActionCategory, SubActionRunner, TextField, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::runners::choices::{AngleRelativeTo, SizeRelativeTo, VertexPinType};
use crate::runners::fields::{
    ITEM_INSTANCE_ID, ITEM_SIZE, optional_number, optional_text, read_non_empty,
};
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const DEFAULT_PIN_ANGLE: f64 = 0.0;
const DEFAULT_PIN_SIZE: f64 = 0.33;

const PIN: ToggleField = ToggleField {
    key: config_keys::PIN,
    label: "Pin (off to unpin)",
    default: true,
};

const VERTEX_PIN_TYPE: ChoiceField<VertexPinType> = ChoiceField {
    key: config_keys::VERTEX_PIN_TYPE,
    label: "Pin Position",
    default: VertexPinType::Center,
};

const ANGLE_RELATIVE_TO: ChoiceField<AngleRelativeTo> = ChoiceField {
    key: config_keys::ANGLE_RELATIVE_TO,
    label: "Angle Relative To",
    default: AngleRelativeTo::Model,
};

const SIZE_RELATIVE_TO: ChoiceField<SizeRelativeTo> = ChoiceField {
    key: config_keys::SIZE_RELATIVE_TO,
    label: "Size Relative To",
    default: SizeRelativeTo::World,
};

const MODEL_ID: OptionalField<TextField> =
    optional_text(config_keys::MODEL_ID, "Model ID", "empty = current model");

const ART_MESH_ID: OptionalField<TextField> = optional_text(
    config_keys::ART_MESH_ID,
    "Art Mesh ID",
    "empty = random art mesh",
);

const ANGLE: OptionalField<NumberField> = optional_number(config_keys::ANGLE, "Angle", "0");

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &ITEM_INSTANCE_ID,
    &PIN,
    &VERTEX_PIN_TYPE,
    &ANGLE_RELATIVE_TO,
    &SIZE_RELATIVE_TO,
    &MODEL_ID,
    &ART_MESH_ID,
    &ANGLE,
    &ITEM_SIZE,
]);

struct PinRequest {
    item_instance_id: String,
    pin: bool,
    vertex_pin_type: VertexPinType,
    angle_relative_to: AngleRelativeTo,
    size_relative_to: SizeRelativeTo,
    model_id: Option<String>,
    art_mesh_id: Option<String>,
    angle: Option<f64>,
    size: Option<f64>,
}

fn read_request(config: &SubActionConfig, args: &ArgStack) -> Result<PinRequest, ConfigProblem> {
    Ok(PinRequest {
        item_instance_id: read_non_empty(&ITEM_INSTANCE_ID, config, args)?,
        pin: PIN.read(config, args)?,
        vertex_pin_type: VERTEX_PIN_TYPE.read(config, args)?,
        angle_relative_to: ANGLE_RELATIVE_TO.read(config, args)?,
        size_relative_to: SIZE_RELATIVE_TO.read(config, args)?,
        model_id: MODEL_ID.read(config, args)?,
        art_mesh_id: ART_MESH_ID.read(config, args)?,
        angle: ANGLE.read(config, args)?,
        size: ITEM_SIZE.read(config, args)?,
    })
}

pub struct ItemPinRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemPinRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ItemPinRunner {
    fn id(&self) -> &str {
        sub_action_ids::ITEM_PIN
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Pin Item"
    }

    fn summary(&self) -> &str {
        "Pins or unpins a loaded VTube Studio item to the current model."
    }

    fn search_text(&self) -> &str {
        "vtube item pin attach model art mesh vts"
    }

    fn icon_name(&self) -> &str {
        "pin"
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
        let request = match read_request(config, ctx.arg_stack) {
            Ok(request) => request,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .pin_item(
                    &request.item_instance_id,
                    request.pin,
                    request.angle_relative_to.wire_name(),
                    request.size_relative_to.wire_name(),
                    request.vertex_pin_type.wire_name(),
                    request.model_id.as_deref().unwrap_or_default(),
                    request.art_mesh_id.as_deref().unwrap_or_default(),
                    request.angle.unwrap_or(DEFAULT_PIN_ANGLE),
                    request.size.unwrap_or(DEFAULT_PIN_SIZE),
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

    fn config_with_id(id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "item_instance_id".to_owned(),
            Variant::String(id.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_empty_id_is_failed_without_calling_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemPinRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&runner.default_config(), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(!sink.was_called());
    }

    #[tokio::test]
    async fn execute_valid_id_pins_via_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemPinRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config_with_id("item-1"), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_unpin_still_calls_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemPinRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("pin".to_owned(), Variant::Bool(false));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_failed_outcome_when_sink_errors() {
        let sink = Arc::new(MockSink::failing());
        let runner = ItemPinRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config_with_id("item-1"), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
    }

    #[test]
    fn validate_config_rejects_missing_id() {
        let runner = ItemPinRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }
}
