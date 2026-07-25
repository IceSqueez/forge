use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct StudioSetEnabledRunner {
    sink: Arc<dyn ObsSink>,
}

impl StudioSetEnabledRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for StudioSetEnabledRunner {
    fn id(&self) -> &str {
        "obs.studio.set_enabled"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Studio Mode"
    }

    fn summary(&self) -> &str {
        "Sets OBS studio mode to an explicit on or off state."
    }

    fn search_text(&self) -> &str {
        "obs studio mode set on off enable disable preview"
    }

    fn icon_name(&self) -> &str {
        "adjustments-horizontal"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("on".to_owned(), Variant::Bool(true))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Toggle {
            key: "on",
            label: "Enabled",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("on") {
            Some(Variant::Bool(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "obs.studio.set_enabled: 'on' must be a bool".to_owned(),
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

        let on = matches!(config.get("on"), Some(Variant::Bool(true)));

        let outcome = SubActionOutcome::from_result(&self.sink.set_studio_mode(on).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.studio.set_enabled".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
