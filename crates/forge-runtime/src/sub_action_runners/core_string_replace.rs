use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

pub struct CoreStringReplaceRunner;

#[async_trait]
impl SubActionRunner for CoreStringReplaceRunner {
    fn id(&self) -> &str {
        "core.string.replace"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Replace"
    }

    fn summary(&self) -> &str {
        "Replace all occurrences of a literal or regex pattern in a string"
    }

    fn search_text(&self) -> &str {
        "string replace substitute regex find pattern"
    }

    fn icon_name(&self) -> &str {
        "replace"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert("search".to_owned(), Variant::String(String::new()));
        cfg.insert("replace_with".to_owned(), Variant::String(String::new()));
        cfg.insert("case_sensitive".to_owned(), Variant::Bool(true));
        cfg.insert("is_regex".to_owned(), Variant::Bool(false));
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
                placeholder: "Hello World",
            },
            FormField::Text {
                key: "search",
                label: "Search",
                placeholder: "World",
            },
            FormField::Text {
                key: "replace_with",
                label: "Replace With",
                placeholder: "Forge",
            },
            FormField::Toggle {
                key: "case_sensitive",
                label: "Case Sensitive",
            },
            FormField::Toggle {
                key: "is_regex",
                label: "Use Regex",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "string.result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let search = config.str("search").unwrap_or("");
        let is_regex = config.bool("is_regex").unwrap_or(false);
        if is_regex && !search.is_empty() {
            regex::Regex::new(search).map_err(|e| {
                RegistryError::InvalidConfig(format!("core.string.replace: invalid regex: {e}"))
            })?;
        }
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::String,
                label: "Replaced string".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.string.replace");

        let source = config.str("source").unwrap_or("");
        let search = config.str("search").unwrap_or("");
        let replace_with = config.str("replace_with").unwrap_or("");
        let case_sensitive = config.bool("case_sensitive").unwrap_or(true);
        let is_regex = config.bool("is_regex").unwrap_or(false);
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("string.result"),
        );

        if search.is_empty() {
            let new_stack = ctx
                .arg_stack
                .clone()
                .set(into_var, Variant::String(source.to_owned()));
            return (timer.success(), Some(new_stack));
        }

        let outcome = match apply_replace(source, search, replace_with, case_sensitive, is_regex) {
            Ok(result) => {
                let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));
                return (timer.success(), Some(new_stack));
            }
            Err(msg) => SubActionOutcome::Failed(msg),
        };

        (timer.finish(outcome), None)
    }
}

fn apply_replace(
    source: &str,
    search: &str,
    replace_with: &str,
    case_sensitive: bool,
    is_regex: bool,
) -> Result<String, String> {
    let pattern = if is_regex {
        if case_sensitive {
            search.to_owned()
        } else {
            format!("(?i){search}")
        }
    } else {
        let escaped = regex::escape(search);
        if case_sensitive {
            escaped
        } else {
            format!("(?i){escaped}")
        }
    };

    let re = regex::Regex::new(&pattern).map_err(|e| format!("invalid pattern: {e}"))?;

    // For literal replacements, escape $ so it is not interpreted as a capture group reference.
    let replacement = if is_regex {
        replace_with.to_owned()
    } else {
        replace_with.replace('$', "$$")
    };

    Ok(re.replace_all(source, replacement.as_str()).into_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    async fn replaced(cfg: &SubActionConfig) -> String {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringReplaceRunner.execute(cfg, &ctx).await.1.unwrap();
        out.get("string.result")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn replace_literal_is_case_insensitive_when_flag_unset() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "source".to_owned(),
            Variant::String("Hello HELLO".to_owned()),
        );
        cfg.insert("search".to_owned(), Variant::String("hello".to_owned()));
        cfg.insert("replace_with".to_owned(), Variant::String("x".to_owned()));
        cfg.insert("case_sensitive".to_owned(), Variant::Bool(false));
        assert_eq!(replaced(&cfg).await, "x x");
    }

    #[tokio::test]
    async fn replace_literal_dollar_in_replacement_is_not_a_capture_reference() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("price".to_owned()));
        cfg.insert("search".to_owned(), Variant::String("price".to_owned()));
        cfg.insert("replace_with".to_owned(), Variant::String("$1".to_owned()));
        assert_eq!(replaced(&cfg).await, "$1");
    }

    #[tokio::test]
    async fn replace_regex_mode_expands_dollar_group_references() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "source".to_owned(),
            Variant::String("John Smith".to_owned()),
        );
        cfg.insert(
            "search".to_owned(),
            Variant::String(r"(\w+) (\w+)".to_owned()),
        );
        cfg.insert(
            "replace_with".to_owned(),
            Variant::String("$2 $1".to_owned()),
        );
        cfg.insert("is_regex".to_owned(), Variant::Bool(true));
        assert_eq!(replaced(&cfg).await, "Smith John");
    }

    #[tokio::test]
    async fn replace_regex_mode_applies_pattern_to_all_matches() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("a1b2".to_owned()));
        cfg.insert("search".to_owned(), Variant::String(r"\d".to_owned()));
        cfg.insert("replace_with".to_owned(), Variant::String("#".to_owned()));
        cfg.insert("is_regex".to_owned(), Variant::Bool(true));
        assert_eq!(replaced(&cfg).await, "a#b#");
    }
}
