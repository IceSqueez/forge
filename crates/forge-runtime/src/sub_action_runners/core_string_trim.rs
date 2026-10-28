use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::config_choice::{ConfigChoice, read_choice};

const MODE_KEY: &str = "mode";

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            MODE_KEY.to_owned(),
            Variant::String(TrimSide::Both.as_str().to_owned()),
        );
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("string.result".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source",
                placeholder: "  hello  ",
            },
            FormField::Select {
                key: MODE_KEY,
                label: "Mode",
                options: TrimSide::WIRE_NAMES,
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "string.result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        read_choice(config, MODE_KEY, TrimSide::Both).map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
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
        let timer = StepTimer::start(ctx, "core.string.trim");

        let source = ctx
            .arg_stack
            .interpolate(config.str("source").unwrap_or(""));
        let side = match read_choice(config, MODE_KEY, TrimSide::Both) {
            Ok(side) => side,
            Err(e) => return (timer.failed(e.to_string()), None),
        };
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("string.result"),
        );

        let result = side.apply(&source).to_owned();

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

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
