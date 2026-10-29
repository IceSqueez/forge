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
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::RECORDING_SET_DIRECTORY;

pub struct RecordSetDirectoryRunner {
    sink: Arc<dyn ObsSink>,
}

impl RecordSetDirectoryRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for RecordSetDirectoryRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Record Directory"
    }

    fn summary(&self) -> &str {
        "Sets the directory OBS writes recording files to."
    }

    fn search_text(&self) -> &str {
        "obs record directory folder path output file location"
    }

    fn icon_name(&self) -> &str {
        "folder"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("path".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::FilePicker {
            key: "path",
            label: "Directory",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("path") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            _ => Err(RegistryError::invalid_field("path", ConfigReason::Required)),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_path = config.str("path").unwrap_or_default();
        let path = ctx.arg_stack.interpolate(raw_path);

        let outcome = SubActionOutcome::from_result(&self.sink.set_record_directory(&path).await);

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
