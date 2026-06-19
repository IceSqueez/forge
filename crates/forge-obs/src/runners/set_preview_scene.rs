use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct SetPreviewSceneRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetPreviewSceneRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetPreviewSceneRunner {
    fn id(&self) -> &str {
        "obs.scenes.set_preview"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Preview Scene"
    }

    fn summary(&self) -> &str {
        "Sets the current preview scene in OBS Studio Mode."
    }

    fn search_text(&self) -> &str {
        "obs scene preview studio mode set switch"
    }

    fn icon_name(&self) -> &str {
        "monitor"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "scene",
            label: "Scene",
            options_key: "obs.scene_names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if matches!(config.get("scene"), Some(Variant::String(_))) {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "obs.scenes.set_preview: 'scene' must be a string".to_owned(),
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

        let raw_scene = config
            .get("scene")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let scene = ctx.arg_stack.interpolate(raw_scene);

        let outcome = match self.sink.set_preview_scene(&scene).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.scenes.set_preview".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
