use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TemplateField, TextField, ToggleField,
    TypedField,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, UserText, Variant, VariantKind,
};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY, source_field};
use super::regex_pattern::compiled_pattern;

const SOURCE: TextField = source_field("Hello World");

const SEARCH: TemplateField = TemplateField {
    key: "search",
    label: "Search",
    placeholder: "World",
    default: "",
    required: false,
};

const REPLACE_WITH: TemplateField = TemplateField {
    key: "replace_with",
    label: "Replace With",
    placeholder: "Forge",
    default: "",
    required: false,
};

const CASE_SENSITIVE: ToggleField = ToggleField {
    key: "case_sensitive",
    label: "Case Sensitive",
    default: true,
};

const IS_REGEX: ToggleField = ToggleField {
    key: "is_regex",
    label: "Use Regex",
    default: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &SOURCE,
    &SEARCH,
    &REPLACE_WITH,
    &CASE_SENSITIVE,
    &IS_REGEX,
    &INTO_RESULT,
]);

struct ReplaceConfig {
    source: String,
    search: String,
    replace_with: String,
    case_sensitive: bool,
    is_regex: bool,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<ReplaceConfig, ConfigProblem> {
    let is_regex = IS_REGEX.read(config, args)?;
    let search = SEARCH.read(config, args)?;
    let replace_with = REPLACE_WITH.read(config, args)?;
    let (search, replace_with) = if is_regex {
        if !search.is_empty() {
            compiled_pattern(SEARCH.key, &search)?;
        }
        (search, replace_with)
    } else {
        (args.interpolate(&search), args.interpolate(&replace_with))
    };
    Ok(ReplaceConfig {
        source: SOURCE.read(config, args)?,
        search,
        replace_with,
        case_sensitive: CASE_SENSITIVE.read(config, args)?,
        is_regex,
        into_var: INTO_RESULT.read(config, args)?,
    })
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match (
            IS_REGEX.peek(config, interpolation)?,
            SEARCH.peek(config, interpolation)?,
        ) {
            (Some(true), Some(search)) if !search.is_empty() => {
                compiled_pattern(SEARCH.key, &search).map(|_| ())
            }
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Replaced string".to_owned(),
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
        let ReplaceConfig {
            source,
            search,
            replace_with,
            case_sensitive,
            is_regex,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        if search.is_empty() {
            let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(source));
            return (timer.success(), Some(new_stack));
        }

        let outcome = match apply_replace(&source, &search, &replace_with, case_sensitive, is_regex)
        {
            Ok(result) => {
                let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));
                return (timer.success(), Some(new_stack));
            }
            Err(msg) => SubActionOutcome::Failed(UserText::unlocalized(msg)),
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
    let replacement = if is_regex {
        replace_with.to_owned()
    } else {
        replace_with.replace('$', "$$")
    };
    Ok(re.replace_all(source, replacement.as_str()).into_owned())
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
