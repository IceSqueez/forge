use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct StreamSendCaptionRunner {
    sink: Arc<dyn ObsSink>,
}

impl StreamSendCaptionRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for StreamSendCaptionRunner {
    fn id(&self) -> &str {
        "obs.stream.send_caption"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Send Stream Caption"
    }

    fn summary(&self) -> &str {
        "Sends CEA-608 caption text over the OBS stream output."
    }

    fn search_text(&self) -> &str {
        "obs stream caption subtitle text cea608 accessibility"
    }

    fn icon_name(&self) -> &str {
        "subtitles"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("caption_text".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "caption_text",
            label: "Caption Text",
            placeholder: "Caption text to send over stream",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("caption_text") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            Some(Variant::String(_)) => Err(RegistryError::UnknownKindId(
                "obs.stream.send_caption: 'caption_text' must not be empty".to_owned(),
            )),
            _ => Err(RegistryError::UnknownKindId(
                "obs.stream.send_caption: 'caption_text' must be a string".to_owned(),
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

        let raw = config
            .get("caption_text")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let text = ctx.arg_stack.interpolate(raw);

        let outcome = match self.sink.send_stream_caption(&text).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.stream.send_caption".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
