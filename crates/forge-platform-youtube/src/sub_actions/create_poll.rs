use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::send_chat::YoutubeSendChat;

const KIND_ID: &str = "youtube.chat.create_poll";
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 4;

pub struct CreatePollRunner {
    sender: Arc<YoutubeSendChat>,
}

impl CreatePollRunner {
    pub fn new(sender: Arc<YoutubeSendChat>) -> Self {
        Self { sender }
    }
}

/// Returns Err if the option count falls outside `[MIN_OPTIONS, MAX_OPTIONS]`.
fn parse_options(raw: &str) -> Result<Vec<String>, String> {
    let options: Vec<String> = raw
        .split(['\n', ','])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if options.len() < MIN_OPTIONS {
        return Err(format!("at least {MIN_OPTIONS} options required"));
    }
    if options.len() > MAX_OPTIONS {
        return Err(format!(
            "too many options: max {MAX_OPTIONS}, got {}",
            options.len()
        ));
    }
    Ok(options)
}

#[async_trait]
impl SubActionRunner for CreatePollRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::PollsPredictions
    }

    fn label(&self) -> &str {
        "Create Live Poll"
    }

    fn summary(&self) -> &str {
        "Posts a poll to the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube poll vote create question options live chat"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("question".to_owned(), Variant::String(String::new())),
            ("options".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "question",
                label: "Question",
                placeholder: "What next?",
            },
            FormField::TextArea {
                key: "options",
                label: "Options (one per line, 2-4)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("question") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'question' is required"
                )));
            }
        }

        let raw_options = match config.get("options") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        parse_options(raw_options)
            .map(|_| ())
            .map_err(|msg| RegistryError::InvalidConfig(format!("{KIND_ID}: {msg}")))
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let question_template = config.str("question").unwrap_or_default();
        let question = ctx.arg_stack.interpolate(question_template);

        if question.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "question is empty after interpolation".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        let options_template = config.str("options").unwrap_or_default();
        let raw_options = ctx.arg_stack.interpolate(options_template);

        let options = match parse_options(&raw_options) {
            Ok(v) => v,
            Err(msg) => {
                return (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Failed(msg),
                        index: ctx.index,
                    },
                    None,
                );
            }
        };

        let outcome =
            SubActionOutcome::from_result(&self.sender.create_poll(&question, &options).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
