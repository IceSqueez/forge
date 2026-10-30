use std::path::Path;

use crate::core_messages;
use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
    VariableField,
};
use forge_types::{
    ArgStack, MessageRef, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
    VariantKind,
};
use tokio::io::AsyncReadExt;

const MAX_FILE_BYTES: u64 = 1_048_576;
pub const FILE_READ_KIND_ID: &str = "core.file.read";
pub const FILE_READ_PATH_KEY: &str = "path";
pub const FILE_READ_TARGET_VAR_KEY: &str = "target_var";

#[derive(Clone, Copy)]
enum ReadAs {
    Lines,
    WholeFile,
    Json,
}

impl ConfigChoice for ReadAs {
    const ALL: &'static [Self] = &[Self::Lines, Self::WholeFile, Self::Json];
    const WIRE_NAMES: &'static [&'static str] = &["Lines array", "Whole file", "JSON"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Lines => "Lines array",
            Self::WholeFile => "Whole file",
            Self::Json => "JSON",
        }
    }
}

const PATH: TextField = TextField {
    key: FILE_READ_PATH_KEY,
    label: "File Path (relative to the files folder)",
    editor: TextEditor::Line {
        placeholder: "quotes/cards.md",
    },
    default: "",
    required: true,
};

const READ_AS: ChoiceField<ReadAs> = ChoiceField {
    key: "read_as",
    label: "Read As",
    default: ReadAs::Lines,
};

const TARGET_VAR: VariableField = VariableField {
    key: FILE_READ_TARGET_VAR_KEY,
    label: "Output Variable",
    placeholder: "file_contents",
    default: "",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PATH, &READ_AS, &TARGET_VAR]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, ReadAs, String), ConfigProblem> {
    Ok((
        PATH.read(config, args)?,
        READ_AS.read(config, args)?,
        TARGET_VAR.read(config, args)?,
    ))
}

pub struct CoreFileReadRunner;

#[async_trait]
impl SubActionRunner for CoreFileReadRunner {
    fn id(&self) -> &str {
        FILE_READ_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "Read File"
    }

    fn summary(&self) -> &str {
        "Read a text file into a variable"
    }

    fn search_text(&self) -> &str {
        "read file text load assets"
    }

    fn icon_name(&self) -> &str {
        "file-text"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: FILE_READ_TARGET_VAR_KEY.to_owned(),
                kind: VariantKind::Array,
                label: "File contents".to_owned(),
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
        let (path, read_as, target_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let (outcome, produced) = match super::file_sandbox::resolve_sandboxed(&path).await {
            Err(reason) => (SubActionOutcome::Failed(reason.into()), None),
            Ok(abs_path) => read_into(&abs_path, read_as, target_var, ctx).await,
        };

        (timer.finish(outcome), produced)
    }
}

fn decode(contents: String, read_as: ReadAs) -> Result<Variant, MessageRef> {
    match read_as {
        ReadAs::WholeFile => Ok(Variant::String(contents)),
        ReadAs::Json => serde_json::from_str::<serde_json::Value>(&contents)
            .map_err(core_messages::file_not_json)
            .and_then(|json| Variant::from_json(json).map_err(core_messages::file_not_json)),
        ReadAs::Lines => Ok(Variant::Array(
            contents
                .lines()
                .map(|line| Variant::String(line.to_owned()))
                .collect(),
        )),
    }
}

async fn read_into(
    abs_path: &Path,
    read_as: ReadAs,
    target_var: String,
    ctx: &RunContext<'_>,
) -> (SubActionOutcome, Option<ArgStack>) {
    match tokio::fs::metadata(abs_path).await {
        Ok(meta) if meta.len() > MAX_FILE_BYTES => (
            SubActionOutcome::Failed(core_messages::file_too_large(MAX_FILE_BYTES).into()),
            None,
        ),
        Ok(meta) if !meta.is_file() => (
            SubActionOutcome::Failed(core_messages::path_not_file().into()),
            None,
        ),
        Ok(_) => match read_capped(abs_path)
            .await
            .and_then(|contents| decode(contents, read_as))
        {
            Ok(value) => {
                let stack = ctx.arg_stack.clone().set(target_var, value);
                (SubActionOutcome::Success, Some(stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.into()), None),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            SubActionOutcome::Failed(core_messages::file_not_found().into()),
            None,
        ),
        Err(e) => (
            SubActionOutcome::Failed(core_messages::file_io_failed(e).into()),
            None,
        ),
    }
}

async fn read_capped(path: &Path) -> Result<String, MessageRef> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(core_messages::file_io_failed)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(core_messages::file_io_failed)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(core_messages::file_too_large(MAX_FILE_BYTES));
    }
    String::from_utf8(bytes).map_err(core_messages::file_io_failed)
}
