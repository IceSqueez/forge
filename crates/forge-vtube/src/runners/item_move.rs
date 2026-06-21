use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ItemMoveRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemMoveRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

const FADE_MODES: &[&str] = &[
    "linear",
    "easeIn",
    "easeOut",
    "easeBoth",
    "overshoot",
    "zip",
];

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
impl SubActionRunner for ItemMoveRunner {
    fn id(&self) -> &str {
        "vtube.item.move"
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "item_instance_id".to_owned(),
                Variant::String(String::new()),
            ),
            ("fade_mode".to_owned(), Variant::String("linear".to_owned())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "item_instance_id",
                label: "Item Instance ID",
                placeholder: "item instance id from VTS",
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
                key: "order",
                label: "Order",
                inner: Box::new(FormField::Integer {
                    key: "order",
                    label: "Order",
                    min: -1000,
                    max: 1000,
                }),
            },
            FormField::Optional {
                key: "duration",
                label: "Duration (s)",
                inner: Box::new(FormField::Text {
                    key: "duration",
                    label: "Duration (s)",
                    placeholder: "0.5",
                }),
            },
            FormField::Select {
                key: "fade_mode",
                label: "Fade Mode",
                options: FADE_MODES,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("item_instance_id") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "vtube.item.move: 'item_instance_id' must be a string".to_owned(),
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

        let raw_id = config
            .get("item_instance_id")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let item_instance_id = ctx.arg_stack.interpolate(raw_id);

        let x = read_opt_float(config, "x");
        let y = read_opt_float(config, "y");
        let size = read_opt_float(config, "size");
        let rotation = read_opt_float(config, "rotation");
        let order = read_opt_int(config, "order");
        let duration = read_opt_float(config, "duration");
        let fade_mode = match config.get("fade_mode") {
            Some(Variant::String(s)) if FADE_MODES.contains(&s.as_str()) => s.as_str(),
            _ => "linear",
        };

        let outcome = if item_instance_id.is_empty() {
            SubActionOutcome::Failed("vtube.item.move: item_instance_id is empty".to_owned())
        } else if x.is_none()
            && y.is_none()
            && size.is_none()
            && rotation.is_none()
            && order.is_none()
        {
            SubActionOutcome::Success
        } else {
            let time_in_seconds = duration.unwrap_or(0.0);
            match self
                .sink
                .move_item(
                    &item_instance_id,
                    x,
                    y,
                    size,
                    rotation,
                    order,
                    time_in_seconds,
                    fade_mode,
                )
                .await
            {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        };

        (
            SubActionTelemetry {
                kind: "vtube.item.move".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
