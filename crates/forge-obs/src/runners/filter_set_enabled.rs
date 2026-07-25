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

pub struct FilterSetEnabledRunner {
    sink: Arc<dyn ObsSink>,
}

impl FilterSetEnabledRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for FilterSetEnabledRunner {
    fn id(&self) -> &str {
        "obs.filter.set_enabled"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Filter Enabled"
    }

    fn summary(&self) -> &str {
        "Enables or disables a filter on an OBS source."
    }

    fn search_text(&self) -> &str {
        "obs filter enable disable toggle source effect"
    }

    fn icon_name(&self) -> &str {
        "filter"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String(String::new())),
            ("filter".to_owned(), Variant::String(String::new())),
            ("enabled".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "source",
                label: "Source",
                options_key: "obs.source_names",
            },
            FormField::Text {
                key: "filter",
                label: "Filter Name",
                placeholder: "e.g. Color Correction",
            },
            FormField::Toggle {
                key: "enabled",
                label: "Enabled",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let source_ok = matches!(config.get("source"), Some(Variant::String(_)));
        let filter_ok = matches!(config.get("filter"), Some(Variant::String(_)));
        if source_ok && filter_ok {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "obs.filter.set_enabled: 'source' and 'filter' must be strings".to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_source = config.str("source").unwrap_or_default();
        let raw_filter = config.str("filter").unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw_source);
        let filter = ctx.arg_stack.interpolate(raw_filter);
        let enabled = matches!(config.get("enabled"), Some(Variant::Bool(true)));

        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .set_source_filter_enabled(&source, &filter, enabled)
                .await,
        );

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.filter.set_enabled".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
