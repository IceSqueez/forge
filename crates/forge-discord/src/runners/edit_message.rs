use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, Interpolation, OptionalField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::{MESSAGE_ID, WEBHOOK, optional_text, present, unlisted_text};
use super::mention_fields::{ALLOW_EVERYONE_PINGS, ALLOW_ROLE_PINGS, read_mention_policy};
use crate::embed::DiscordEmbed;
use crate::mention::MentionPolicy;
use crate::sink::DiscordSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::WEBHOOK_UPDATE_MESSAGE;

const CONTENT: TextField = optional_text(config_keys::CONTENT, "New Content", TextEditor::Area);

const EMBED_TITLE: OptionalField<TextField> =
    unlisted_text(config_keys::EMBED_TITLE, "Embed Title");

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &WEBHOOK,
    &MESSAGE_ID,
    &CONTENT,
    &ALLOW_ROLE_PINGS,
    &ALLOW_EVERYONE_PINGS,
]);

fn no_content() -> ConfigProblem {
    ConfigProblem::whole(ConfigReason::any_of([
        config_keys::CONTENT,
        config_keys::EMBED_TITLE,
    ]))
}

struct MessageEdit {
    webhook: String,
    message_id: String,
    content: Option<String>,
    embed: Option<DiscordEmbed>,
    mentions: MentionPolicy,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<MessageEdit, ConfigProblem> {
    let webhook = WEBHOOK.read(config, args)?;
    let message_id = MESSAGE_ID.read(config, args)?;
    let content = present(CONTENT.read(config, args)?);
    let embed_title = EMBED_TITLE.read(config, args)?.and_then(present);
    if content.is_none() && embed_title.is_none() {
        return Err(no_content());
    }
    Ok(MessageEdit {
        webhook,
        message_id,
        content,
        embed: embed_title.map(|title| DiscordEmbed {
            title: Some(title),
            ..Default::default()
        }),
        mentions: read_mention_policy(config, args)?,
    })
}

pub struct EditMessageRunner {
    sink: Arc<dyn DiscordSink>,
}

impl EditMessageRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for EditMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Edit Message"
    }

    fn summary(&self) -> &str {
        "Edits a previously posted Discord webhook message."
    }

    fn search_text(&self) -> &str {
        "discord webhook edit message update"
    }

    fn icon_name(&self) -> &str {
        "pencil"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        let content = CONTENT.peek(config, interpolation)?;
        let embed_title = EMBED_TITLE.peek(config, interpolation)?;
        if content.is_some_and(|text| present(text).is_none()) && matches!(embed_title, Some(None))
        {
            return Err(no_content());
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let edit = match read_config(config, ctx.arg_stack) {
            Ok(edit) => edit,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let edited = self
            .sink
            .edit_message(
                &edit.webhook,
                &edit.message_id,
                edit.content.as_deref(),
                edit.embed,
                edit.mentions,
            )
            .await;
        (timer.finish(SubActionOutcome::from_result(&edited)), None)
    }
}
