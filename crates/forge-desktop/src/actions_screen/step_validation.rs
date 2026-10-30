use forge_registry::SubActionRunner;
use forge_types::{SubActionConfig, Variant, variable_references};

use super::config_problem_text::{config_problem_text, config_reason_text};

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
    let problem = runner.validate_config(config).err()?.into_config_problem();
    let deferred = match &problem.field {
        Some(key) => config.get(key).is_some_and(holds_variable_reference),
        None => config.values().any(holds_variable_reference),
    };
    if deferred {
        return None;
    }
    let field_key = problem.field.clone().filter(|key| field_keys.contains(key));
    let fields = runner.config_fields();
    let message = match field_key {
        Some(_) => config_reason_text(&problem.reason, runner.id(), &fields),
        None => config_problem_text(&problem, runner.id(), &fields),
    };
    Some(StepRejection { field_key, message })
}

fn holds_variable_reference(value: &Variant) -> bool {
    match value {
        Variant::String(text) => variable_references(text).next().is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use forge_registry::{
        ConfigProblem, ConfigReason, FormField, RegistryError, RunContext, SubActionCategory,
    };
    use forge_storage::Language;
    use forge_types::{ArgStack, MessageCode, MessageRef, SubActionTelemetry};

    use super::*;
    use crate::i18n::install_language;

    const KIND: &str = "test.kind";
    const KEYS: [&str; 3] = ["id", "clip_id", "text"];

    struct Verdict(Option<ConfigProblem>, Vec<FormField>);

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
            self.1.clone()
        }

        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            match &self.0 {
                Some(problem) => Err(RegistryError::InvalidConfig(problem.clone())),
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

    fn rejection(problem: ConfigProblem, entries: &[(&str, &str)]) -> Option<StepRejection> {
        rejection_on_form(problem, Vec::new(), entries)
    }

    fn rejection_on_form(
        problem: ConfigProblem,
        fields: Vec<FormField>,
        entries: &[(&str, &str)],
    ) -> Option<StepRejection> {
        install_language(Language::En);
        let runner = Verdict(Some(problem), fields);
        step_rejection(&runner, &config(entries), &keys()).map(|r| StepRejection {
            message: r
                .message
                .chars()
                .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
                .collect(),
            ..r
        })
    }

    fn clip_required() -> ConfigProblem {
        ConfigProblem::field("clip_id", ConfigReason::Required)
    }

    #[test]
    fn a_config_the_runner_accepts_is_not_rejected() {
        let verdict = step_rejection(
            &Verdict(None, Vec::new()),
            &config(&[("clip_id", "")]),
            &keys(),
        );

        assert_eq!(verdict, None);
    }

    #[test]
    fn a_problem_on_a_form_field_is_attributed_to_it_with_only_the_reason() {
        assert_eq!(
            rejection(clip_required(), &[]),
            Some(StepRejection {
                field_key: Some("clip_id".to_owned()),
                message: "Required".to_owned(),
            })
        );
    }

    #[test]
    fn a_problem_on_a_key_the_form_does_not_show_goes_to_the_footer_naming_the_key() {
        let problem = ConfigProblem::field("volume", ConfigReason::NotANumber);

        assert_eq!(
            rejection(problem, &[]),
            Some(StepRejection {
                field_key: None,
                message: "Must be a number (volume)".to_owned(),
            })
        );
    }

    #[test]
    fn a_footer_problem_names_a_key_the_runner_declares_by_its_label() {
        let volume = FormField::Slider {
            key: "volume",
            label: "Volume (0-100)",
            min: 0,
            max: 100,
            unit: "%",
        };
        let problem = ConfigProblem::field("volume", ConfigReason::NotANumber);

        assert_eq!(
            rejection_on_form(problem, vec![volume], &[]),
            Some(StepRejection {
                field_key: None,
                message: "Must be a number (Volume)".to_owned(),
            })
        );
    }

    #[test]
    fn a_whole_config_problem_goes_to_the_footer_as_the_reason_alone() {
        let problem = ConfigProblem::whole(ConfigReason::NotANumber);

        assert_eq!(
            rejection(problem, &[("text", "hello")]),
            Some(StepRejection {
                field_key: None,
                message: "Must be a number".to_owned(),
            })
        );
    }

    #[test]
    fn a_coded_reason_is_rendered_on_the_field_or_in_the_footer() {
        let coded = ConfigReason::Coded(MessageRef::new(MessageCode::from_static(
            "config",
            "not_a_step_list",
        )));
        for (problem, field_key) in [
            (
                ConfigProblem::field("clip_id", coded.clone()),
                Some("clip_id".to_owned()),
            ),
            (ConfigProblem::whole(coded), None),
        ] {
            assert_eq!(
                rejection(problem, &[]),
                Some(StepRejection {
                    field_key,
                    message: "Must be a list of steps".to_owned(),
                })
            );
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
            let verdict = rejection(clip_required(), &[("clip_id", value)]);

            assert_eq!(verdict.is_some(), rejected, "clip_id = {value:?}");
        }
    }

    #[test]
    fn a_variable_in_another_field_does_not_excuse_the_named_field() {
        let verdict = rejection(clip_required(), &[("clip_id", ""), ("text", "%user%")]);

        assert_eq!(
            verdict.and_then(|r| r.field_key).as_deref(),
            Some("clip_id")
        );
    }

    #[test]
    fn a_variable_anywhere_defers_a_whole_config_rejection() {
        let verdict = rejection(
            ConfigProblem::whole(ConfigReason::NotANumber),
            &[("clip_id", ""), ("text", "%user%")],
        );

        assert_eq!(verdict, None);
    }
}
