use async_trait::async_trait;
use forge_types::{ArgStack, IntegrationId, SubActionTelemetry};

pub use forge_types::SubActionConfig;

use crate::category::SubActionCategory;
use crate::config_problem::ConfigProblem;
use crate::declared_config::{ConfigSpec, Interpolation};
use crate::error::RegistryError;
use crate::form::FormField;
use crate::io::SubActionIo;
use crate::references::ReferenceField;
use crate::refinement::FormRefinement;
use crate::run_context::RunContext;

#[async_trait]
pub trait SubActionRunner: Send + Sync {
    fn id(&self) -> &str;
    fn category(&self) -> SubActionCategory;
    fn label(&self) -> &str;
    fn summary(&self) -> &str;
    fn search_text(&self) -> &str;
    fn icon_name(&self) -> &str;
    fn config_spec(&self) -> Option<ConfigSpec> {
        None
    }
    fn check_config(
        &self,
        _config: &SubActionConfig,
        _interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        Ok(())
    }
    fn default_config(&self) -> SubActionConfig {
        self.config_spec()
            .map(|spec| spec.default_config())
            .unwrap_or_default()
    }
    fn config_fields(&self) -> Vec<FormField> {
        self.config_spec()
            .map(|spec| spec.form_fields())
            .unwrap_or_default()
    }
    fn config_refinement(&self) -> Option<FormRefinement> {
        None
    }
    fn declared_references(&self) -> Vec<ReferenceField> {
        Vec::new()
    }
    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_declared_config(self, config, Interpolation::Deferred)
            .map_err(RegistryError::InvalidConfig)
    }
    fn checks_config_while_editing(&self) -> bool {
        self.config_spec().is_some()
    }
    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>);
    fn scope_io(&self) -> SubActionIo {
        SubActionIo::default()
    }
    fn targeted_integration(
        &self,
        _config: &SubActionConfig,
        _arg_stack: &ArgStack,
    ) -> Option<IntegrationId> {
        None
    }
}

pub fn validate_declared_config<R: SubActionRunner + ?Sized>(
    runner: &R,
    config: &SubActionConfig,
    interpolation: Interpolation<'_>,
) -> Result<(), ConfigProblem> {
    let Some(spec) = runner.config_spec() else {
        return Ok(());
    };
    spec.validate(config, interpolation)?;
    runner.check_config(config, interpolation)
}

#[cfg(test)]
mod tests {
    use forge_types::Variant;

    use super::*;
    use crate::config_problem::{ConfigReason, ValueBounds};
    use crate::declared_fields::{IntegerEditor, IntegerField};

    const LIMIT: IntegerField = IntegerField {
        key: "limit",
        label: "Limit",
        editor: IntegerEditor::Number,
        default: 3,
        bounds: ValueBounds {
            min: Some(1),
            max: Some(5),
        },
    };

    struct DeclaringRunner {
        spec: Option<ConfigSpec>,
    }

    const DECLARING: DeclaringRunner = DeclaringRunner {
        spec: Some(ConfigSpec::new(&[&LIMIT])),
    };
    const UNDECLARED: DeclaringRunner = DeclaringRunner { spec: None };

    fn cross_check_problem() -> ConfigProblem {
        ConfigProblem::whole(ConfigReason::Custom("cross check".to_owned()))
    }

    #[async_trait]
    impl SubActionRunner for DeclaringRunner {
        fn id(&self) -> &str {
            "test.declaring"
        }
        fn category(&self) -> SubActionCategory {
            SubActionCategory::Util
        }
        fn label(&self) -> &str {
            "Declaring"
        }
        fn summary(&self) -> &str {
            ""
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            "bolt"
        }
        fn config_spec(&self) -> Option<ConfigSpec> {
            self.spec
        }
        fn check_config(
            &self,
            config: &SubActionConfig,
            _interpolation: Interpolation<'_>,
        ) -> Result<(), ConfigProblem> {
            match config.get("limit") {
                Some(Variant::Int(4)) => Err(cross_check_problem()),
                _ => Ok(()),
            }
        }
        async fn execute(
            &self,
            _config: &SubActionConfig,
            _ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            unreachable!("config checks never execute the step")
        }
    }

    fn limit(value: Variant) -> SubActionConfig {
        SubActionConfig::from([("limit".to_owned(), value)])
    }

    #[test]
    fn declared_config_runs_the_runner_cross_check_only_after_every_field_passes() {
        for (value, expected) in [
            (Variant::Int(3), Ok(())),
            (Variant::Int(4), Err(cross_check_problem())),
            (
                Variant::Int(9),
                Err(ConfigProblem::field(
                    "limit",
                    ConfigReason::OutOfRange(LIMIT.bounds),
                )),
            ),
        ] {
            assert_eq!(
                validate_declared_config(
                    &DECLARING,
                    &limit(value.clone()),
                    Interpolation::Deferred
                ),
                expected,
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_runner_without_a_declared_spec_accepts_any_config_and_skips_its_cross_check() {
        assert_eq!(
            validate_declared_config(
                &UNDECLARED,
                &limit(Variant::Int(4)),
                Interpolation::Deferred
            ),
            Ok(())
        );
    }

    #[test]
    fn validate_config_wraps_the_declared_problem_and_defers_templates() {
        let refused = DECLARING.validate_config(&limit(Variant::Int(0)));

        assert!(
            matches!(
                &refused,
                Err(RegistryError::InvalidConfig(problem))
                    if *problem == ConfigProblem::field("limit", ConfigReason::OutOfRange(LIMIT.bounds))
            ),
            "{refused:?}"
        );
        assert!(
            DECLARING
                .validate_config(&limit(Variant::String("%anything%".to_owned())))
                .is_ok()
        );
    }

    #[test]
    fn declaring_a_spec_derives_defaults_form_and_live_checking() {
        assert_eq!(
            (
                DECLARING.default_config(),
                DECLARING.config_fields().len(),
                DECLARING.checks_config_while_editing(),
            ),
            (limit(Variant::Int(3)), 1, true)
        );
        assert_eq!(
            (
                UNDECLARED.default_config(),
                UNDECLARED.config_fields().len(),
                UNDECLARED.checks_config_while_editing(),
            ),
            (SubActionConfig::new(), 0, false)
        );
    }
}
