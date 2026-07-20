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
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

    fn runner() -> StreamSendCaptionRunner {
        StreamSendCaptionRunner::new(Arc::new(MockSink))
    }

    fn config_with(text: &str) -> SubActionConfig {
        BTreeMap::from([("caption_text".to_owned(), Variant::String(text.to_owned()))])
    }

    #[test]
    fn validate_config_accepts_non_empty_caption() {
        assert!(
            runner()
                .validate_config(&config_with("Hello world"))
                .is_ok()
        );
    }

    #[test]
    fn validate_config_rejects_empty_or_whitespace_caption() {
        for blank in ["", "   ", "\t", "\n", " \t\n "] {
            assert!(
                runner().validate_config(&config_with(blank)).is_err(),
                "expected reject for {blank:?}"
            );
        }
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let (tel, extra) = runner()
            .execute(&config_with("On screen now"), &make_ctx(&stack))
            .await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.stream.send_caption");
        assert!(extra.is_none());
    }
}
