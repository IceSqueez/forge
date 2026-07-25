use std::collections::BTreeMap;
use std::path::Path;
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

pub struct CaptureScreenshotRunner {
    sink: Arc<dyn ObsSink>,
}

impl CaptureScreenshotRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for CaptureScreenshotRunner {
    fn id(&self) -> &str {
        "obs.capture.screenshot"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Save Source Screenshot"
    }

    fn summary(&self) -> &str {
        "Saves a screenshot of an OBS source to a file, format from the file extension."
    }

    fn search_text(&self) -> &str {
        "obs screenshot capture save source image png jpg file"
    }

    fn icon_name(&self) -> &str {
        "camera"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String(String::new())),
            ("path".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "source",
                label: "Source",
                options_key: "obs.source_names",
            },
            FormField::FilePicker {
                key: "path",
                label: "Save To",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let source_ok = matches!(config.get("source"), Some(Variant::String(_)));
        let path_ok =
            matches!(config.get("path"), Some(Variant::String(s)) if !s.trim().is_empty());
        if source_ok && path_ok {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "obs.capture.screenshot: 'source' must be a string and 'path' must not be empty"
                    .to_owned(),
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
        let raw_path = config.str("path").unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw_source);
        let path = ctx.arg_stack.interpolate(raw_path);
        let format = Path::new(&path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
            .unwrap_or_else(|| "png".to_owned());

        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .save_source_screenshot(&source, &path, &format)
                .await,
        );

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.capture.screenshot".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
