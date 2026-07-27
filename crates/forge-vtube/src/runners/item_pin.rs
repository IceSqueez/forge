use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ItemPinRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemPinRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

const ANGLE_RELATIVE_TO_OPTIONS: &[&str] = &[
    "RelativeToWorld",
    "RelativeToCurrentItemRotation",
    "RelativeToModel",
    "RelativeToPinPosition",
];

const SIZE_RELATIVE_TO_OPTIONS: &[&str] = &["RelativeToWorld", "RelativeToCurrentItemSize"];

const VERTEX_PIN_TYPE_OPTIONS: &[&str] = &["Center", "Random"];

fn read_opt_float(config: &SubActionConfig, key: &str) -> Option<f64> {
    match config.get(key) {
        Some(Variant::Float(f)) => Some(*f),
        _ => None,
    }
}

fn select_or_default<'a>(
    config: &'a SubActionConfig,
    key: &str,
    options: &'a [&str],
    default: &'a str,
) -> &'a str {
    match config.get(key) {
        Some(Variant::String(s)) if options.contains(&s.as_str()) => s.as_str(),
        _ => default,
    }
}

#[async_trait]
impl SubActionRunner for ItemPinRunner {
    fn id(&self) -> &str {
        "vtube.item.pin"
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "item_instance_id".to_owned(),
                Variant::String(String::new()),
            ),
            ("pin".to_owned(), Variant::Bool(true)),
            (
                "angle_relative_to".to_owned(),
                Variant::String("RelativeToModel".to_owned()),
            ),
            (
                "size_relative_to".to_owned(),
                Variant::String("RelativeToWorld".to_owned()),
            ),
            (
                "vertex_pin_type".to_owned(),
                Variant::String("Center".to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "item_instance_id",
                label: "Item Instance ID",
                placeholder: "item instance id from VTS",
            },
            FormField::Toggle {
                key: "pin",
                label: "Pin (off to unpin)",
            },
            FormField::Select {
                key: "vertex_pin_type",
                label: "Pin Position",
                options: VERTEX_PIN_TYPE_OPTIONS,
            },
            FormField::Select {
                key: "angle_relative_to",
                label: "Angle Relative To",
                options: ANGLE_RELATIVE_TO_OPTIONS,
            },
            FormField::Select {
                key: "size_relative_to",
                label: "Size Relative To",
                options: SIZE_RELATIVE_TO_OPTIONS,
            },
            FormField::Optional {
                key: "model_id",
                label: "Model ID",
                inner: Box::new(FormField::Text {
                    key: "model_id",
                    label: "Model ID",
                    placeholder: "empty = current model",
                }),
            },
            FormField::Optional {
                key: "art_mesh_id",
                label: "Art Mesh ID",
                inner: Box::new(FormField::Text {
                    key: "art_mesh_id",
                    label: "Art Mesh ID",
                    placeholder: "empty = random art mesh",
                }),
            },
            FormField::Optional {
                key: "angle",
                label: "Angle",
                inner: Box::new(FormField::Text {
                    key: "angle",
                    label: "Angle",
                    placeholder: "0",
                }),
            },
            FormField::Optional {
                key: "size",
                label: "Size",
                inner: Box::new(FormField::Text {
                    key: "size",
                    label: "Size",
                    placeholder: "0.0 to 1.0",
                }),
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("item_instance_id") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "vtube.item.pin: 'item_instance_id' must be a string".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_id = config.str("item_instance_id").unwrap_or_default();
        let item_instance_id = ctx.arg_stack.interpolate(raw_id);

        let pin = !matches!(config.get("pin"), Some(Variant::Bool(false)));
        let angle_relative_to = select_or_default(
            config,
            "angle_relative_to",
            ANGLE_RELATIVE_TO_OPTIONS,
            "RelativeToModel",
        );
        let size_relative_to = select_or_default(
            config,
            "size_relative_to",
            SIZE_RELATIVE_TO_OPTIONS,
            "RelativeToWorld",
        );
        let vertex_pin_type =
            select_or_default(config, "vertex_pin_type", VERTEX_PIN_TYPE_OPTIONS, "Center");
        let model_id = config.str("model_id").unwrap_or_default();
        let art_mesh_id = config.str("art_mesh_id").unwrap_or_default();
        let angle = read_opt_float(config, "angle").unwrap_or(0.0);
        let size = read_opt_float(config, "size").unwrap_or(0.33);

        let outcome = if item_instance_id.is_empty() {
            SubActionOutcome::Failed("vtube.item.pin: item_instance_id is empty".to_owned())
        } else {
            SubActionOutcome::from_result(
                &self
                    .sink
                    .pin_item(
                        &item_instance_id,
                        pin,
                        angle_relative_to,
                        size_relative_to,
                        vertex_pin_type,
                        model_id,
                        art_mesh_id,
                        angle,
                        size,
                    )
                    .await,
            )
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.item.pin".to_owned(),
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
