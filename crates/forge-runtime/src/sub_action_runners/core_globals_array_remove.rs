use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use time::OffsetDateTime;

pub struct CoreGlobalsArrayRemoveRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsArrayRemoveRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsArrayRemoveRunner {
    fn id(&self) -> &str {
        "core.globals.array_remove"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Remove from Global Array"
    }

    fn summary(&self) -> &str {
        "Remove matching items from a global array variable"
    }

    fn search_text(&self) -> &str {
        "remove delete array global list item filter"
    }

    fn icon_name(&self) -> &str {
        "list-minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
        cfg.insert("remove_all".to_owned(), Variant::Bool(false));
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
                label: "Value to Remove",
                placeholder: "item",
            },
            FormField::Toggle {
                key: "remove_all",
                label: "Remove All Matches",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("key").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.array_remove: key is required".to_owned(),
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
        let remove_all = config
            .get("remove_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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
        let target = super::interpolate::parse_variant(&raw_value);

        let outcome = match self.globals.get(&resolved_key).await {
            Err(e) => SubActionOutcome::Failed(e.to_string()),
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.globals.array_remove: global '{}' does not exist",
                resolved_key
            )),
            Ok(Some(Variant::Array(mut arr))) => {
                if remove_all {
                    arr.retain(|item| item != &target);
                } else {
                    // Remove only the first matching element (leftmost).
                    if let Some(pos) = arr.iter().position(|item| item == &target) {
                        arr.remove(pos);
                    }
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
            Ok(Some(other)) => SubActionOutcome::Failed(format!(
                "core.globals.array_remove: expected array, found {}",
                VariantKind::from_variant(&other).label()
            )),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.array_remove".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
