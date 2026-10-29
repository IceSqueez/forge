use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::VIRTUALCAM_SET_ACTIVE;

pub struct VirtualCamSetActiveRunner {
    sink: Arc<dyn ObsSink>,
}

impl VirtualCamSetActiveRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for VirtualCamSetActiveRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Virtual Camera Active"
    }

    fn summary(&self) -> &str {
        "Starts or stops the OBS virtual camera output to an explicit state."
    }

    fn search_text(&self) -> &str {
        "obs virtual camera cam start stop set on off active"
    }

    fn icon_name(&self) -> &str {
        "camera"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("on".to_owned(), Variant::Bool(true))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Toggle {
            key: "on",
            label: "Active",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("on") {
            Some(Variant::Bool(_)) => Ok(()),
            _ => Err(RegistryError::invalid_field("on", ConfigReason::Required)),
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

        let outcome = if on {
            SubActionOutcome::from_result(&self.sink.start_virtual_cam().await)
        } else {
            SubActionOutcome::from_result(&self.sink.stop_virtual_cam().await)
        };

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
