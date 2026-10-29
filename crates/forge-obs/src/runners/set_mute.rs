use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;
use crate::vocabulary::{picker_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::AUDIO_SET_MUTE;

pub struct SetMuteRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetMuteRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetMuteRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Input Mute"
    }

    fn summary(&self) -> &str {
        "Mutes or unmutes an OBS audio input."
    }

    fn search_text(&self) -> &str {
        "obs audio mute unmute input source volume"
    }

    fn icon_name(&self) -> &str {
        "volume"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String(String::new())),
            ("muted".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "source",
                label: "Input",
                options_key: picker_keys::INPUT_NAMES,
            },
            FormField::Toggle {
                key: "muted",
                label: "Muted",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("source") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            Some(Variant::String(_)) => Err(RegistryError::invalid_field(
                "source",
                ConfigReason::Required,
            )),
            _ => Err(RegistryError::invalid_field(
                "source",
                ConfigReason::Required,
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw = config.str("source").unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw);
        let muted = matches!(config.get("muted"), Some(Variant::Bool(true)));

        let outcome =
            SubActionOutcome::from_result(&self.sink.set_input_mute(&source, muted).await);

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
