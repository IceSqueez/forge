use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct SetMuteRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetMuteRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetMuteRunner {
    fn id(&self) -> &str {
        "obs.audio.set_mute"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Input Mute"
    }

    fn summary(&self) -> &str {
        "Mutes or unmutes an OBS audio input."
    }

    fn search_text(&self) -> &str {
        "obs audio mute unmute input source volume"
    }

    fn icon_name(&self) -> &str {
        "volume"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String(String::new())),
            ("muted".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "source",
                label: "Input",
                options_key: "obs.input_names",
            },
            FormField::Toggle {
                key: "muted",
                label: "Muted",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("source") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "obs.audio.set_mute: 'source' must be a string".to_owned(),
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
            .get("source")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw);
        let muted = matches!(config.get("muted"), Some(Variant::Bool(true)));

        let outcome = match self.sink.set_input_mute(&source, muted).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.audio.set_mute".to_owned(),
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
    use crate::runners::test_support::MockSink;

    #[test]
    fn validate_config_accepts_valid_source() {
        let runner = SetMuteRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([
            (
                "source".to_owned(),
                Variant::String("Microphone".to_owned()),
            ),
            ("muted".to_owned(), Variant::Bool(true)),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_source() {
        let runner = SetMuteRunner::new(Arc::new(MockSink));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }
}
