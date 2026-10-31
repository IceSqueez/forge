use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{WEBHOOK, filled_text, optional_text};
use super::mention_fields::{ALLOW_EVERYONE_PINGS, ALLOW_ROLE_PINGS, read_mention_policy};
use super::step_failure;
use super::{message_id_scope_io, message_id_stack};
use crate::mention::MentionPolicy;
use crate::sink::DiscordSink;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::WEBHOOK_SEND_FILE;

const BYTES_PER_MEBIBYTE: u64 = 1024 * 1024;
const UPLOAD_LIMIT_MEBIBYTES: u64 = 25;
const DISCORD_UPLOAD_LIMIT_BYTES: u64 = UPLOAD_LIMIT_MEBIBYTES * BYTES_PER_MEBIBYTE;

const CAPTION: TextField = optional_text(config_keys::CONTENT, "Caption", TextEditor::Area);

const FILE_PATH: TextField = filled_text(
    config_keys::FILE_PATH,
    "File Path",
    TextEditor::Line {
        placeholder: "/home/user/clip.png",
    },
);

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &WEBHOOK,
    &CAPTION,
    &FILE_PATH,
    &ALLOW_ROLE_PINGS,
    &ALLOW_EVERYONE_PINGS,
]);

struct FileUpload {
    webhook: String,
    caption: Option<String>,
    file_path: String,
    mentions: MentionPolicy,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<FileUpload, ConfigProblem> {
    Ok(FileUpload {
        webhook: WEBHOOK.read(config, args)?,
        caption: Some(CAPTION.read(config, args)?).filter(|text| !text.trim().is_empty()),
        file_path: FILE_PATH.read(config, args)?,
        mentions: read_mention_policy(config, args)?,
    })
}

pub(crate) struct SendFileRunner {
    sink: Arc<dyn DiscordSink>,
}

impl SendFileRunner {
    pub(crate) fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SendFileRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Send File Attachment"
    }

    fn summary(&self) -> &str {
        "Uploads a file from disk to a Discord webhook with an optional caption."
    }

    fn search_text(&self) -> &str {
        "discord webhook send file attachment upload"
    }

    fn icon_name(&self) -> &str {
        "paperclip"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        message_id_scope_io()
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let upload = match read_config(config, ctx.arg_stack) {
            Ok(upload) => upload,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let Some(file_name) = Path::new(&upload.file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            return (
                timer.failed(messages::file_name_unavailable(&upload.file_path).into()),
                None,
            );
        };

        match tokio::fs::metadata(&upload.file_path).await {
            Ok(meta) if meta.is_file() && meta.len() <= DISCORD_UPLOAD_LIMIT_BYTES => {}
            Ok(_) => {
                return (
                    timer.failed(messages::file_not_uploadable(&upload.file_path).into()),
                    None,
                );
            }
            Err(e) => {
                return (
                    timer.failed(
                        messages::file_unreadable(&upload.file_path, &e.to_string()).into(),
                    ),
                    None,
                );
            }
        }

        let file_bytes = match tokio::fs::read(&upload.file_path).await {
            Ok(bytes) => bytes,
            Err(e) => {
                return (
                    timer.failed(
                        messages::file_unreadable(&upload.file_path, &e.to_string()).into(),
                    ),
                    None,
                );
            }
        };

        let posted = self
            .sink
            .send_file(
                &upload.webhook,
                upload.caption.as_deref(),
                &file_name,
                &file_bytes,
                upload.mentions,
            )
            .await;
        let produced = message_id_stack(ctx.arg_stack, &posted);
        (timer.finish(step_failure::outcome(&posted)), produced)
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;
    use std::collections::BTreeMap;

    use forge_types::Variant;

    use forge_types::SubActionOutcome;

    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use crate::mention::MentionPolicy;

    use forge_types::EventId;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Captured {
        webhook_name: String,
        content: Option<String>,
        file_name: String,
        file_bytes: Vec<u8>,
    }

    struct CapturingSink {
        captured: Mutex<Option<Captured>>,
        should_fail: bool,
    }

    impl CapturingSink {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(None),
                should_fail: false,
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(None),
                should_fail: true,
            })
        }

