use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, IntegerEditor, IntegerField, NumberField, OptionalField, RunContext,
    StepTimer, SubActionCategory, SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::runners::fields::optional_number;
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const CHANNEL_MAX: i64 = 255;

const CHANNEL_BOUNDS: ValueBounds = ValueBounds {
    min: Some(0),
    max: Some(CHANNEL_MAX),
};

const COLOR_R: IntegerField = IntegerField {
    key: config_keys::COLOR_R,
    label: "Red",
    editor: IntegerEditor::Number,
    default: CHANNEL_MAX,
    bounds: CHANNEL_BOUNDS,
};

const COLOR_G: IntegerField = IntegerField {
    key: config_keys::COLOR_G,
    label: "Green",
    editor: IntegerEditor::Number,
    default: CHANNEL_MAX,
    bounds: CHANNEL_BOUNDS,
};

const COLOR_B: IntegerField = IntegerField {
    key: config_keys::COLOR_B,
    label: "Blue",
    editor: IntegerEditor::Number,
    default: CHANNEL_MAX,
    bounds: CHANNEL_BOUNDS,
};

const COLOR_A: IntegerField = IntegerField {
    key: config_keys::COLOR_A,
    label: "Alpha",
    editor: IntegerEditor::Number,
    default: CHANNEL_MAX,
    bounds: CHANNEL_BOUNDS,
};

const MIX_WITH_SCENE_LIGHTING: OptionalField<NumberField> = optional_number(
    config_keys::MIX_WITH_SCENE_LIGHTING,
    "Mix With Scene Lighting",
    "0.0 to 1.0",
);

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &COLOR_R,
    &COLOR_G,
    &COLOR_B,
    &COLOR_A,
    &MIX_WITH_SCENE_LIGHTING,
]);

struct Tint {
    r: i64,
    g: i64,
    b: i64,
    a: i64,
    mix_with_scene_lighting: Option<f64>,
}

fn read_tint(config: &SubActionConfig, args: &ArgStack) -> Result<Tint, ConfigProblem> {
    Ok(Tint {
        r: COLOR_R.read(config, args)?,
        g: COLOR_G.read(config, args)?,
        b: COLOR_B.read(config, args)?,
        a: COLOR_A.read(config, args)?,
        mix_with_scene_lighting: MIX_WITH_SCENE_LIGHTING.read(config, args)?,
    })
}

pub struct ModelTintRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelTintRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ModelTintRunner {
    fn id(&self) -> &str {
        sub_action_ids::MODEL_TINT
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Tint Model"
    }

    fn summary(&self) -> &str {
        "Applies a color tint over every ArtMesh on the VTube Studio model."
    }

    fn search_text(&self) -> &str {
        "vtube model tint color overlay art mesh vts"
    }

    fn icon_name(&self) -> &str {
        "droplet"
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
        let tint = match read_tint(config, ctx.arg_stack) {
            Ok(tint) => tint,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .tint_all_art_meshes(tint.r, tint.g, tint.b, tint.a, tint.mix_with_scene_lighting)
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, RecordingSink, make_ctx};
    use forge_types::Variant;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_default_config_succeeds() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelTintRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&runner.default_config(), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn channels_at_the_byte_range_ends_reach_vts_unchanged() {
        let sink = Arc::new(RecordingSink::default());
        let runner = ModelTintRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([
            ("color_r".to_owned(), Variant::Int(0)),
            ("color_g".to_owned(), Variant::String("255".to_owned())),
            ("color_b".to_owned(), Variant::Int(255)),
            ("color_a".to_owned(), Variant::String("0".to_owned())),
        ]);
        let stack = ArgStack::new();

        runner.execute(&config, &make_ctx(&stack)).await;

        assert_eq!(
            sink.numbers_sent_to("tint_all_art_meshes")
                .map(|args| args[..4].to_vec()),
            Some(vec![Some(0.0), Some(255.0), Some(255.0), Some(0.0)])
        );
    }

    #[tokio::test]
    async fn a_channel_one_past_the_byte_range_fails_without_tinting() {
        for (key, value) in [
            ("color_r", Variant::Int(256)),
            ("color_g", Variant::String("-1".to_owned())),
            ("color_b", Variant::Int(-1)),
            ("color_a", Variant::String("256".to_owned())),
        ] {
            let sink = Arc::new(RecordingSink::default());
            let runner = ModelTintRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
            let config = BTreeMap::from([(key.to_owned(), value.clone())]);
            let stack = ArgStack::new();

            let (tel, _) = runner.execute(&config, &make_ctx(&stack)).await;

            assert!(
                matches!(&tel.outcome, SubActionOutcome::Failed(reason)
                    if reason.code_or_text().starts_with(&format!("invalid step config: '{key}' "))),
                "{key} = {value:?} ran as {:?}",
                tel.outcome,
            );
            assert!(!sink.was_called(), "{key} = {value:?} reached VTS");
        }
    }

    #[tokio::test]
    async fn a_blank_or_boolean_scene_lighting_mix_is_left_out() {
        let sink = Arc::new(RecordingSink::default());
        let runner = ModelTintRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let blank = BTreeMap::from([(
            "mix_with_scene_lighting".to_owned(),
            Variant::String(" ".to_owned()),
        )]);
        let flag = BTreeMap::from([("mix_with_scene_lighting".to_owned(), Variant::Bool(true))]);

        let (blank_tel, _) = runner.execute(&blank, &make_ctx(&stack)).await;
        let sent = sink.numbers_sent_to("tint_all_art_meshes");
        let (flag_tel, _) = runner.execute(&flag, &make_ctx(&stack)).await;

        assert_eq!(blank_tel.outcome, SubActionOutcome::Success);
        assert_eq!(sent.map(|args| args[4]), Some(None));
        assert_eq!(flag_tel.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn execute_propagates_sink_error() {
        let runner = ModelTintRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
    }
}
