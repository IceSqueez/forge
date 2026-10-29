use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;
use crate::vocabulary::{picker_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::BROWSER_REFRESH;

pub struct BrowserRefreshRunner {
    sink: Arc<dyn ObsSink>,
}

impl BrowserRefreshRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for BrowserRefreshRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Refresh Browser Source"
    }

    fn summary(&self) -> &str {
        "Refreshes a browser source, bypassing its cache."
    }

    fn search_text(&self) -> &str {
        "obs browser source refresh reload cache nocache"
    }

    fn icon_name(&self) -> &str {
        "refresh"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("source".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "source",
            label: "Browser Source",
            options_key: picker_keys::INPUT_NAMES,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("source") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            Some(Variant::String(_)) => Err(RegistryError::invalid_field(
                "source",
                ConfigReason::Required,
            )),
            _ => Err(RegistryError::invalid_field(
                "source",
                ConfigReason::Required,
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

        let raw_source = config.str("source").unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw_source);

        let outcome =
            SubActionOutcome::from_result(&self.sink.refresh_browser_source(&source).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
