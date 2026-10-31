use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, LinesField, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY};

const PARTS: LinesField = LinesField {
    key: "parts",
    label: "Parts (one per line)",
};

const SEPARATOR: TextField = TextField {
    key: "separator",
    label: "Separator",
    editor: TextEditor::Line { placeholder: "" },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PARTS, &SEPARATOR, &INTO_RESULT]);

struct ConcatConfig {
    parts: Vec<String>,
    separator: String,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<ConcatConfig, ConfigProblem> {
    let parts = PARTS
        .read(config, args)?
        .iter()
        .map(|part| args.interpolate(part.as_str().unwrap_or_default()))
        .collect();
    Ok(ConcatConfig {
        parts,
        separator: SEPARATOR.read(config, args)?,
        into_var: INTO_RESULT.read(config, args)?,
    })
}

pub struct CoreStringConcatRunner;

#[async_trait]
impl SubActionRunner for CoreStringConcatRunner {
    fn id(&self) -> &str {
        "core.string.concat"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Concat"
    }

    fn summary(&self) -> &str {
        "Join a list of strings with an optional separator"
    }

    fn search_text(&self) -> &str {
        "string concat join concatenate parts separator"
    }

    fn icon_name(&self) -> &str {
        "text-plus"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Concatenated string".to_owned(),
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
        let ConcatConfig {
            parts,
            separator,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let result = parts.join(&separator);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;

    async fn run(cfg: &SubActionConfig, stack: &ArgStack) -> Option<ArgStack> {
        let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
        CoreStringConcatRunner.execute(cfg, &ctx).await.1
    }

    #[tokio::test]
    async fn concat_joins_array_parts_with_separator_into_result_var() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "parts".to_owned(),
            Variant::Array(vec![
                Variant::String("a".to_owned()),
                Variant::String("b".to_owned()),
                Variant::String("c".to_owned()),
            ]),
        );
        cfg.insert("separator".to_owned(), Variant::String("-".to_owned()));
        let out = run(&cfg, &ArgStack::new()).await.unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("a-b-c")
        );
    }
}
