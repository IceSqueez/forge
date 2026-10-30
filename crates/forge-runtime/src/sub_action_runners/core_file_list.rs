use std::io::ErrorKind;
use std::path::Path;

use crate::core_messages;
use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, ToggleField, TypedField, VariableField,
};
use forge_types::{
    ArgStack, MessageRef, SubActionConfig, SubActionOutcome, SubActionTelemetry,
    VARIANT_MAX_COLLECTION_LEN, Variant, VariantKind,
};

const INTO_VAR_KEY: &str = "into_var";
const DEFAULT_INTO_VAR: &str = "file.entries";

const PATH: TextField = TextField {
    key: "path",
    label: "Directory Path (relative to the files folder)",
    editor: TextEditor::Line {
        placeholder: "logs/",
    },
    default: "",
    required: true,
};

const PATTERN: TextField = TextField {
    key: "pattern",
    label: "Glob Pattern",
    editor: TextEditor::Line { placeholder: "*" },
    default: "*",
    required: true,
};

const RECURSIVE: ToggleField = ToggleField {
    key: "recursive",
    label: "Recursive",
    default: false,
};

const INCLUDE_DIRS: ToggleField = ToggleField {
    key: "include_dirs",
    label: "Include Directories",
    default: false,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: DEFAULT_INTO_VAR,
    default: DEFAULT_INTO_VAR,
};

const CONFIG: ConfigSpec =
    ConfigSpec::new(&[&PATH, &PATTERN, &RECURSIVE, &INCLUDE_DIRS, &INTO_VAR]);

struct ListRequest {
    path: String,
    pattern: String,
    recursive: bool,
    include_dirs: bool,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<ListRequest, ConfigProblem> {
    Ok(ListRequest {
        path: PATH.read(config, args)?,
        pattern: PATTERN.read(config, args)?,
        recursive: RECURSIVE.read(config, args)?,
        include_dirs: INCLUDE_DIRS.read(config, args)?,
        into_var: INTO_VAR.read(config, args)?,
    })
}

pub struct CoreFileListRunner;

#[async_trait]
impl SubActionRunner for CoreFileListRunner {
    fn id(&self) -> &str {
        "core.file.list"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "List Directory"
    }

    fn summary(&self) -> &str {
        "Enumerate a sandboxed directory; stores entry names in a variable array"
    }

    fn search_text(&self) -> &str {
        "list directory files glob recursive assets"
    }

    fn icon_name(&self) -> &str {
        "folder"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Array,
                label: "Directory entries".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let request = match read_config(config, ctx.arg_stack) {
            Ok(request) => request,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let (outcome, produced) = match super::file_sandbox::resolve_sandboxed(&request.path).await
        {
            Err(reason) => (SubActionOutcome::Failed(reason.into()), None),
            Ok(abs_path) => match tokio::fs::metadata(&abs_path).await {
                Err(error) if error.kind() == ErrorKind::NotFound => (
                    SubActionOutcome::Failed(core_messages::directory_not_found().into()),
                    None,
                ),
                Err(error) => (
                    SubActionOutcome::Failed(core_messages::file_io_failed(error).into()),
                    None,
                ),
                Ok(meta) if !meta.is_dir() => (
                    SubActionOutcome::Failed(core_messages::path_not_directory().into()),
                    None,
                ),
                Ok(_) => match collect_entries(&abs_path, &request).await {
                    Err(reason) => (SubActionOutcome::Failed(reason.into()), None),
                    Ok(entries) => {
                        let array =
                            Variant::Array(entries.into_iter().map(Variant::String).collect());
                        let stack = ctx.arg_stack.clone().set(request.into_var, array);
                        (SubActionOutcome::Success, Some(stack))
                    }
                },
            },
        };

        (timer.finish(outcome), produced)
    }
}

async fn collect_entries(root: &Path, request: &ListRequest) -> Result<Vec<String>, MessageRef> {
    let mut result = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut read_dir = tokio::fs::read_dir(&current)
            .await
            .map_err(core_messages::file_io_failed)?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(core_messages::file_io_failed)?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(core_messages::file_io_failed)?;

            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = entry
                .path()
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());

            if file_type.is_dir() {
                if request.recursive {
                    stack.push(entry.path());
                }
                if request.include_dirs
                    && super::file_sandbox::glob_matches(&request.pattern, &name)
                {
                    result.push(rel);
                }
            } else if super::file_sandbox::glob_matches(&request.pattern, &name) {
                result.push(rel);
            }
            if result.len() > VARIANT_MAX_COLLECTION_LEN {
                return Err(core_messages::directory_too_large(
                    VARIANT_MAX_COLLECTION_LEN,
                ));
            }
        }
    }

    result.sort_unstable();
    Ok(result)
}
