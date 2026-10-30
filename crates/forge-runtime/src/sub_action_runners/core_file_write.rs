use std::io::ErrorKind;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core_messages;
use async_trait::async_trait;
use base64::Engine as _;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, ToggleField, TypedField,
};
use forge_types::{
    ArgStack, MessageRef, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use tokio::io::AsyncWriteExt as _;

static STAGED_WRITES: AtomicU64 = AtomicU64::new(0);

const FILE_BYTES_WRITTEN_VAR: &str = "file.bytes_written";

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Latin1,
    RawBase64,
}

impl ConfigChoice for Encoding {
    const ALL: &'static [Self] = &[Self::Utf8, Self::Latin1, Self::RawBase64];
    const WIRE_NAMES: &'static [&'static str] = &["utf8", "latin1", "raw_base64"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::Latin1 => "latin1",
            Self::RawBase64 => "raw_base64",
        }
    }
}

#[derive(Clone, Copy)]
enum WriteMode {
    Overwrite,
    Append,
    CreateNew,
}

impl ConfigChoice for WriteMode {
    const ALL: &'static [Self] = &[Self::Overwrite, Self::Append, Self::CreateNew];
    const WIRE_NAMES: &'static [&'static str] = &["overwrite", "append", "create_new"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Overwrite => "overwrite",
            Self::Append => "append",
            Self::CreateNew => "create_new",
        }
    }
}

const PATH: TextField = TextField {
    key: "path",
    label: "File Path (relative to the files folder)",
    editor: TextEditor::Line {
        placeholder: "output/data.txt",
    },
    default: "",
    required: true,
};

const CONTENT: TextField = TextField {
    key: "content",
    label: "Content",
    editor: TextEditor::Area,
    default: "",
    required: false,
};

const ENCODING: ChoiceField<Encoding> = ChoiceField {
    key: "encoding",
    label: "Encoding",
    default: Encoding::Utf8,
};

const MODE: ChoiceField<WriteMode> = ChoiceField {
    key: "mode",
    label: "Write Mode",
    default: WriteMode::Overwrite,
};

const CREATE_PARENT_DIRS: ToggleField = ToggleField {
    key: "create_parent_dirs",
    label: "Create Parent Directories",
    default: false,
};

const CONFIG: ConfigSpec =
    ConfigSpec::new(&[&PATH, &CONTENT, &ENCODING, &MODE, &CREATE_PARENT_DIRS]);

struct WriteRequest {
    path: String,
    content: String,
    encoding: Encoding,
    mode: WriteMode,
    create_parent_dirs: bool,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<WriteRequest, ConfigProblem> {
    Ok(WriteRequest {
        path: PATH.read(config, args)?,
        content: CONTENT.read(config, args)?,
        encoding: ENCODING.read(config, args)?,
        mode: MODE.read(config, args)?,
        create_parent_dirs: CREATE_PARENT_DIRS.read(config, args)?,
    })
}

pub struct CoreFileWriteRunner;

#[async_trait]
impl SubActionRunner for CoreFileWriteRunner {
    fn id(&self) -> &str {
        "core.file.write"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "Write File"
    }

    fn summary(&self) -> &str {
        "Write content to a sandboxed file; stores bytes written in `file.bytes_written`"
    }

    fn search_text(&self) -> &str {
        "write file save output create append overwrite assets"
    }

    fn icon_name(&self) -> &str {
        "file-edit"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![FILE_BYTES_WRITTEN_VAR.to_owned()],
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

        let (outcome, updated) = match super::file_sandbox::resolve_sandboxed(&request.path).await {
            Err(reason) => (SubActionOutcome::Failed(reason.into()), None),
            Ok(abs_path) => match do_write(&abs_path, &request).await {
                Err(reason) => (SubActionOutcome::Failed(reason.into()), None),
                Ok(bytes_written) => {
                    let stack = ctx.arg_stack.clone().set(
                        FILE_BYTES_WRITTEN_VAR.to_owned(),
                        Variant::Int(i64::try_from(bytes_written).unwrap_or(i64::MAX)),
                    );
                    (SubActionOutcome::Success, Some(stack))
                }
            },
        };

        (timer.finish(outcome), updated)
    }
}

fn encode_content(content: &str, encoding: Encoding) -> Result<Vec<u8>, MessageRef> {
    match encoding {
        Encoding::Utf8 => Ok(content.as_bytes().to_vec()),
        Encoding::Latin1 => content
            .chars()
            .map(|c| u8::try_from(u32::from(c)).map_err(|_| core_messages::file_char_not_latin1(c)))
            .collect(),
        Encoding::RawBase64 => base64::engine::general_purpose::STANDARD
            .decode(content.trim())
            .map_err(core_messages::file_not_base64),
    }
}

async fn do_write(abs_path: &std::path::Path, request: &WriteRequest) -> Result<usize, MessageRef> {
    if request.create_parent_dirs
        && let Some(parent) = abs_path.parent()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(core_messages::file_io_failed)?;
    }

