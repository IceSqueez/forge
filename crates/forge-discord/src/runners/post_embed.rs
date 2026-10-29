use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, DeclaredField, Interpolation, OptionalField,
    RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::{WEBHOOK, optional_text, present, unlisted_text};
use super::{message_id_scope_io, message_id_stack};
use crate::embed::DiscordEmbed;
use crate::sink::DiscordSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::WEBHOOK_SEND_EMBED;

const TITLE: TextField = optional_text(
    config_keys::EMBED_TITLE,
    "Title",
    TextEditor::Line {
        placeholder: "Stream is live",
    },
);

const DESCRIPTION: TextField = optional_text(
    config_keys::EMBED_DESCRIPTION,
    "Description",
    TextEditor::Area,
);

const FOOTER_TEXT: OptionalField<TextField> =
    unlisted_text(config_keys::EMBED_FOOTER_TEXT, "Footer");
const AUTHOR_NAME: OptionalField<TextField> =
    unlisted_text(config_keys::EMBED_AUTHOR_NAME, "Author");
const THUMBNAIL_URL: OptionalField<TextField> =
    unlisted_text(config_keys::EMBED_THUMBNAIL_URL, "Thumbnail URL");
const IMAGE_URL: OptionalField<TextField> =
    unlisted_text(config_keys::EMBED_IMAGE_URL, "Image URL");

const UNLISTED: [&dyn DeclaredField; 4] = [&FOOTER_TEXT, &AUTHOR_NAME, &THUMBNAIL_URL, &IMAGE_URL];

const CONFIG: ConfigSpec = ConfigSpec::new(&[&WEBHOOK, &TITLE, &DESCRIPTION]);

fn no_content() -> ConfigProblem {
    ConfigProblem::whole(ConfigReason::any_of([
        config_keys::EMBED_TITLE,
        config_keys::EMBED_DESCRIPTION,
    ]))
}

struct EmbedPost {
    webhook: String,
    embed: DiscordEmbed,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<EmbedPost, ConfigProblem> {
    let webhook = WEBHOOK.read(config, args)?;
    let title = present(TITLE.read(config, args)?);
    let description = present(DESCRIPTION.read(config, args)?);
    if title.is_none() && description.is_none() {
        return Err(no_content());
    }
    Ok(EmbedPost {
        webhook,
        embed: DiscordEmbed {
            title,
            description,
            footer_text: FOOTER_TEXT.read(config, args)?.and_then(present),
            author_name: AUTHOR_NAME.read(config, args)?.and_then(present),
            thumbnail_url: THUMBNAIL_URL.read(config, args)?.and_then(present),
            image_url: IMAGE_URL.read(config, args)?.and_then(present),
            ..Default::default()
        },
    })
}

pub struct PostEmbedRunner {
    sink: Arc<dyn DiscordSink>,
}

impl PostEmbedRunner {
    pub fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for PostEmbedRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Discord
    }

    fn label(&self) -> &str {
        "Post Embed"
    }

    fn summary(&self) -> &str {
        "Posts a rich embed message to a Discord webhook."
    }

    fn search_text(&self) -> &str {
        "discord webhook post embed rich card"
    }

    fn icon_name(&self) -> &str {
        "layout-cards"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        UNLISTED
            .iter()
            .try_for_each(|field| field.check(config, interpolation))?;
        let title = TITLE.peek(config, interpolation)?;
        let description = DESCRIPTION.peek(config, interpolation)?;
        if title.is_some_and(|text| text.is_empty())
            && description.is_some_and(|text| text.is_empty())
        {
            return Err(no_content());
        }
        Ok(())
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
        if let Err(error) = post.embed.validate() {
            return (timer.failed(error.to_string()), None);
        }
        let posted = self.sink.post_embed(&post.webhook, post.embed).await;
        let produced = message_id_stack(ctx.arg_stack, &posted);
        (
            timer.finish(SubActionOutcome::from_result(&posted)),
            produced,
        )
    }
}
