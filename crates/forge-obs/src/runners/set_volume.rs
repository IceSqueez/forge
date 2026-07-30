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

use crate::ObsSink;

pub struct SetVolumeRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetVolumeRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetVolumeRunner {
    fn id(&self) -> &str {
        "obs.audio.set_volume"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Source Volume"
    }

    fn summary(&self) -> &str {
        "Sets the volume of an audio input in dB."
    }

    fn search_text(&self) -> &str {
        "obs audio volume input set db decibel source"
    }

    fn icon_name(&self) -> &str {
        "volume-2"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String(String::new())),
            ("volume_db".to_owned(), Variant::String("0.0".to_owned())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "source",
                label: "Audio Input",
                options_key: "obs.audio_inputs",
            },
            FormField::Text {
                key: "volume_db",
                label: "Volume (dB)",
                placeholder: "e.g. -6.0",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let source_ok =
            matches!(config.get("source"), Some(Variant::String(s)) if !s.trim().is_empty());
        if !source_ok {
            return Err(RegistryError::InvalidConfig(
                "obs.audio.set_volume: 'source' must not be empty".to_owned(),
            ));
        }
        let db_ok = config.get("volume_db").is_some_and(|v| match v {
            Variant::String(s) => s.parse::<f64>().is_ok(),
            Variant::Float(_) | Variant::Int(_) => true,
            _ => false,
        });
        if !db_ok {
            return Err(RegistryError::InvalidConfig(
                "obs.audio.set_volume: 'volume_db' must be a valid number".to_owned(),
            ));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_source = config.str("source").unwrap_or_default();

        let source = ctx.arg_stack.interpolate(raw_source);

        let db = config
            .get("volume_db")
            .and_then(|v| match v {
                Variant::String(s) => s.parse::<f64>().ok(),
                Variant::Float(f) => Some(*f),
                Variant::Int(i) => Some(*i as f64),
                _ => None,
            })
            .unwrap_or(0.0);

        let outcome =
            SubActionOutcome::from_result(&self.sink.set_input_volume_db(&source, db).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.audio.set_volume".to_owned(),
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

    fn runner() -> SetVolumeRunner {
        SetVolumeRunner::new(Arc::new(MockSink))
    }

    fn with_db(db: Option<Variant>) -> SubActionConfig {
        let mut config = BTreeMap::from([("source".to_owned(), Variant::String("Mic".to_owned()))]);
        if let Some(db) = db {
            config.insert("volume_db".to_owned(), db);
        }
        config
    }

    /// The UI stores an interpolated `%arg%` as a string, so a decibel figure reaches the runner
    /// either already numeric or still spelled out.
    #[test]
    fn validate_config_accepts_a_decibel_figure_in_any_numeric_shape() {
        for db in [
            Variant::String("-6.0".to_owned()),
            Variant::Float(-3.0),
            Variant::Int(0),
        ] {
            assert!(
                runner().validate_config(&with_db(Some(db.clone()))).is_ok(),
                "rejected volume_db = {db:?}",
            );
        }
    }

    #[test]
    fn validate_config_rejects_a_decibel_figure_that_is_missing_or_unparseable() {
        for db in [
            None,
            Some(Variant::String("loud".to_owned())),
            Some(Variant::String(String::new())),
            Some(Variant::Bool(true)),
        ] {
            assert!(
                runner().validate_config(&with_db(db.clone())).is_err(),
                "accepted volume_db = {db:?}",
            );
        }
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("Mic".to_owned())),
            ("volume_db".to_owned(), Variant::String("-6.0".to_owned())),
        ]);
        let (tel, extra) = runner().execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.audio.set_volume");
        assert!(extra.is_none());
    }
}
