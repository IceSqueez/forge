use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TextEditor,
    TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry, UserText, Variant};

use crate::ObsSink;
use crate::vocabulary::fields::query;
use crate::vocabulary::{config_keys, sub_action_ids, variables};

const KIND_ID: &str = sub_action_ids::SOURCE_GET_INPUT_SETTINGS;

const SOURCE: TextField = TextField {
    key: config_keys::SOURCE,
    label: "Input name",
    editor: TextEditor::Line {
        placeholder: "e.g. Webcam",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE]);

pub struct QueryInputSettingsRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryInputSettingsRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryInputSettingsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Input Settings"
    }

    fn summary(&self) -> &str {
        "Queries OBS for the current settings object of a named input source."
    }

    fn search_text(&self) -> &str {
        "obs source input get settings properties kind"
    }

    fn icon_name(&self) -> &str {
        "settings"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                variables::input_query::SETTINGS.to_owned(),
                variables::input_query::KIND.to_owned(),
            ],
            ..SubActionIo::default()
        }
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

        match self.sink.get_input_settings(&source).await {
            Ok(variant) => {
                let mut stack = ctx.arg_stack.clone();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get(query::SETTINGS) {
                        stack = stack.set(variables::input_query::SETTINGS.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(query::KIND) {
                        stack = stack.set(variables::input_query::KIND.to_owned(), v.clone());
                    }
                }
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.failed(UserText::unlocalized(e.to_string())), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    fn runner() -> QueryInputSettingsRunner {
        QueryInputSettingsRunner::new(Arc::new(MockSink))
    }

    #[tokio::test]
    async fn execute_populates_kind_and_settings_object_from_sink() {
        let config = BTreeMap::from([("source".to_owned(), Variant::String("Caption".to_owned()))]);
        let empty = ArgStack::new();
        let (telemetry, stack) = runner().execute(&config, &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.input.kind"),
            Some(&Variant::String("text_ft2_source_v2".to_owned())),
        );
        let expected_settings = Variant::Object(BTreeMap::from([(
            "text".to_owned(),
            Variant::String("hello".to_owned()),
        )]));
        assert_eq!(stack.get("obs.input.settings"), Some(&expected_settings));
    }
}
