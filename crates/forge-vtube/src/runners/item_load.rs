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

pub struct ItemLoadRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemLoadRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

fn read_opt_float(config: &SubActionConfig, key: &str) -> Option<f64> {
    match config.get(key) {
        Some(Variant::Float(f)) => Some(*f),
        _ => None,
    }
}

fn read_opt_int(config: &SubActionConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(i)) => Some(*i),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for ItemLoadRunner {
    fn id(&self) -> &str {
        "vtube.item.load"
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("file_name".to_owned(), Variant::String(String::new())),
            ("unload_on_disconnect".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "file_name",
                label: "Item File Name",
                placeholder: "my_item.png",
            },
            FormField::Optional {
                key: "x",
                label: "X Position",
                inner: Box::new(FormField::Text {
                    key: "x",
                    label: "X Position",
                    placeholder: "-1.0 to 1.0",
                }),
            },
            FormField::Optional {
                key: "y",
                label: "Y Position",
                inner: Box::new(FormField::Text {
                    key: "y",
                    label: "Y Position",
                    placeholder: "-1.0 to 1.0",
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
            FormField::Optional {
                key: "rotation",
                label: "Rotation",
                inner: Box::new(FormField::Text {
                    key: "rotation",
                    label: "Rotation",
                    placeholder: "-360 to 360",
                }),
            },
            FormField::Optional {
                key: "fade_time",
                label: "Fade Time (s)",
                inner: Box::new(FormField::Text {
                    key: "fade_time",
                    label: "Fade Time (s)",
                    placeholder: "0.5",
                }),
            },
            FormField::Optional {
                key: "order",
                label: "Order",
                inner: Box::new(FormField::Integer {
                    key: "order",
                    label: "Order",
                    min: 0,
                    max: 1000,
                }),
            },
            FormField::Toggle {
                key: "unload_on_disconnect",
                label: "Unload When Plugin Disconnects",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("file_name") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "vtube.item.load: 'file_name' must be a string".to_owned(),
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

        let raw_name = config.str("file_name").unwrap_or_default();
        let file_name = ctx.arg_stack.interpolate(raw_name);

        if file_name.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: "vtube.item.load".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "vtube.item.load: file_name is empty".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        let x = read_opt_float(config, "x");
        let y = read_opt_float(config, "y");
        let size = read_opt_float(config, "size");
        let rotation = read_opt_float(config, "rotation");
        let fade_time = read_opt_float(config, "fade_time");
        let order = read_opt_int(config, "order");
        let unload_on_disconnect = !matches!(
            config.get("unload_on_disconnect"),
            Some(Variant::Bool(false))
        );

        match self
            .sink
            .load_item(
                &file_name,
                x,
                y,
                size,
                rotation,
                fade_time,
                order,
                unload_on_disconnect,
            )
            .await
        {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get("instance_id") {
                        stack = stack.set("vtube.item.instance_id".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("file_name") {
                        stack = stack.set("vtube.item.file_name".to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: "vtube.item.load".to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(stack),
                )
            }
            Err(e) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: "vtube.item.load".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(e.to_string()),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

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
