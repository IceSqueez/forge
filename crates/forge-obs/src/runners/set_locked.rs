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

pub(crate) const KIND_ID: &str = "obs.sources.set_locked";

pub struct SetLockedRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetLockedRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetLockedRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Source Locked"
    }

    fn summary(&self) -> &str {
        "Locks or unlocks a source within an OBS scene."
    }

    fn search_text(&self) -> &str {
        "obs source lock locked unlock scene item"
    }

    fn icon_name(&self) -> &str {
        "lock"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("scene".to_owned(), Variant::String(String::new())),
            ("source".to_owned(), Variant::String(String::new())),
            ("locked".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "scene",
                label: "Scene",
                options_key: "obs.scene_names",
            },
            FormField::DynamicSelect {
                key: "source",
                label: "Source",
                options_key: "obs.source_names",
            },
            FormField::Toggle {
                key: "locked",
                label: "Locked",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let scene_ok =
            matches!(config.get("scene"), Some(Variant::String(s)) if !s.trim().is_empty());
        let source_ok =
            matches!(config.get("source"), Some(Variant::String(s)) if !s.trim().is_empty());
        if scene_ok && source_ok {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'scene' and 'source' must be non-empty strings"
            )))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_scene = config.str("scene").unwrap_or_default();
        let raw_source = config.str("source").unwrap_or_default();
        let locked = matches!(config.get("locked"), Some(Variant::Bool(true)));

        let scene = ctx.arg_stack.interpolate(raw_scene);
        let source = ctx.arg_stack.interpolate(raw_source);

        let outcome = SubActionOutcome::from_result(
            &self.sink.set_source_locked(&scene, &source, locked).await,
        );

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
