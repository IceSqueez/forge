use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::STREAMING_SEND_CAPTION;

const CAPTION_TEXT: TextField = TextField {
    key: config_keys::CAPTION_TEXT,
    label: "Caption Text",
    editor: TextEditor::Line {
        placeholder: "Caption text to send over stream",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CAPTION_TEXT]);

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
        KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let text = match CAPTION_TEXT.read(config, ctx.arg_stack) {
            Ok(text) => text,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.send_stream_caption(&text).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner() -> StreamSendCaptionRunner {
        StreamSendCaptionRunner::new(Arc::new(MockSink))
    }

    fn config_with(text: &str) -> SubActionConfig {
        BTreeMap::from([("caption_text".to_owned(), Variant::String(text.to_owned()))])
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let (tel, extra) = runner()
            .execute(&config_with("On screen now"), &make_ctx(&stack))
            .await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.streaming.send_caption");
        assert!(extra.is_none());
    }
}
