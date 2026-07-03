use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreGlobalsSetRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsSetRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsSetRunner {
    fn id(&self) -> &str {
        "core.globals.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Set Global"
    }

    fn summary(&self) -> &str {
        "Set a global variable to a value"
    }

    fn search_text(&self) -> &str {
        "set global variable store write"
    }

    fn icon_name(&self) -> &str {
        "database"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
        cfg.insert("persisted".to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "name",
                label: "Variable Name",
                placeholder: "my_counter",
            },
            FormField::Text {
                key: "value",
                label: "Value",
                placeholder: "42",
            },
            FormField::Toggle {
                key: "persisted",
                label: "Persist across restarts",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.set: name is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let name_template = config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let value_template = config
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let name = super::interpolate::interpolate_with_globals(
            name_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let raw = super::interpolate::interpolate_with_globals(
            value_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let variant = parse_variant(&raw);
        let persisted = config
            .get("persisted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let prev_value = self.globals.get(&name).await.ok().flatten();

        let outcome = match self.globals.set(&name, variant, persisted).await {
            Ok(()) => {
                let mut payload = serde_json::json!({
                    "key": name,
                    "new_value": raw,
                });
                if let Some(prev) = prev_value {
                    payload["prev_value"] = serde_json::Value::String(prev.to_string());
                }
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.set",
                    payload,
                    ctx.parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.set".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

fn parse_variant(s: &str) -> Variant {
    if let Ok(i) = s.parse::<i64>() {
        return Variant::Int(i);
    }
    if let Ok(f) = s.parse::<f64>()
        && let Ok(v) = Variant::float(f)
    {
        return v;
    }
    if s.eq_ignore_ascii_case("true") {
        return Variant::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Variant::Bool(false);
    }
    Variant::String(s.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_variant_infers_type_per_input_format() {
        assert!(matches!(parse_variant("42"), Variant::Int(42)));
        assert!(matches!(parse_variant("-7"), Variant::Int(-7)));
        let f = parse_variant("3.99");
        assert!(matches!(f, Variant::Float(x) if (x - 3.99).abs() < 1e-10));
        assert!(matches!(parse_variant("true"), Variant::Bool(true)));
        assert!(matches!(parse_variant("TRUE"), Variant::Bool(true)));
        assert!(matches!(parse_variant("false"), Variant::Bool(false)));
        assert!(matches!(parse_variant("False"), Variant::Bool(false)));
        assert!(matches!(parse_variant("hello"), Variant::String(s) if s == "hello"));
        assert!(matches!(parse_variant(""), Variant::String(s) if s.is_empty()));
    }
}
