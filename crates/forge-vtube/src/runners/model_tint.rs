use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ModelTintRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelTintRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

fn read_color_channel(config: &SubActionConfig, key: &str) -> i64 {
    match config.get(key) {
        Some(Variant::Int(v)) => (*v).clamp(0, 255),
        _ => 255,
    }
}

fn read_opt_float(config: &SubActionConfig, key: &str) -> Option<f64> {
    match config.get(key) {
        Some(Variant::Float(f)) => Some(*f),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for ModelTintRunner {
    fn id(&self) -> &str {
        "vtube.model.tint"
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("color_r".to_owned(), Variant::Int(255)),
            ("color_g".to_owned(), Variant::Int(255)),
            ("color_b".to_owned(), Variant::Int(255)),
            ("color_a".to_owned(), Variant::Int(255)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Integer {
                key: "color_r",
                label: "Red",
                min: 0,
                max: 255,
            },
            FormField::Integer {
                key: "color_g",
                label: "Green",
                min: 0,
                max: 255,
            },
            FormField::Integer {
                key: "color_b",
                label: "Blue",
                min: 0,
                max: 255,
            },
            FormField::Integer {
                key: "color_a",
                label: "Alpha",
                min: 0,
                max: 255,
            },
            FormField::Optional {
                key: "mix_with_scene_lighting",
                label: "Mix With Scene Lighting",
                inner: Box::new(FormField::Text {
                    key: "mix_with_scene_lighting",
                    label: "Mix With Scene Lighting",
                    placeholder: "0.0 to 1.0",
                }),
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let color_r = read_color_channel(config, "color_r");
        let color_g = read_color_channel(config, "color_g");
        let color_b = read_color_channel(config, "color_b");
        let color_a = read_color_channel(config, "color_a");
        let mix_with_scene_lighting = read_opt_float(config, "mix_with_scene_lighting");

        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .tint_all_art_meshes(color_r, color_g, color_b, color_a, mix_with_scene_lighting)
                .await,
        );

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.model.tint".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

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
    async fn execute_clamps_out_of_range_channel() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelTintRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([("color_r".to_owned(), Variant::Int(999))]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
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
