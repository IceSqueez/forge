use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, TypedField, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::INTO_VAR_KEY;

const FORMATTED_VARIABLE: &str = "string.formatted";

const TEMPLATE: TextField = TextField {
    key: "template",
    label: "Template",
    editor: TextEditor::Area,
    default: "",
    required: false,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: FORMATTED_VARIABLE,
    default: FORMATTED_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TEMPLATE, &INTO_VAR]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((TEMPLATE.read(config, args)?, INTO_VAR.read(config, args)?))
}

pub struct CoreStringFormatRunner;

#[async_trait]
impl SubActionRunner for CoreStringFormatRunner {
    fn id(&self) -> &str {
        "core.string.format"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Format Template"
    }

    fn summary(&self) -> &str {
        "Compose a string from a template with %variable% placeholders"
    }

    fn search_text(&self) -> &str {
        "string format template compose placeholder variable"
    }

    fn icon_name(&self) -> &str {
        "template"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Formatted string".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (formatted, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::String(formatted));

        (timer.success(), Some(new_stack))
    }
}
