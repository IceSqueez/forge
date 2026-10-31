use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY, source_field};

#[derive(Clone, Copy)]
enum LengthMode {
    Chars,
    Bytes,
}

impl LengthMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Chars => "chars",
            Self::Bytes => "bytes",
        }
    }

    fn measure(self, source: &str) -> usize {
        match self {
            Self::Chars => source.chars().count(),
            Self::Bytes => source.len(),
        }
    }
}

impl ConfigChoice for LengthMode {
    const ALL: &'static [Self] = &[Self::Chars, Self::Bytes];
    const WIRE_NAMES: &'static [&'static str] = &[Self::Chars.as_str(), Self::Bytes.as_str()];

    fn wire_name(self) -> &'static str {
        self.as_str()
    }
}

const SOURCE: TextField = source_field("Hello");

const MODE: ChoiceField<LengthMode> = ChoiceField {
    key: "mode",
    label: "Mode",
    default: LengthMode::Chars,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &MODE, &INTO_RESULT]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, LengthMode, String), ConfigProblem> {
    Ok((
        SOURCE.read(config, args)?,
        MODE.read(config, args)?,
        INTO_RESULT.read(config, args)?,
    ))
}

pub struct CoreStringLengthRunner;

#[async_trait]
impl SubActionRunner for CoreStringLengthRunner {
    fn id(&self) -> &str {
        "core.string.length"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Length"
    }

    fn summary(&self) -> &str {
        "Get the length of a string in bytes or Unicode scalar values (chars)"
    }

    fn search_text(&self) -> &str {
        "string length size count chars bytes"
    }

    fn icon_name(&self) -> &str {
        "ruler"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Int,
                label: "String length".to_owned(),
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
        let (source, mode, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let length = i64::try_from(mode.measure(&source)).unwrap_or(i64::MAX);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Int(length));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;

    async fn length_of(mode: &str) -> i64 {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("Привіт".to_owned()));
        cfg.insert("mode".to_owned(), Variant::String(mode.to_owned()));
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringLengthRunner.execute(&cfg, &ctx).await.1.unwrap();
        out.get("string.result").and_then(|v| v.as_int()).unwrap()
    }

    #[tokio::test]
    async fn length_bytes_mode_exceeds_char_count_for_non_ascii() {
        assert_eq!(length_of("chars").await, 6);
        assert_eq!(length_of("bytes").await, 12);
    }
}
