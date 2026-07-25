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

pub struct SceneCollectionSwitchRunner {
    sink: Arc<dyn ObsSink>,
}

impl SceneCollectionSwitchRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SceneCollectionSwitchRunner {
    fn id(&self) -> &str {
        "obs.scene_collection.switch"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Switch Scene Collection"
    }

    fn summary(&self) -> &str {
        "Switches OBS to a different scene collection by name."
    }

    fn search_text(&self) -> &str {
        "obs scene collection switch change load"
    }

    fn icon_name(&self) -> &str {
        "stack-2"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("name".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "name",
            label: "Collection Name",
            placeholder: "e.g. Main Collection",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if matches!(config.get("name"), Some(Variant::String(_))) {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "obs.scene_collection.switch: 'name' must be a string".to_owned(),
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

        let raw_name = config.str("name").unwrap_or_default();
        let name = ctx.arg_stack.interpolate(raw_name);

        let outcome =
            SubActionOutcome::from_result(&self.sink.set_current_scene_collection(&name).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.scene_collection.switch".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
