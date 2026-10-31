use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{WEBHOOK, filled_text};
use super::mention_fields::{ALLOW_EVERYONE_PINGS, ALLOW_ROLE_PINGS, read_mention_policy};
use super::step_failure;
use super::{message_id_scope_io, message_id_stack};
use crate::mention::MentionPolicy;
use crate::sink::DiscordSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::WEBHOOK_SEND_MESSAGE;

const CONTENT: TextField = filled_text(config_keys::CONTENT, "Message", TextEditor::Area);

const CONFIG: ConfigSpec =
    ConfigSpec::new(&[&WEBHOOK, &CONTENT, &ALLOW_ROLE_PINGS, &ALLOW_EVERYONE_PINGS]);

struct TextPost {
    webhook: String,
    content: String,
    mentions: MentionPolicy,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<TextPost, ConfigProblem> {
    Ok(TextPost {
        webhook: WEBHOOK.read(config, args)?,
        content: CONTENT.read(config, args)?,
        mentions: read_mention_policy(config, args)?,
    })
}

pub(crate) struct PostTextRunner {
    sink: Arc<dyn DiscordSink>,
}

impl PostTextRunner {
    pub(crate) fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for PostTextRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Post Text"
    }

    fn summary(&self) -> &str {
        "Posts a text message to a Discord webhook."
    }

    fn search_text(&self) -> &str {
        "discord webhook post message text send"
    }

    fn icon_name(&self) -> &str {
        "message"
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
        let post = match read_config(config, ctx.arg_stack) {
            Ok(post) => post,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let posted = self
            .sink
            .post_text(&post.webhook, &post.content, post.mentions)
            .await;
        let produced = message_id_stack(ctx.arg_stack, &posted);
        (timer.finish(step_failure::outcome(&posted)), produced)
    }
}
