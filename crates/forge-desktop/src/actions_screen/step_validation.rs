use forge_registry::SubActionRunner;
use forge_types::{SubActionConfig, Variant, variable_references};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StepRejection {
    pub field_key: Option<String>,
    pub message: String,
}

pub(super) fn step_rejection(
    runner: &dyn SubActionRunner,
    config: &SubActionConfig,
    field_keys: &[String],
) -> Option<StepRejection> {
    let error = runner.validate_config(config).err()?;
    let raw = error.to_string();
    let message = raw
        .strip_prefix(runner.id())
        .and_then(|rest| rest.strip_prefix(':'))
        .map(str::trim_start)
        .unwrap_or(&raw)
        .to_owned();
    let field_key = field_keys
        .iter()
        .find(|key| message_names_key(&message, key))
        .cloned();
    let deferred = match &field_key {
        Some(key) => config.get(key).is_some_and(holds_variable_reference),
        None => config.values().any(holds_variable_reference),
    };
    (!deferred).then_some(StepRejection { field_key, message })
}

fn message_names_key(message: &str, key: &str) -> bool {
    message.contains(&format!("'{key}'"))
        || message.contains(&format!("`{key}`"))
        || message.starts_with(&format!("{key} "))
}

fn holds_variable_reference(value: &Variant) -> bool {
    match value {
        Variant::String(text) => variable_references(text).next().is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory};
    use forge_types::{ArgStack, SubActionTelemetry};

    use super::*;

    const KIND: &str = "test.kind";
    const KEYS: [&str; 3] = ["id", "clip_id", "text"];

    struct Verdict(Option<&'static str>);

    #[async_trait::async_trait]
    impl SubActionRunner for Verdict {
        fn id(&self) -> &str {
            KIND
        }

        fn category(&self) -> SubActionCategory {
            SubActionCategory::Audio
        }

        fn label(&self) -> &str {
            KIND
        }

        fn summary(&self) -> &str {
            KIND
        }

        fn search_text(&self) -> &str {
            KIND
        }

        fn icon_name(&self) -> &str {
            "volume"
        }

        fn default_config(&self) -> SubActionConfig {
            SubActionConfig::new()
        }

        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }

        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            match self.0 {
                Some(message) => Err(RegistryError::InvalidConfig(message.to_owned())),
                None => Ok(()),
            }
        }

        async fn execute(
            &self,
            _: &SubActionConfig,
            _: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            unreachable!("validation never runs the step")
        }
    }

    fn keys() -> Vec<String> {
        KEYS.map(str::to_owned).to_vec()
    }

    fn config(entries: &[(&str, &str)]) -> SubActionConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect()
    }

    fn rejection(message: &'static str, entries: &[(&str, &str)]) -> Option<StepRejection> {
        step_rejection(&Verdict(Some(message)), &config(entries), &keys())
    }

    #[test]
    fn a_config_the_runner_accepts_is_not_rejected() {
        let verdict = step_rejection(&Verdict(None), &config(&[("clip_id", "")]), &keys());

        assert_eq!(verdict, None);
    }

    #[test]
    fn the_rejection_names_the_field_its_message_quotes_or_starts_with() {
        for (message, expected) in [
            ("clip_id is required", Some("clip_id")),
            ("missing 'clip_id' value", Some("clip_id")),
            ("missing `text` value", Some("text")),
            ("test.kind: clip_id is required", Some("clip_id")),
            ("id is required", Some("id")),
            ("volume must be between 0 and 1", None),
        ] {
            let field = rejection(message, &[]).and_then(|r| r.field_key);

            assert_eq!(field.as_deref(), expected, "{message:?}");
        }
    }

    #[test]
    fn the_rejection_message_drops_only_the_runners_own_kind_prefix() {
        for (raw, shown) in [
            ("test.kind: clip_id is required", "clip_id is required"),
            ("test.kind:clip_id is required", "clip_id is required"),
            ("clip_id is required", "clip_id is required"),
            (
                "test.kinder: clip_id is required",
                "test.kinder: clip_id is required",
            ),
            (
                "other.kind: clip_id is required",
                "other.kind: clip_id is required",
            ),
        ] {
            let message = rejection(raw, &[]).map(|r| r.message);

            assert_eq!(message.as_deref(), Some(shown), "{raw:?}");
        }
    }

    #[test]
    fn a_variable_in_the_named_field_defers_the_check_to_run_time() {
        for (value, rejected) in [
            ("%clip.id%", false),
            ("intro %clip.id%", false),
            ("50% off", true),
            ("", true),
        ] {
            let verdict = rejection("test.kind: clip_id is required", &[("clip_id", value)]);

            assert_eq!(verdict.is_some(), rejected, "clip_id = {value:?}");
        }
    }

    #[test]
    fn a_variable_in_another_field_does_not_excuse_the_named_field() {
        let verdict = rejection(
            "test.kind: clip_id is required",
            &[("clip_id", ""), ("text", "%user%")],
        );

        assert_eq!(
            verdict.and_then(|r| r.field_key).as_deref(),
            Some("clip_id")
        );
    }

    #[test]
    fn an_unattributed_rejection_goes_to_the_footer() {
        let verdict = rejection("test.kind: volume out of range", &[("text", "hello")]);

        assert_eq!(
            verdict,
            Some(StepRejection {
                field_key: None,
                message: "volume out of range".to_owned(),
            })
        );
    }

    #[test]
    fn a_variable_anywhere_defers_an_unattributed_rejection() {
        let verdict = rejection(
            "test.kind: volume out of range",
            &[("clip_id", ""), ("text", "%user%")],
        );

        assert_eq!(verdict, None);
    }
}
