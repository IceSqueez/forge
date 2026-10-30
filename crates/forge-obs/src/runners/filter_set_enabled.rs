use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::runners::fields::SOURCE;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::FILTER_SET_ENABLED;

const FILTER: TextField = TextField {
    key: config_keys::FILTER,
    label: "Filter Name",
    editor: TextEditor::Line {
        placeholder: "e.g. Color Correction",
    },
    default: "",
    required: true,
};

const ENABLED: ToggleField = ToggleField {
    key: config_keys::ENABLED,
    label: "Enabled",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &FILTER, &ENABLED]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String, bool), ConfigProblem> {
    Ok((
        SOURCE.read(config, args)?,
        FILTER.read(config, args)?,
        ENABLED.read(config, args)?,
    ))
}

pub struct FilterSetEnabledRunner {
    sink: Arc<dyn ObsSink>,
}

impl FilterSetEnabledRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for FilterSetEnabledRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Filter Enabled"
    }

    fn summary(&self) -> &str {
        "Enables or disables a filter on an OBS source."
    }

    fn search_text(&self) -> &str {
        "obs filter enable disable toggle source effect"
    }

    fn icon_name(&self) -> &str {
        "filter"
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
        let (source, filter, enabled) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(
            &self
                .sink
                .set_source_filter_enabled(&source, &filter, enabled)
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{RecordingSink, make_ctx};
    use forge_types::Variant;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_forwards_the_interpolated_source_filter_and_enabled_flag() {
        for enabled in [true, false] {
            let sink = RecordingSink::new();
            let runner = FilterSetEnabledRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
            let stack = ArgStack::new().set("cam".to_owned(), Variant::String("Webcam".to_owned()));
            let config = BTreeMap::from([
                ("source".to_owned(), Variant::String("%cam%".to_owned())),
                (
                    "filter".to_owned(),
                    Variant::String("Chroma Key".to_owned()),
                ),
                ("enabled".to_owned(), Variant::Bool(enabled)),
            ]);

            runner.execute(&config, &make_ctx(&stack)).await;

            assert_eq!(
                sink.calls(),
                vec![format!(
                    "set_source_filter_enabled(Webcam,Chroma Key,{enabled})"
                )],
            );
        }
    }
}