    let bytes = encode_content(&request.content, request.encoding)?;

    let mut opts = tokio::fs::OpenOptions::new();
    match request.mode {
        WriteMode::Overwrite => return replace_file(abs_path, &bytes).await,
        WriteMode::Append => {
            opts.append(true).create(true);
        }
        WriteMode::CreateNew => {
            opts.write(true).create_new(true);
        }
    }

    let mut file = opts
        .open(abs_path)
        .await
        .map_err(core_messages::file_io_failed)?;

    file.write_all(&bytes)
        .await
        .map_err(core_messages::file_io_failed)?;
    file.flush().await.map_err(core_messages::file_io_failed)?;

    Ok(bytes.len())
}

async fn replace_file(abs_path: &std::path::Path, bytes: &[u8]) -> Result<usize, MessageRef> {
    let staged = staged_sibling(abs_path);
    let written = write_synced(&staged, bytes).await;
    let replaced = match written {
        Ok(()) => tokio::fs::rename(&staged, abs_path).await,
        Err(error) => Err(error),
    };
    if let Err(error) = replaced {
        if let Err(cleanup) = tokio::fs::remove_file(&staged).await
            && cleanup.kind() != ErrorKind::NotFound
        {
            tracing::warn!(error = %cleanup, "staged file write could not be removed");
        }
        return Err(core_messages::file_io_failed(error));
    }
    Ok(bytes.len())
}

async fn write_synced(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    file.sync_all().await
}

fn staged_sibling(abs_path: &std::path::Path) -> std::path::PathBuf {
    let writer = STAGED_WRITES.fetch_add(1, Ordering::Relaxed);
    let name = abs_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    abs_path.with_file_name(format!(".{name}.{}.{writer}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_content_utf8_returns_raw_utf8_bytes() {
        let expected = "héllo".as_bytes().to_vec();
        assert_eq!(encode_content("héllo", Encoding::Utf8).unwrap(), expected);
    }

    #[test]
    fn encode_content_latin1_maps_each_char_to_one_byte() {
        assert_eq!(
            encode_content("Aÿ", Encoding::Latin1).unwrap(),
            vec![0x41, 0xFF]
        );
    }

    #[test]
    fn encode_content_latin1_accepts_ff_boundary_and_rejects_above() {
        assert_eq!(
            encode_content("\u{00FF}", Encoding::Latin1).unwrap(),
            vec![0xFF]
        );
        let err = encode_content("\u{0100}", Encoding::Latin1).unwrap_err();
        assert_eq!(err.code(), &crate::core_messages::FILE_CHAR_NOT_LATIN1);
        assert_eq!(
            err.arg("char"),
            Some(&forge_types::MessageArg::Verbatim(
                forge_types::VerbatimText::new("\u{0100}")
            ))
        );
    }

    #[test]
    fn encode_content_base64_decodes_trimmed_input() {
        assert_eq!(encode_content("aGk=", Encoding::RawBase64).unwrap(), b"hi");
        assert_eq!(
            encode_content("  aGk=\n", Encoding::RawBase64).unwrap(),
            b"hi"
        );
    }

    #[test]
    fn encode_content_base64_rejects_invalid_payload() {
        let err = encode_content("not base64!", Encoding::RawBase64).unwrap_err();
        assert_eq!(err.code(), &crate::core_messages::FILE_NOT_BASE64);
        assert!(err.detail().is_some());
    }
}
