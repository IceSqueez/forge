use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, VerbatimText};
use time::OffsetDateTime;
use tokio::sync::Notify;

pub struct GateRunner {
    pub running: Arc<Notify>,
}

#[async_trait]
impl SubActionRunner for GateRunner {
    fn id(&self) -> &str {
        "test.gate"
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        self.running.notify_one();
        while !ctx.cancel.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "test.gate".to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: SubActionOutcome::Success,
            },
            None,
        )
    }
}

pub struct CancelsItsRunRunner;

#[async_trait]
impl SubActionRunner for CancelsItsRunRunner {
    fn id(&self) -> &str {
        "test.cancels_its_run"
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        ctx.cancel.cancel();
        (
            StepTimer::start(ctx, "test.cancels_its_run").success(),
            None,
        )
    }
}

pub struct FixedOutcomeRunner {
    pub id: &'static str,
    pub outcome: SubActionOutcome,
}

#[async_trait]
impl SubActionRunner for FixedOutcomeRunner {
    fn id(&self) -> &str {
        self.id
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        self.id
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: self.id.to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: self.outcome.clone(),
            },
            None,
        )
    }
}

pub fn always_failing_runner(id: &'static str) -> FixedOutcomeRunner {
    FixedOutcomeRunner {
        id,
        outcome: SubActionOutcome::Failed(VerbatimText::new("boom").into()),
    }
}
