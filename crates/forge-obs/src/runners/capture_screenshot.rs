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
        let source_ok =
            matches!(config.get("source"), Some(Variant::String(s)) if !s.trim().is_empty());
        let path_ok =
            matches!(config.get("path"), Some(Variant::String(s)) if !s.trim().is_empty());
        if source_ok && path_ok {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "obs.capture.screenshot: 'source' and 'path' must not be empty".to_owned(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{RecordingSink, make_ctx};

    async fn recorded_call(source: &str, path: &str) -> String {
        let sink = RecordingSink::new();
        let runner = CaptureScreenshotRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String(source.to_owned())),
            ("path".to_owned(), Variant::String(path.to_owned())),
        ]);
        runner.execute(&config, &make_ctx(&stack)).await;
        sink.calls().first().cloned().unwrap_or_default()
    }

    #[tokio::test]
    async fn screenshot_format_comes_from_the_lowercased_path_extension() {
        for (path, expected_format) in [
            ("/tmp/shot.png", "png"),
            ("/tmp/shot.jpg", "jpg"),
            ("/tmp/shot.JPEG", "jpeg"),
            ("/tmp/my.shot.bmp", "bmp"),
        ] {
            let call = recorded_call("Cam", path).await;
            assert_eq!(
                call,
                format!("save_source_screenshot(Cam,{path},{expected_format})"),
            );
        }
    }

    #[tokio::test]
    async fn screenshot_format_falls_back_to_png_when_the_path_carries_no_extension() {
        for path in ["/tmp/shot", "/tmp/.png"] {
            let call = recorded_call("Cam", path).await;
            assert_eq!(
                call,
                format!("save_source_screenshot(Cam,{path},png)"),
                "path {path:?}",
            );
        }
    }

    #[tokio::test]
    async fn screenshot_path_and_source_are_interpolated_before_the_format_is_derived() {
        let sink = RecordingSink::new();
        let runner = CaptureScreenshotRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new()
            .set("cam".to_owned(), Variant::String("Webcam".to_owned()))
            .set("ext".to_owned(), Variant::String("jpg".to_owned()));
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("%cam%".to_owned())),
            (
                "path".to_owned(),
                Variant::String("/tmp/shot.%ext%".to_owned()),
            ),
        ]);

        runner.execute(&config, &make_ctx(&stack)).await;

        assert_eq!(
            sink.calls(),
            vec!["save_source_screenshot(Webcam,/tmp/shot.jpg,jpg)".to_owned()],
        );
    }
}
