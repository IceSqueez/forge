use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, PickedTextField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::{path_field, step_failure};
use crate::ObsSink;
use crate::runners::fields::SOURCE;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CAPTURE_SCREENSHOT;

const PATH: PickedTextField = PickedTextField {
    key: config_keys::PATH,
    label: "Save To",
    picker: TextPicker::File,
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &PATH]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((
        SOURCE.read(config, args)?,
        path_field::read_path(&PATH, config, args)?,
    ))
}

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
        KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        _interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        path_field::check_template(&PATH, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (source, path) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let format = Path::new(&path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
            .unwrap_or_else(|| "png".to_owned());
        let outcome = step_failure::outcome(
            &self
                .sink
                .save_source_screenshot(&source, &path, &format)
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{RecordingSink, make_ctx};
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
