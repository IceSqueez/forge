use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY, read_source_into, source_field};

const SOURCE: TextField = source_field("hello world");

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &INTO_RESULT]);

pub struct CoreStringTitlecaseRunner;

#[async_trait]
impl SubActionRunner for CoreStringTitlecaseRunner {
    fn id(&self) -> &str {
        "core.string.titlecase"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Titlecase"
    }

    fn summary(&self) -> &str {
        "Capitalize the first character of each whitespace-delimited word"
    }

    fn search_text(&self) -> &str {
        "string titlecase title case capitalize word"
    }

    fn icon_name(&self) -> &str {
        "letter-case"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Title-cased string".to_owned(),
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
        let (source, into_var) = match read_source_into(&SOURCE, config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let result = to_titlecase(&source);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        (timer.success(), Some(new_stack))
    }
}
fn to_titlecase(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_whitespace() {
            result.push(c);
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.extend(c.to_lowercase());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;

    #[tokio::test]
    async fn titlecase_capitalizes_only_after_whitespace_not_hyphen_or_underscore() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "source".to_owned(),
            Variant::String("foo-bar baz_qux".to_owned()),
        );
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringTitlecaseRunner
            .execute(&cfg, &ctx)
            .await
            .1
            .unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("Foo-bar Baz_qux")
        );
    }
}
