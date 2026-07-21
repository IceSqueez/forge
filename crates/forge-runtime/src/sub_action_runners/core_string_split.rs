use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, SubActionCategory, SubActionIo,
    SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use time::OffsetDateTime;

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
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let source = config.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let separator = config
            .get("separator")
            .and_then(|v| v.as_str())
            .unwrap_or(",");
        let trim_each = config
            .get("trim_each")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_parts = config
            .get("max_parts")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(0) as usize;
        let into_var = super::interpolate::sanitize_var_name(
            config
                .get("into_var")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("string.parts"),
        );

        let raw_parts: Vec<&str> = if separator.is_empty() {
            source.split("").collect()
        } else if max_parts > 0 {
            source.splitn(max_parts, separator).collect()
        } else {
            source.split(separator).collect()
        };

        let parts: Vec<Variant> = raw_parts
            .into_iter()
            .map(|p| {
                let s = if trim_each { p.trim() } else { p };
                Variant::String(s.to_owned())
            })
            .collect();

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Array(parts));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.string.split".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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

    #[tokio::test]
    async fn split_empty_separator_splits_between_each_char() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("Hi".to_owned()));
        cfg.insert("separator".to_owned(), Variant::String(String::new()));
        // Rust's `split("")` yields empty boundary parts at both ends.
        assert_eq!(split_parts(&cfg).await, vec!["", "H", "i", ""]);
    }
}
