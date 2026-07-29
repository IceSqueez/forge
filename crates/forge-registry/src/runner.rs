use async_trait::async_trait;
use forge_types::{ArgStack, SubActionTelemetry};

pub use forge_types::SubActionConfig;

use crate::category::SubActionCategory;
use crate::error::RegistryError;
use crate::form::FormField;
use crate::io::SubActionIo;
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
    fn default_config(&self) -> SubActionConfig;
    fn config_fields(&self) -> Vec<FormField>;
    fn config_refinement(&self) -> Option<FormRefinement> {
        None
    }
    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError>;
    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>);
    fn scope_io(&self) -> SubActionIo {
        SubActionIo::default()
    }
}
