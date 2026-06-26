use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use time::OffsetDateTime;

pub struct CoreGlobalsArrayAppendRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsArrayAppendRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsArrayAppendRunner {
    fn id(&self) -> &str {
        "core.globals.array_append"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Append to Global Array"
    }

    fn summary(&self) -> &str {
        "Append a value to a global array variable"
    }

    fn search_text(&self) -> &str {
        "append push array global list add item"
    }

    fn icon_name(&self) -> &str {
        "list-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
        cfg.insert("max_length".to_owned(), Variant::Int(0));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "key",
                label: "Variable Name",
                placeholder: "my_list",
            },
            FormField::Text {
                key: "value",
                label: "Value to Append",
                placeholder: "item",
            },
            FormField::Integer {
                key: "max_length",
                label: "Max Length (0 = unbounded)",
                min: 0,
                max: i64::MAX,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("key").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.array_append: key is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let key_template = config
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let value_template = config
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let max_len = config
            .get("max_length")
            .and_then(|v| v.as_int())
            .unwrap_or(0);

        let resolved_key = super::interpolate::interpolate_with_globals(
            key_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let raw_value = super::interpolate::interpolate_with_globals(
            value_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let item = super::interpolate::parse_variant(&raw_value);

        let outcome = match self.globals.get(&resolved_key).await {
            Err(e) => SubActionOutcome::Failed(e.to_string()),
            Ok(current) => {
                let array_result: Result<Vec<Variant>, String> = match current {
                    None => Ok(Vec::new()),
                    Some(Variant::Array(a)) => Ok(a),
                    Some(other) => Err(format!(
                        "core.globals.array_append: expected array, found {}",
                        VariantKind::from_variant(&other).label()
                    )),
                };
                match array_result {
                    Err(msg) => SubActionOutcome::Failed(msg),
                    Ok(mut arr) => {
                        arr.push(item);

                        // When bounded, drop oldest items from the front to stay within max_len.
                        if max_len > 0 && arr.len() as i64 > max_len {
                            let to_drain = (arr.len() as i64 - max_len) as usize;
                            arr.drain(0..to_drain);
                        }

                        let new_len = arr.len();
                        match self
                            .globals
                            .set(&resolved_key, Variant::Array(arr), false)
                            .await
                        {
                            Ok(()) => {
                                ctx.publisher.publish(Event::caused_by(
                                    EventSource::Core,
                                    "global.set",
                                    serde_json::json!({
                                        "key": resolved_key,
                                        "new_length": new_len,
                                    }),
                                    ctx.parent_event_id,
                                ));
                                SubActionOutcome::Success
                            }
                            Err(e) => SubActionOutcome::Failed(e.to_string()),
                        }
                    }
                }
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.array_append".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
