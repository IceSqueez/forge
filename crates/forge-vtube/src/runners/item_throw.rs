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

pub struct ItemThrowRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemThrowRunner {
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

#[async_trait]
impl SubActionRunner for ItemThrowRunner {
    fn id(&self) -> &str {
        "vtube.item.throw"
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("file_name".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "file_name",
                label: "Item File Name",
                placeholder: "my_item.png",
            },
            FormField::Optional {
                key: "from_x",
                label: "From X",
                inner: Box::new(FormField::Text {
                    key: "from_x",
                    label: "From X",
                    placeholder: "-1.0 to 1.0",
                }),
            },
            FormField::Optional {
                key: "from_y",
                label: "From Y",
                inner: Box::new(FormField::Text {
                    key: "from_y",
                    label: "From Y",
                    placeholder: "-1.0 to 1.0",
                }),
            },
            FormField::Optional {
                key: "to_x",
                label: "To X",
                inner: Box::new(FormField::Text {
                    key: "to_x",
                    label: "To X",
                    placeholder: "-1.0 to 1.0",
                }),
            },
            FormField::Optional {
                key: "to_y",
                label: "To Y",
                inner: Box::new(FormField::Text {
                    key: "to_y",
                    label: "To Y",
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
                key: "duration",
                label: "Throw Duration (s)",
                inner: Box::new(FormField::Text {
                    key: "duration",
                    label: "Throw Duration (s)",
                    placeholder: "0.4",
                }),
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("file_name") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "vtube.item.throw: 'file_name' must be a string".to_owned(),
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
                telemetry(
                    started_at,
                    start,
                    ctx.index,
                    SubActionOutcome::Failed("vtube.item.throw: file_name is empty".to_owned()),
                ),
                None,
            );
        }

        let from_x = read_opt_float(config, "from_x");
        let from_y = read_opt_float(config, "from_y");
        let to_x = read_opt_float(config, "to_x");
        let to_y = read_opt_float(config, "to_y");
        let size = read_opt_float(config, "size");
        let duration = read_opt_float(config, "duration").unwrap_or(0.4);

        let load_result = self
            .sink
            .load_item(
                &file_name,
                from_x,
                from_y,
                size,
                None,
                Some(0.0),
                None,
                true,
            )
            .await;

        let instance_id = match load_result {
            Ok(Variant::Object(ref map)) => match map.get("instance_id") {
                Some(Variant::String(id)) if !id.is_empty() => id.clone(),
                _ => {
                    return (
                        telemetry(
                            started_at,
                            start,
                            ctx.index,
                            SubActionOutcome::Failed(
                                "vtube.item.throw: VTS did not return an item instance id"
                                    .to_owned(),
                            ),
                        ),
                        None,
                    );
                }
            },
            Ok(_) => {
                return (
                    telemetry(
                        started_at,
                        start,
                        ctx.index,
                        SubActionOutcome::Failed(
                            "vtube.item.throw: VTS did not return an item instance id".to_owned(),
                        ),
                    ),
                    None,
                );
            }
            Err(e) => {
                return (
                    telemetry(
                        started_at,
                        start,
                        ctx.index,
                        SubActionOutcome::Failed(e.to_string()),
                    ),
                    None,
                );
            }
        };

        let move_result = self
            .sink
            .move_item(
                &instance_id,
                to_x,
                to_y,
                None,
                None,
                None,
                duration,
                "linear",
            )
            .await;

        let mut stack = ArgStack::new();
        stack = stack.set(
            "vtube.item.instance_id".to_owned(),
            Variant::String(instance_id),
        );

        (
            telemetry(
                started_at,
                start,
                ctx.index,
                SubActionOutcome::from_result(&move_result),
            ),
            Some(stack),
        )
    }
}

fn telemetry(
    started_at: OffsetDateTime,
    start: Instant,
    index: usize,
    outcome: SubActionOutcome,
) -> SubActionTelemetry {
    SubActionTelemetry {
        args_in: ::std::collections::BTreeMap::new(),
        produced: ::std::collections::BTreeMap::new(),
        kind: "vtube.item.throw".to_owned(),
        started_at,
        duration_ms: start.elapsed().as_millis() as u64,
        outcome,
        index,
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
