use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, PickedTextField, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextPicker, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};

const EXPRESSION_FILE: PickedTextField = PickedTextField {
    key: config_keys::EXPRESSION_FILE,
    label: "Expression",
    picker: TextPicker::Options {
        options_key: picker_keys::EXPRESSION_FILES,
    },
    default: "",
    required: true,
};

const ACTIVE: ToggleField = ToggleField {
    key: config_keys::ACTIVE,
    label: "Active",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&EXPRESSION_FILE, &ACTIVE]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, bool), ConfigProblem> {
    Ok((
        EXPRESSION_FILE.read(config, args)?,
        ACTIVE.read(config, args)?,
    ))
}

pub(crate) struct ExpressionSetRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ExpressionSetRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ExpressionSetRunner {
    fn id(&self) -> &str {
        sub_action_ids::EXPRESSION_SET
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Set Expression"
    }

    fn summary(&self) -> &str {
        "Activates or deactivates a VTube Studio expression."
    }

    fn search_text(&self) -> &str {
        "vtube expression activate toggle face vts"
    }

    fn icon_name(&self) -> &str {
        "mood-smile"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (expression_file, active) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome =
            step_failure::outcome(&self.sink.set_expression(&expression_file, active).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    #[test]
    fn validate_config_accepts_expression_string() {
        let runner = ExpressionSetRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([
            (
                "expression_file".to_owned(),
                Variant::String("smile.exp3.json".to_owned()),
            ),
            ("active".to_owned(), Variant::Bool(true)),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_expression_file() {
        let runner = ExpressionSetRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[tokio::test]
    async fn execute_interpolates_expression_file() {
        let sink = Arc::new(MockSink::new());
        let runner = ExpressionSetRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new().set(
            "expr".to_owned(),
            Variant::String("blink.exp3.json".to_owned()),
        );
        let config = BTreeMap::from([
            (
                "expression_file".to_owned(),
                Variant::String("%expr%".to_owned()),
            ),
            ("active".to_owned(), Variant::Bool(true)),
        ]);
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
        assert_eq!(
            sink.strings_sent_to("set_expression"),
            Some(vec!["blink.exp3.json".to_owned()]),
        );
    }
}
