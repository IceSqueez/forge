use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
        "String — Regex Match"
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
                label: "Match Result Variable (bool)",
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
        let pattern = config.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        if pattern.is_empty() {
            return Err(RegistryError::UnknownKindId(
                "core.string.regex_match: pattern is required".to_owned(),
            ));
        }
        regex::Regex::new(pattern).map_err(|e| {
            RegistryError::UnknownKindId(format!("core.string.regex_match: invalid regex: {e}"))
        })?;
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let source = config.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let pattern = config.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("regex.matched")
            .to_owned();
        let captures_into_var = config
            .get("captures_into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("regex.captures")
            .to_owned();

        if pattern.is_empty() {
            let new_stack = ctx
                .arg_stack
                .clone()
                .set(into_var, Variant::Bool(false))
                .set(captures_into_var, Variant::Array(vec![]));
            let duration_ms = (OffsetDateTime::now_utc() - started_at)
                .whole_milliseconds()
                .max(0) as u64;
            return (
                SubActionTelemetry {
                    index: ctx.index,
                    kind: "core.string.regex_match".to_owned(),
                    started_at,
                    duration_ms,
                    outcome: SubActionOutcome::Success,
                },
                Some(new_stack),
            );
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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.regex_match".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            new_stack_opt,
        )
    }
}
