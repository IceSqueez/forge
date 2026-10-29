use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, PickedTextField, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::ObsSink;
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::BROWSER_REFRESH;

const SOURCE: PickedTextField = PickedTextField {
    key: config_keys::SOURCE,
    label: "Browser Source",
    picker: TextPicker::Options {
        options_key: picker_keys::INPUT_NAMES,
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE]);

pub struct BrowserRefreshRunner {
    sink: Arc<dyn ObsSink>,
}

impl BrowserRefreshRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for BrowserRefreshRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Refresh Browser Source"
    }

    fn summary(&self) -> &str {
        "Refreshes a browser source, bypassing its cache."
    }

    fn search_text(&self) -> &str {
        "obs browser source refresh reload cache nocache"
    }

    fn icon_name(&self) -> &str {
        "refresh"
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
        let source = match SOURCE.read(config, ctx.arg_stack) {
            Ok(source) => source,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome =
            SubActionOutcome::from_result(&self.sink.refresh_browser_source(&source).await);
        (timer.finish(outcome), None)
    }
}
