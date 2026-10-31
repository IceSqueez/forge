use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TemplateField, TextField, TypedField,
    VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use regex::Regex;

use super::core_string_shared::{INTO_VAR_KEY, source_field};
use super::regex_pattern::compiled_pattern;

const CAPTURES_INTO_VAR_KEY: &str = "captures_into_var";
const MATCHED_VARIABLE: &str = "regex.matched";
const CAPTURES_VARIABLE: &str = "regex.captures";

const SOURCE: TextField = source_field("Hello World");

const PATTERN: TemplateField = TemplateField {
    key: "pattern",
    label: "Pattern",
    placeholder: r"(\w+)",
    default: "",
    required: true,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable (bool)",
    placeholder: MATCHED_VARIABLE,
    default: MATCHED_VARIABLE,
};

const CAPTURES_INTO_VAR: VariableField = VariableField {
    key: CAPTURES_INTO_VAR_KEY,
    label: "Captures Variable (array)",
    placeholder: CAPTURES_VARIABLE,
    default: CAPTURES_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &PATTERN, &INTO_VAR, &CAPTURES_INTO_VAR]);

struct MatchConfig {
    source: String,
    pattern: Regex,
    into_var: String,
    captures_into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<MatchConfig, ConfigProblem> {
    Ok(MatchConfig {
        source: SOURCE.read(config, args)?,
        pattern: compiled_pattern(PATTERN.key, &PATTERN.read(config, args)?)?,
        into_var: INTO_VAR.read(config, args)?,
        captures_into_var: CAPTURES_INTO_VAR.read(config, args)?,
    })
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match PATTERN.peek(config, interpolation)? {
            Some(pattern) => compiled_pattern(PATTERN.key, &pattern).map(|_| ()),
            None => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![
                ProducedVariable {
                    output_name_key: INTO_VAR_KEY.to_owned(),
                    kind: VariantKind::Bool,
                    label: "Match found".to_owned(),
                },
                ProducedVariable {
                    output_name_key: CAPTURES_INTO_VAR_KEY.to_owned(),
                    kind: VariantKind::Array,
                    label: "Capture groups".to_owned(),
                },
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let MatchConfig {
            source,
            pattern,
            into_var,
            captures_into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let (matched, captures) = match pattern.captures(&source) {
            None => (false, Vec::new()),
            Some(caps) => {
                let groups = caps
                    .iter()
                    .map(|m| Variant::String(m.map(|m| m.as_str()).unwrap_or("").to_owned()))
                    .collect();
                (true, groups)
            }
        };

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::Bool(matched))
            .set(captures_into_var, Variant::Array(captures));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_registry::ConfigReason;
    use forge_registry::RegistryError;
    use forge_types::EventId;
    use forge_types::SubActionOutcome;

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

    fn pattern_problem(cfg: &SubActionConfig) -> Option<forge_registry::ConfigProblem> {
        CoreStringRegexMatchRunner
            .validate_config(cfg)
            .err()
            .map(RegistryError::into_config_problem)
    }

    fn pattern(value: &str) -> SubActionConfig {
        SubActionConfig::from([("pattern".to_owned(), Variant::String(value.to_owned()))])
    }

    #[test]
    fn validate_config_reports_an_unparsable_pattern_as_a_format_problem_on_the_pattern() {
        assert!(matches!(
            pattern_problem(&pattern("(ab")),
            Some(forge_registry::ConfigProblem {
                field: Some(field),
                reason: ConfigReason::Coded(message),
            }) if field == "pattern" && message.code() == &crate::core_messages::REGEX_INVALID
        ));
    }

    #[test]
    fn validate_config_requires_a_pattern_and_accepts_a_parsable_one() {
        assert_eq!(
            pattern_problem(&SubActionConfig::new()),
            Some(forge_registry::ConfigProblem::field(
                "pattern",
                ConfigReason::Required
            ))
        );
        assert_eq!(pattern_problem(&pattern("(ab)+")), None);
    }
}