        fn captured(&self) -> Option<Captured> {
            self.captured.lock().unwrap().take()
        }
    }

    #[async_trait]
    impl DiscordSink for CapturingSink {
        async fn post_text(
            &self,
            _: &str,
            _: &str,
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn post_embed(&self, _: &str, _: DiscordEmbed) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn edit_message(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<DiscordEmbed>,
            _: MentionPolicy,
        ) -> Result<(), DiscordError> {
            Ok(())
        }
        async fn send_file(
            &self,
            webhook_name: &str,
            content: Option<&str>,
            file_name: &str,
            file_bytes: &[u8],
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
            *self.captured.lock().unwrap() = Some(Captured {
                webhook_name: webhook_name.to_owned(),
                content: content.map(str::to_owned),
                file_name: file_name.to_owned(),
                file_bytes: file_bytes.to_vec(),
            });
            if self.should_fail {
                Err(DiscordError::WebhookNotFound {
                    name: webhook_name.to_owned(),
                })
            } else {
                Ok("msg-1".to_owned())
            }
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    fn config(file_path: &str, caption: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "webhook_name".to_owned(),
                Variant::String("alerts".to_owned()),
            ),
            ("content".to_owned(), Variant::String(caption.to_owned())),
            (
                "file_path".to_owned(),
                Variant::String(file_path.to_owned()),
            ),
        ])
    }

    fn ctx<'a>(stack: &'a ArgStack, publisher: &'a NullPublisher) -> RunContext<'a> {
        RunContext::leaf(stack, 0, EventId::new(), publisher)
    }

    fn temp_file(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("forge-discord-test-{name}"));
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[tokio::test]
    async fn empty_file_path_fails_without_calling_sink() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let (telemetry, _) = runner
            .execute(&config("", "cap"), &ctx(&stack, &publisher))
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(sink.captured().is_none(), "sink must not run on empty path");
    }

    #[tokio::test]
    async fn missing_file_fails_without_calling_sink() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let missing = std::env::temp_dir().join("forge-discord-test-does-not-exist-xyz.png");
        let cfg = config(missing.to_str().unwrap(), "cap");
        let (telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            sink.captured().is_none(),
            "sink must not run on unreadable file"
        );
    }

    fn sized_file(dir: &tempfile::TempDir, len: u64) -> std::path::PathBuf {
        let path = dir.path().join(format!("sized-{len}.bin"));
        std::fs::File::create(&path).unwrap().set_len(len).unwrap();
        path
    }

    async fn upload_of(path: &str) -> (SubActionOutcome, Arc<CapturingSink>) {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let (telemetry, _) = runner
            .execute(&config(path, "cap"), &ctx(&stack, &publisher))
            .await;
        (telemetry.outcome, sink)
    }

    #[tokio::test]
    async fn a_path_that_is_not_an_uploadable_regular_file_fails_without_reading_it() {
        let dir = tempfile::tempdir().unwrap();
        let oversize = sized_file(&dir, DISCORD_UPLOAD_LIMIT_BYTES + 1);
        let mut refused = vec![
            dir.path().to_str().unwrap().to_owned(),
            oversize.to_str().unwrap().to_owned(),
        ];
        if cfg!(unix) {
            refused.push("/dev/zero".to_owned());
        }

        for path in refused {
            let (outcome, sink) = upload_of(&path).await;

            assert_eq!(
                outcome,
                SubActionOutcome::Failed(messages::file_not_uploadable(&path).into()),
                "{path}"
            );
            assert!(sink.captured().is_none(), "{path} reached the sink");
        }
    }

    #[tokio::test]
    async fn a_file_of_exactly_the_upload_limit_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let at_limit = sized_file(&dir, DISCORD_UPLOAD_LIMIT_BYTES);

        let (outcome, sink) = upload_of(at_limit.to_str().unwrap()).await;

        assert_eq!(outcome, SubActionOutcome::Success);
        assert_eq!(
            sink.captured()
                .expect("sink must be called")
                .file_bytes
                .len() as u64,
            DISCORD_UPLOAD_LIMIT_BYTES
        );
    }

    #[tokio::test]
    async fn reads_file_and_forwards_bytes_basename_and_no_caption_when_empty() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let (_dir, path) = temp_file("clip-a.png", &[0xDE, 0xAD, 0xBE, 0xEF]);
        let cfg = config(path.to_str().unwrap(), "   ");
        let (telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let c = sink.captured().expect("sink must be called");
        assert_eq!(c.file_bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(c.file_name, "forge-discord-test-clip-a.png");
        assert_eq!(c.webhook_name, "alerts");
        assert!(c.content.is_none(), "blank caption must become None");
    }

    #[tokio::test]
    async fn non_empty_caption_is_forwarded_as_some() {
        let sink = CapturingSink::ok();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let (_dir, path) = temp_file("clip-b.png", &[1, 2, 3]);
        let cfg = config(path.to_str().unwrap(), "look at this");
        let (_telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        let c = sink.captured().expect("sink must be called");
        assert_eq!(c.content.as_deref(), Some("look at this"));
    }

    #[tokio::test]
    async fn sink_error_yields_failed_outcome() {
        let sink = CapturingSink::failing();
        let runner = SendFileRunner::new(Arc::clone(&sink) as Arc<dyn DiscordSink>);
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let (_dir, path) = temp_file("clip-c.png", &[9]);
        let cfg = config(path.to_str().unwrap(), "cap");
        let (telemetry, _) = runner.execute(&cfg, &ctx(&stack, &publisher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }
}
