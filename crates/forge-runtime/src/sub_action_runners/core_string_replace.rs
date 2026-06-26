use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
        "String — Replace"
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
        let search = config.get("search").and_then(|v| v.as_str()).unwrap_or("");
        let is_regex = config
            .get("is_regex")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_regex && !search.is_empty() {
            regex::Regex::new(search).map_err(|e| {
                RegistryError::UnknownKindId(format!("core.string.replace: invalid regex: {e}"))
            })?;
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let source = config.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let search = config.get("search").and_then(|v| v.as_str()).unwrap_or("");
        let replace_with = config
            .get("replace_with")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let case_sensitive = config
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let is_regex = config
            .get("is_regex")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("string.result")
            .to_owned();

        if search.is_empty() {
            let new_stack = ctx
                .arg_stack
                .clone()
                .set(into_var, Variant::String(source.to_owned()));
            let duration_ms = (OffsetDateTime::now_utc() - started_at)
                .whole_milliseconds()
                .max(0) as u64;
            return (
                SubActionTelemetry {
                    index: ctx.index,
                    kind: "core.string.replace".to_owned(),
                    started_at,
                    duration_ms,
                    outcome: SubActionOutcome::Success,
                },
                Some(new_stack),
            );
        }

        let outcome = match apply_replace(source, search, replace_with, case_sensitive, is_regex) {
            Ok(result) => {
                let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));
                let duration_ms = (OffsetDateTime::now_utc() - started_at)
                    .whole_milliseconds()
                    .max(0) as u64;
                return (
                    SubActionTelemetry {
                        index: ctx.index,
                        kind: "core.string.replace".to_owned(),
                        started_at,
                        duration_ms,
                        outcome: SubActionOutcome::Success,
                    },
                    Some(new_stack),
                );
            }
            Err(msg) => SubActionOutcome::Failed(msg),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.replace".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
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
