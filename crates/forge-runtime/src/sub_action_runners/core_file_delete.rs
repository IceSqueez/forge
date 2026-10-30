use crate::core_messages;
use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

const PATH: TextField = TextField {
    key: "path",
    label: "File Path (relative to the files folder)",
    editor: TextEditor::Line {
        placeholder: "output/data.txt",
    },
    default: "",
    required: true,
};

const IGNORE_MISSING: ToggleField = ToggleField {
    key: "ignore_missing",
    label: "Ignore Missing File",
    default: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PATH, &IGNORE_MISSING]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, bool), ConfigProblem> {
    Ok((PATH.read(config, args)?, IGNORE_MISSING.read(config, args)?))
}

pub struct CoreFileDeleteRunner;

#[async_trait]
impl SubActionRunner for CoreFileDeleteRunner {
    fn id(&self) -> &str {
        "core.file.delete"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "Delete File"
    }

    fn summary(&self) -> &str {
        "Remove a sandboxed file; never removes directories"
    }

    fn search_text(&self) -> &str {
        "delete remove file assets"
    }

    fn icon_name(&self) -> &str {
        "file-minus"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (path, ignore_missing) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = match super::file_sandbox::resolve_sandboxed(&path).await {
            Err(reason) => SubActionOutcome::Failed(reason.into()),
            Ok(abs_path) => match tokio::fs::remove_file(&abs_path).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if ignore_missing {
                        SubActionOutcome::Success
                    } else {
                        SubActionOutcome::Failed(core_messages::file_not_found().into())
                    }
                }
                Err(e) => SubActionOutcome::Failed(core_messages::file_io_failed(e).into()),
            },
        };

        (timer.finish(outcome), None)
    }
}
