use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
    VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

const INTO_VAR_KEY: &str = "into_var";

#[derive(Clone, Copy)]
enum TrimSide {
    Both,
    Left,
    Right,
}

impl TrimSide {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn apply(self, source: &str) -> &str {
        match self {
            Self::Both => source.trim(),
            Self::Left => source.trim_start(),
            Self::Right => source.trim_end(),
        }
    }
}

impl ConfigChoice for TrimSide {
    const ALL: &'static [Self] = &[Self::Both, Self::Left, Self::Right];
    const WIRE_NAMES: &'static [&'static str] = &[
        Self::Both.as_str(),
        Self::Left.as_str(),
        Self::Right.as_str(),
    ];

    fn wire_name(self) -> &'static str {
        self.as_str()
    }
}

const SOURCE: TextField = TextField {
    key: "source",
    label: "Source",
    editor: TextEditor::Line {
        placeholder: "  hello  ",
    },
    default: "",
    required: false,
};

const MODE: ChoiceField<TrimSide> = ChoiceField {
    key: "mode",
    label: "Mode",
    default: TrimSide::Both,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: "string.result",
    default: "string.result",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &MODE, &INTO_VAR]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, TrimSide, String), ConfigProblem> {
    Ok((
        SOURCE.read(config, args)?,
        MODE.read(config, args)?,
        INTO_VAR.read(config, args)?,
    ))
}

pub struct CoreStringTrimRunner;

#[async_trait]
impl SubActionRunner for CoreStringTrimRunner {
    fn id(&self) -> &str {
        "core.string.trim"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Trim Whitespace"
    }

    fn summary(&self) -> &str {
        "Remove leading and/or trailing whitespace from a string"
    }

    fn search_text(&self) -> &str {
        "string trim strip whitespace leading trailing both"
    }

    fn icon_name(&self) -> &str {
        "text-wrap"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Trimmed string".to_owned(),
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
        let (source, side, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let result = side.apply(&source).to_owned();
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
        CoreStringTrimRunner.execute(cfg, &ctx).await.1
    }

    #[tokio::test]
    async fn trim_mode_selects_which_sides_are_stripped() {
        for (mode, expected) in [("both", "hi"), ("left", "hi  "), ("right", "  hi")] {
            let mut cfg = SubActionConfig::new();
            cfg.insert("source".to_owned(), Variant::String("  hi  ".to_owned()));
            cfg.insert("mode".to_owned(), Variant::String(mode.to_owned()));
            let out = run(&cfg, &ArgStack::new()).await.unwrap();
            assert_eq!(
                out.get("string.result").and_then(|v| v.as_str()),
                Some(expected),
                "mode {mode}"
            );
        }
    }
}
