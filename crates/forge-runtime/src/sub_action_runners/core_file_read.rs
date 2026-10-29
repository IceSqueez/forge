use std::path::Path;

use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
    VariableField,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use tokio::io::AsyncReadExt;

const MAX_FILE_BYTES: u64 = 1_048_576;
const TARGET_VAR_KEY: &str = "target_var";

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
    key: "path",
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
    key: TARGET_VAR_KEY,
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
        "core.file.read"
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
                output_name_key: TARGET_VAR_KEY.to_owned(),
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
            Err(reason) => (
                SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
                None,
            ),
            Ok(abs_path) => read_into(&abs_path, read_as, target_var, ctx).await,
        };

        (timer.finish(outcome), produced)
    }
}

fn decode(contents: String, read_as: ReadAs) -> Result<Variant, String> {
    match read_as {
        ReadAs::WholeFile => Ok(Variant::String(contents)),
        ReadAs::Json => serde_json::from_str::<serde_json::Value>(&contents)
            .map_err(|e| format!("invalid JSON: {e}"))
            .and_then(|json| {
                Variant::from_json(json).map_err(|e| format!("unsupported JSON value: {e}"))
            }),
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
            SubActionOutcome::Failed(format!(
                "file exceeds {MAX_FILE_BYTES} byte cap: {} bytes",
                meta.len()
            )),
            None,
        ),
        Ok(meta) if !meta.is_file() => (
            SubActionOutcome::Failed("path is not a file".to_owned()),
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
            Err(e) => (SubActionOutcome::Failed(e), None),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            SubActionOutcome::Failed("file not found in the files folder".to_owned()),
            None,
        ),
        Err(e) => (SubActionOutcome::Failed(format!("stat failed: {e}")), None),
    }
}

async fn read_capped(path: &Path) -> Result<String, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(format!("file exceeds {MAX_FILE_BYTES} byte cap"));
    }
    String::from_utf8(bytes).map_err(|e| format!("read failed: {e}"))
}
