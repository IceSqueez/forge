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
