use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ExpressionSetRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ExpressionSetRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ExpressionSetRunner {
    fn id(&self) -> &str {
        "vtube.expression.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Set Expression"
    }

    fn summary(&self) -> &str {
        "Activates or deactivates a VTube Studio expression."
    }

    fn search_text(&self) -> &str {
        "vtube expression activate toggle face vts"
    }

    fn icon_name(&self) -> &str {
        "mood-smile"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("expression_file".to_owned(), Variant::String(String::new())),
            ("active".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "expression_file",
                label: "Expression",
                options_key: "vtube.expression_files",
            },
            FormField::Toggle {
                key: "active",
                label: "Active",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("expression_file") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "vtube.expression.set: 'expression_file' must be a string".to_owned(),
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

        let raw = config
            .get("expression_file")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let expression_file = ctx.arg_stack.interpolate(raw);
        let active = matches!(config.get("active"), Some(Variant::Bool(true)));

        let outcome = match self.sink.set_expression(&expression_file, active).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "vtube.expression.set".to_owned(),
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

    #[test]
    fn validate_config_accepts_expression_string() {
        let runner = ExpressionSetRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([
            (
                "expression_file".to_owned(),
                Variant::String("smile.exp3.json".to_owned()),
            ),
            ("active".to_owned(), Variant::Bool(true)),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_expression_file() {
        let runner = ExpressionSetRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[tokio::test]
    async fn execute_interpolates_expression_file() {
        let runner = ExpressionSetRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new().set(
            "expr".to_owned(),
            Variant::String("blink.exp3.json".to_owned()),
        );
        let config = BTreeMap::from([
            (
                "expression_file".to_owned(),
                Variant::String("%expr%".to_owned()),
            ),
            ("active".to_owned(), Variant::Bool(true)),
        ]);
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
    }

    #[tokio::test]
    async fn execute_returns_success_on_mock_sink() {
        let runner = ExpressionSetRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let config = BTreeMap::from([
            (
                "expression_file".to_owned(),
                Variant::String("smile.exp3.json".to_owned()),
            ),
            ("active".to_owned(), Variant::Bool(false)),
        ]);
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "vtube.expression.set");
    }
}
