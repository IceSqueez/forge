use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

pub struct CoreStringRegexMatchRunner;

#[async_trait]
impl SubActionRunner for CoreStringRegexMatchRunner {
    fn id(&self) -> &str {
        "core.string.regex_match"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Regex Match"
    }

    fn summary(&self) -> &str {
        "Test a string against a regex pattern and optionally capture groups"
    }

    fn search_text(&self) -> &str {
        "string regex match pattern test capture groups"
    }

    fn icon_name(&self) -> &str {
        "regex"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert("pattern".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("regex.matched".to_owned()),
        );
        cfg.insert(
            "captures_into_var".to_owned(),
            Variant::String("regex.captures".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source",
                placeholder: "Hello World",
            },
            FormField::Text {
                key: "pattern",
                label: "Pattern",
                placeholder: r"(\w+)",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable (bool)",
                placeholder: "regex.matched",
            },
            FormField::Text {
                key: "captures_into_var",
                label: "Captures Variable (array)",
                placeholder: "regex.captures",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let pattern = config.require_str("pattern")?;
        regex::Regex::new(pattern).map_err(|e| {
            RegistryError::InvalidConfig(format!("core.string.regex_match: invalid regex: {e}"))
        })?;
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![
                ProducedVariable {
                    output_name_key: "into_var".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Match found".to_owned(),
                },
                ProducedVariable {
                    output_name_key: "captures_into_var".to_owned(),
                    kind: VariantKind::Array,
                    label: "Capture groups".to_owned(),
                },
            ],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.string.regex_match");

        let source = config.str("source").unwrap_or("");
        let pattern = config.str("pattern").unwrap_or("");
        let into_var = super::interpolate::sanitize_var_name(
            config.str_nonempty("into_var").unwrap_or("regex.matched"),
        );
        let captures_into_var = super::interpolate::sanitize_var_name(
            config
                .str_nonempty("captures_into_var")
                .unwrap_or("regex.captures"),
        );

        if pattern.is_empty() {
            let new_stack = ctx
                .arg_stack
                .clone()
                .set(into_var, Variant::Bool(false))
                .set(captures_into_var, Variant::Array(vec![]));
            return (timer.success(), Some(new_stack));
        }

        let (outcome, new_stack_opt) = match regex::Regex::new(pattern) {
            Err(e) => (
                SubActionOutcome::Failed(format!("invalid regex: {e}")),
                None,
            ),
            Ok(re) => {
                // captures[0] = full match, captures[1..] = numbered groups.
                let (matched, captures) = match re.captures(source) {
                    None => (false, vec![]),
                    Some(caps) => {
                        let groups: Vec<Variant> = caps
                            .iter()
                            .map(|m| {
                                Variant::String(m.map(|m| m.as_str()).unwrap_or("").to_owned())
                            })
                            .collect();
                        (true, groups)
                    }
                };
                let new_stack = ctx
                    .arg_stack
                    .clone()
                    .set(into_var, Variant::Bool(matched))
                    .set(captures_into_var, Variant::Array(captures));
                (SubActionOutcome::Success, Some(new_stack))
            }
        };

        (timer.finish(outcome), new_stack_opt)
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

    async fn run(cfg: &SubActionConfig) -> (SubActionTelemetry, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        CoreStringRegexMatchRunner.execute(cfg, &ctx).await
    }

    fn captures(stack: &ArgStack) -> Vec<String> {
        match stack.get("regex.captures") {
            Some(Variant::Array(items)) => items
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
            other => panic!("expected Variant::Array under regex.captures, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn regex_match_sets_flag_and_full_match_plus_group_captures() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("2026-06".to_owned()));
        cfg.insert(
            "pattern".to_owned(),
            Variant::String(r"(\d+)-(\d+)".to_owned()),
        );
        let stack = run(&cfg).await.1.unwrap();
        assert_eq!(
            stack.get("regex.matched").and_then(|v| v.as_bool()),
            Some(true)
        );
        // captures[0] is the whole match, [1..] the numbered groups.
        assert_eq!(captures(&stack), vec!["2026-06", "2026", "06"]);
    }

    #[tokio::test]
    async fn regex_match_no_match_sets_false_and_empty_captures() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("abc".to_owned()));
        cfg.insert("pattern".to_owned(), Variant::String(r"\d+".to_owned()));
        let stack = run(&cfg).await.1.unwrap();
        assert_eq!(
            stack.get("regex.matched").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert!(captures(&stack).is_empty());
    }

    #[tokio::test]
    async fn regex_match_invalid_pattern_fails_without_panic() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("abc".to_owned()));
        cfg.insert("pattern".to_owned(), Variant::String("(".to_owned()));
        let (tel, stack) = run(&cfg).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(stack.is_none());
    }
}
