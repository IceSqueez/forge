use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use time::OffsetDateTime;

pub struct CoreGlobalsToggleRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsToggleRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsToggleRunner {
    fn id(&self) -> &str {
        "core.globals.toggle"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Toggle Global"
    }

    fn summary(&self) -> &str {
        "Flip a boolean global variable between true and false"
    }

    fn search_text(&self) -> &str {
        "toggle global variable bool boolean flip switch"
    }

    fn icon_name(&self) -> &str {
        "toggle"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "key",
            label: "Variable Name",
            placeholder: "my_flag",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("key").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.toggle: key is required".to_owned(),
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

        let resolved_key = super::interpolate::interpolate_with_globals(
            key_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let outcome = match self.globals.get(&resolved_key).await {
            Err(e) => SubActionOutcome::Failed(e.to_string()),
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.globals.toggle: global '{}' does not exist",
                resolved_key
            )),
            Ok(Some(Variant::Bool(b))) => {
                let flipped = Variant::Bool(!b);
                match self.globals.set(&resolved_key, flipped, false).await {
                    Ok(()) => {
                        ctx.publisher.publish(Event::caused_by(
                            EventSource::Core,
                            "global.set",
                            serde_json::json!({
                                "key": resolved_key,
                                "new_value": !b,
                            }),
                            ctx.parent_event_id,
                        ));
                        SubActionOutcome::Success
                    }
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                }
            }
            Ok(Some(other)) => SubActionOutcome::Failed(format!(
                "core.globals.toggle: expected bool, found {}",
                VariantKind::from_variant(&other).label()
            )),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.toggle".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
