use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert("separator".to_owned(), Variant::String(",".to_owned()));
        cfg.insert("trim_each".to_owned(), Variant::Bool(false));
        cfg.insert("max_parts".to_owned(), Variant::Int(0));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("string.parts".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source",
                placeholder: "a, b, c",
            },
            FormField::Text {
                key: "separator",
                label: "Separator",
                placeholder: ",",
            },
            FormField::Toggle {
                key: "trim_each",
                label: "Trim Each Part",
            },
            FormField::Integer {
                key: "max_parts",
                label: "Max Parts (0 = unlimited)",
                min: 0,
                max: i64::MAX,
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "string.parts",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
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
        let timer = StepTimer::start(ctx, "core.string.split");

        let source = ctx
            .arg_stack
            .interpolate(config.str("source").unwrap_or(""));
        let separator = ctx
            .arg_stack
            .interpolate(config.str("separator").unwrap_or(","));
        let trim_each = config.bool("trim_each").unwrap_or(false);
        let max_parts = usize::try_from(config.int("max_parts").unwrap_or(0)).unwrap_or(0);
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("string.parts"),
        );

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
