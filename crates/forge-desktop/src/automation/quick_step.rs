use forge_runtime::{ActionEngineHandle, QuickActionOrigin};
use forge_types::{SubActionOutcome, SubActionStep, integration_disabled_message};

use crate::shared::repo_failure::RepoFailure;

pub(crate) async fn run_quick_step(
    engine: ActionEngineHandle,
    step: SubActionStep,
    origin: QuickActionOrigin,
) -> Result<(), RepoFailure> {
    let pending = engine
        .execute_quick_action(step, origin, None)
        .await
        .map_err(RepoFailure::from)?;
    match pending.outcome().await.map_err(RepoFailure::from)? {
        SubActionOutcome::Success => Ok(()),
        SubActionOutcome::Failed(reason) | SubActionOutcome::Skipped(reason) => {
            Err(RepoFailure::shown(reason.clone(), reason.code_or_text()))
        }
        SubActionOutcome::IntegrationDisabled(integration) => Err(RepoFailure::shown(
            integration_disabled_message(&integration),
            integration.as_str(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_registry::{
        FormField, RegistryError, RunContext, StepCatalogEntry, StepTimer, SubActionCategory,
        SubActionRegistry, SubActionRunner,
    };
    use forge_runtime::{ActionCancelRegistry, EventBus, RuntimeError, spawn_action_engine};
    use forge_storage::MockHistoryRepo;
    use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

    use super::*;
    use crate::test_support::{StubActions, StubEventLog, runtime};

    const SUCCEEDS: &str = "test.succeeds";
    const FAILS: &str = "test.fails";
    const SKIPS: &str = "test.skips";
    const FAILURE_REASON: &str = "scene not found";
    const SKIP_REASON: &str = "not connected";
    const ENGINE_EXIT_HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);

    struct FixedOutcome {
        id: &'static str,
        outcome: SubActionOutcome,
    }

    impl StepCatalogEntry for FixedOutcome {
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
    }

    #[async_trait::async_trait]
    impl SubActionRunner for FixedOutcome {
        fn id(&self) -> &str {
            self.id
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
                StepTimer::start(ctx, self.id).finish(self.outcome.clone()),
                None,
            )
        }
    }

    fn engine() -> ActionEngineHandle {
        let mut registry = SubActionRegistry::new();
        for (id, outcome) in [
            (SUCCEEDS, SubActionOutcome::Success),
            (
                FAILS,
                SubActionOutcome::Failed(forge_types::VerbatimText::new(FAILURE_REASON).into()),
            ),
            (
                SKIPS,
                SubActionOutcome::Skipped(forge_types::VerbatimText::new(SKIP_REASON).into()),
            ),
        ] {
            registry
                .register(Box::new(FixedOutcome { id, outcome }))
                .expect("each fixed-outcome runner has its own id");
        }
        let mut history = MockHistoryRepo::new();
        history.expect_save().returning(|_| Ok(()));
        spawn_action_engine(forge_runtime::ActionEngineParts {
            bus: EventBus::new(Arc::new(StubEventLog)),
            catalog: crate::test_support::stub_catalog(),
            actions: Arc::new(StubActions),
            history: Arc::new(history),
            sub_action_registry: Arc::new(registry),
            cancel_registry: Arc::new(ActionCancelRegistry::new()),
            date_names: forge_runtime::DateNamesHandle::default(),
            config: forge_runtime::Config::default(),
        })
    }

    fn quick_step(kind_id: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config: SubActionConfig::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    async fn run(engine: ActionEngineHandle, kind_id: &str) -> Result<(), String> {
        run_quick_step(
            engine,
            quick_step(kind_id),
            QuickActionOrigin {
                builtin_id: "obs".to_owned(),
                label: kind_id.to_owned(),
            },
        )
        .await
        .map_err(|failure| failure.to_string())
    }

    #[test]
    fn a_quick_step_settles_ok_only_when_the_step_succeeded() {
        let rt = runtime();
        for (kind_id, expected) in [
            (SUCCEEDS, Ok(())),
            (FAILS, Err(FAILURE_REASON.to_owned())),
            (SKIPS, Err(SKIP_REASON.to_owned())),
        ] {
            let settled = rt.block_on(async { run(engine(), kind_id).await });
            assert_eq!(settled, expected, "{kind_id}");
        }
    }

    #[test]
    fn a_quick_step_the_stopped_engine_never_ran_settles_as_an_error() {
        let rt = runtime();
        let not_accepted = rt.block_on(async {
            let engine = engine();
            engine.clone().shutdown();
            let exited = tokio::time::timeout(ENGINE_EXIT_HANG_GUARD, async {
                loop {
                    let outcome = run(engine.clone(), SUCCEEDS).await;
                    if outcome != Err(RuntimeError::EngineNoOutcome.to_string()) {
                        return outcome;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await;
            exited.expect("the stopped engine never closed its intake")
        });
        let dropped_from_intake = rt.block_on(async {
            let engine = engine();
            engine.clone().shutdown();
            run(engine, SUCCEEDS).await
        });

        assert_eq!(
            (not_accepted, dropped_from_intake),
            (
                Err(RuntimeError::EngineChannelClosed.to_string()),
                Err(RuntimeError::EngineNoOutcome.to_string()),
            )
        );
    }
}
