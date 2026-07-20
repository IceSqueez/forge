use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct SetVisibleRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetVisibleRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetVisibleRunner {
    fn id(&self) -> &str {
        "obs.sources.set_visible"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Source Visible"
    }

    fn summary(&self) -> &str {
        "Shows or hides a source within an OBS scene."
    }

    fn search_text(&self) -> &str {
        "obs source visible hidden show hide toggle scene item"
    }

    fn icon_name(&self) -> &str {
        "eye"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("scene".to_owned(), Variant::String(String::new())),
            ("source".to_owned(), Variant::String(String::new())),
            ("visible".to_owned(), Variant::Bool(true)),
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
                key: "visible",
                label: "Visible",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let scene_ok = matches!(config.get("scene"), Some(Variant::String(_)));
        let source_ok = matches!(config.get("source"), Some(Variant::String(_)));
        if scene_ok && source_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "obs.sources.set_visible: 'scene' and 'source' must be strings".to_owned(),
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
        let raw_source = config
            .get("source")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let visible = matches!(config.get("visible"), Some(Variant::Bool(true)));

        let scene = ctx.arg_stack.interpolate(raw_scene);
        let source = ctx.arg_stack.interpolate(raw_source);

        let outcome = match self.sink.set_source_visible(&scene, &source, visible).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.sources.set_visible".to_owned(),
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
    fn validate_config_accepts_valid_config() {
        let runner = SetVisibleRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([
            ("scene".to_owned(), Variant::String("Gameplay".to_owned())),
            ("source".to_owned(), Variant::String("Cam".to_owned())),
            ("visible".to_owned(), Variant::Bool(true)),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_source() {
        let runner = SetVisibleRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Gameplay".to_owned()))]);
        assert!(runner.validate_config(&config).is_err());
    }

    #[test]
    fn validate_config_rejects_missing_scene() {
        let runner = SetVisibleRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("source".to_owned(), Variant::String("Cam".to_owned()))]);
        assert!(runner.validate_config(&config).is_err());
    }
}
