use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, Interpolation, RunContext, StepCatalogEntry,
    StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{MESSAGE_ID, WEBHOOK, optional_text, present};
use super::mention_fields::{ALLOW_EVERYONE_PINGS, ALLOW_ROLE_PINGS, read_mention_policy};
use super::step_failure;
use crate::mention::MentionPolicy;
use crate::sink::DiscordSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const CONTENT: TextField = optional_text(config_keys::CONTENT, "New Content", TextEditor::Area);

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &WEBHOOK,
    &MESSAGE_ID,
    &CONTENT,
    &ALLOW_ROLE_PINGS,
    &ALLOW_EVERYONE_PINGS,
]);

fn no_content() -> ConfigProblem {
    ConfigProblem::whole(ConfigReason::any_of([config_keys::CONTENT]))
}

struct MessageEdit {
    webhook: String,
    message_id: String,
    content: String,
    mentions: MentionPolicy,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<MessageEdit, ConfigProblem> {
    let webhook = WEBHOOK.read(config, args)?;
    let message_id = MESSAGE_ID.read(config, args)?;
    let content = present(CONTENT.read(config, args)?).ok_or_else(no_content)?;
    Ok(MessageEdit {
        webhook,
        message_id,
        content,
        mentions: read_mention_policy(config, args)?,
    })
}

pub(crate) struct EditMessageRunner {
    sink: Arc<dyn DiscordSink>,
}

impl EditMessageRunner {
    pub(crate) fn new(sink: Arc<dyn DiscordSink>) -> Self {
        Self { sink }
    }
}

impl StepCatalogEntry for EditMessageRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
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
}

#[async_trait]
impl SubActionRunner for EditMessageRunner {
    fn id(&self) -> &str {
        sub_action_ids::WEBHOOK_UPDATE_MESSAGE
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
        if content.is_some_and(|text| present(text).is_none()) {
            return Err(no_content());
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let edit = match read_config(config, ctx.arg_stack) {
            Ok(edit) => edit,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let edited = self
            .sink
            .edit_message(
                &edit.webhook,
                &edit.message_id,
                Some(&edit.content),
                None,
                edit.mentions,
            )
            .await;
        (timer.finish(step_failure::outcome(&edited)), None)
    }
}

#[cfg(test)]
mod tests {
    use forge_types::Variant;

    use super::*;

    const SNOWFLAKE: &str = "123456789012345678";

    fn edit(content: &str) -> Result<MessageEdit, ConfigProblem> {
        let config: SubActionConfig = [
            (config_keys::WEBHOOK_NAME, "alerts"),
            (config_keys::MESSAGE_ID, SNOWFLAKE),
            (config_keys::CONTENT, content),
        ]
        .iter()
        .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
        .collect();
        read_config(&config, &ArgStack::new())
    }

    #[test]
    fn an_edit_without_content_is_refused() {
        for content in ["", "  ", "\n"] {
            let refused = edit(content).err().expect("nothing to edit");
            assert!(matches!(refused.reason, ConfigReason::AnyOfRequired(_)));
        }
    }

    #[test]
    fn an_edit_carries_its_content() {
        let edited = edit("new text").expect("content is an edit");
        assert_eq!(edited.content, "new text");
    }
}
