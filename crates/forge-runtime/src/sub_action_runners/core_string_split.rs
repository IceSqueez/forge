use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, IntegerEditor, IntegerField, ProducedVariable, RunContext,
    StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField, ToggleField,
    TypedField, ValueBounds, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::{INTO_VAR_KEY, source_field};

const PARTS_VARIABLE: &str = "string.parts";
const DEFAULT_SEPARATOR: &str = ",";

const SOURCE: TextField = source_field("a, b, c");

const SEPARATOR: TextField = TextField {
    key: "separator",
    label: "Separator",
    editor: TextEditor::Line {
        placeholder: DEFAULT_SEPARATOR,
    },
    default: DEFAULT_SEPARATOR,
    required: false,
};

const TRIM_EACH: ToggleField = ToggleField {
    key: "trim_each",
    label: "Trim Each Part",
    default: false,
};

const MAX_PARTS: IntegerField = IntegerField {
    key: "max_parts",
    label: "Max Parts (0 = unlimited)",
    editor: IntegerEditor::Number,
    default: 0,
    bounds: ValueBounds {
        min: Some(0),
        max: None,
    },
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: PARTS_VARIABLE,
    default: PARTS_VARIABLE,
};

const CONFIG: ConfigSpec =
    ConfigSpec::new(&[&SOURCE, &SEPARATOR, &TRIM_EACH, &MAX_PARTS, &INTO_VAR]);

struct SplitConfig {
    source: String,
    separator: String,
    trim_each: bool,
    max_parts: usize,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<SplitConfig, ConfigProblem> {
    Ok(SplitConfig {
        source: SOURCE.read(config, args)?,
        separator: SEPARATOR.read(config, args)?,
        trim_each: TRIM_EACH.read(config, args)?,
        max_parts: usize::try_from(MAX_PARTS.read(config, args)?).unwrap_or(usize::MAX),
        into_var: INTO_VAR.read(config, args)?,
    })
}

pub struct CoreStringSplitRunner;

#[async_trait]
impl SubActionRunner for CoreStringSplitRunner {
    fn id(&self) -> &str {
        "core.string.split"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Split"
    }

    fn summary(&self) -> &str {
        "Split a string into an array of parts by a separator"
    }

    fn search_text(&self) -> &str {
        "string split array parts separator delimiter"
    }

    fn icon_name(&self) -> &str {
        "scissors"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Array,
                label: "Split parts".to_owned(),
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
        let SplitConfig {
            source,
            separator,
            trim_each,
            max_parts,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let raw_parts: Vec<&str> = if separator.is_empty() {
            split_chars(&source, max_parts)
        } else if max_parts > 0 {
            source.splitn(max_parts, separator.as_str()).collect()
        } else {
            source.split(separator.as_str()).collect()
        };

        let parts: Vec<Variant> = raw_parts
            .into_iter()
            .map(|p| {
                let s = if trim_each { p.trim() } else { p };
                Variant::String(s.to_owned())
            })
            .collect();

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Array(parts));
        (timer.success(), Some(new_stack))
    }
}

fn split_chars(source: &str, max_parts: usize) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = source;
    while let Some(first) = rest.chars().next() {
        if max_parts > 0 && parts.len() + 1 == max_parts {
            parts.push(rest);
            break;
        }
        let (head, tail) = rest.split_at(first.len_utf8());
        parts.push(head);
        rest = tail;
    }
    parts
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

    async fn split_parts(cfg: &SubActionConfig) -> Vec<String> {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringSplitRunner.execute(cfg, &ctx).await.1.unwrap();
        match out.get("string.parts") {
            Some(Variant::Array(items)) => items
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
            other => panic!("expected Variant::Array under string.parts, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn split_max_parts_caps_count_keeping_remainder_in_final_part() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("a,b,c,d".to_owned()));
        cfg.insert("max_parts".to_owned(), Variant::Int(2));
        assert_eq!(split_parts(&cfg).await, vec!["a", "b,c,d"]);
    }

    #[tokio::test]
    async fn split_max_parts_zero_means_unlimited() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("a,b,c,d".to_owned()));
        cfg.insert("max_parts".to_owned(), Variant::Int(0));
        assert_eq!(split_parts(&cfg).await, vec!["a", "b", "c", "d"]);
    }

    fn empty_separator_cfg(source: &str, max_parts: i64) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(source.to_owned()));
        cfg.insert("separator".to_owned(), Variant::String(String::new()));
        cfg.insert("max_parts".to_owned(), Variant::Int(max_parts));
        cfg
    }

    #[tokio::test]
    async fn split_empty_separator_yields_one_part_per_char() {
        for (source, expected) in [("Hi", vec!["H", "i"]), ("При", vec!["П", "р", "и"])] {
            assert_eq!(
                split_parts(&empty_separator_cfg(source, 0)).await,
                expected,
                "{source}"
            );
        }
    }

    #[tokio::test]
    async fn split_empty_separator_honours_max_parts_keeping_remainder_in_final_part() {
        for (max_parts, expected) in [
            (1, vec!["abc"]),
            (2, vec!["a", "bc"]),
            (3, vec!["a", "b", "c"]),
            (4, vec!["a", "b", "c"]),
        ] {
            assert_eq!(
                split_parts(&empty_separator_cfg("abc", max_parts)).await,
                expected,
                "max_parts {max_parts}"
            );
        }
    }

    #[tokio::test]
    async fn split_empty_separator_on_empty_source_yields_no_parts() {
        assert!(split_parts(&empty_separator_cfg("", 2)).await.is_empty());
    }

    #[tokio::test]
    async fn split_interpolates_the_separator() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("a;b;c".to_owned()));
        cfg.insert("separator".to_owned(), Variant::String("%sep%".to_owned()));
        let stack = ArgStack::new().set("sep".to_owned(), Variant::String(";".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringSplitRunner.execute(&cfg, &ctx).await.1.unwrap();
        assert_eq!(
            out.get("string.parts"),
            Some(&Variant::Array(
                ["a", "b", "c"]
                    .map(|part| Variant::String(part.to_owned()))
                    .to_vec()
            ))
        );
    }
}
