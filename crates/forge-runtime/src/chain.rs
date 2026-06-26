use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, RegistryError, RunContext,
    SubActionRegistry, effective_config,
};
use forge_types::{ArgStack, EventId, SubActionOutcome, SubActionStep, SubActionTelemetry};
use serde_json::json;
use tracing::warn;

use crate::Config;
use crate::action_engine::skipped_telemetry;

pub struct ChainEngine {
    registry: Arc<SubActionRegistry>,
    publisher: Arc<dyn EventPublisher>,
    config: Config,
}

pub struct ChainRun {
    pub signal: ChainSignal,
    pub arg_stack: ArgStack,
    pub telemetry: Vec<SubActionTelemetry>,
}

impl ChainEngine {
    pub fn new(
        registry: Arc<SubActionRegistry>,
        publisher: Arc<dyn EventPublisher>,
        config: Config,
    ) -> Self {
        Self {
            registry,
            publisher,
            config,
        }
    }

    /// Scope for the action's top-level chain (depth 0); its steps run at depth 1.
    pub fn root_scope(self: &Arc<Self>, cancel: CancelSignal) -> ChainScope {
        ChainScope {
            engine: Arc::clone(self),
            depth: 0,
            cancel,
        }
    }

    pub async fn run_sequential(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        cancel: &CancelSignal,
    ) -> ChainRun {
        let mut current = arg_stack.clone();
        let mut telemetry = Vec::new();

        for (index, step) in steps.iter().enumerate() {
            if !step.enabled {
                continue;
            }
            if cancel.is_cancelled() {
                return ChainRun {
                    signal: ChainSignal::Aborted,
                    arg_stack: current,
                    telemetry,
                };
            }

            let run_event = Event::caused_by(
                EventSource::Core,
                "subaction.run",
                json!({ "step_index": index, "kind": step.kind_id }),
                parent_event_id,
            );
            let run_event_id = run_event.id;
            self.publisher.publish(run_event);

            let run_ctx = RunContext {
                arg_stack: &current,
                index,
                parent_event_id: run_event_id,
                publisher: self.publisher.as_ref(),
            };

            let (tel, updated) = match self.registry.get(&step.kind_id) {
                Some(runner) => {
                    let resolved = effective_config(&runner.default_config(), &step.config);
                    runner.execute(&resolved, &run_ctx).await
                }
                None => {
                    warn!(
                        "unknown sub-action kind_id: {} — skipping step",
                        step.kind_id
                    );
                    (skipped_telemetry(index, &step.kind_id), None)
                }
            };

            if let Some(new_stack) = updated {
                current = new_stack;
            }

            let failure = match &tel.outcome {
                SubActionOutcome::Failed(m) => Some(m.clone()),
                _ => None,
            };
            telemetry.push(tel);

            if let Some(msg) = failure {
                return ChainRun {
                    signal: ChainSignal::Error(msg),
                    arg_stack: current,
                    telemetry,
                };
            }
        }

        ChainRun {
            signal: ChainSignal::Completed,
            arg_stack: current,
            telemetry,
        }
    }

    pub async fn run_concurrent(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        cancel: &CancelSignal,
    ) -> ChainRun {
        use futures_util::future::join_all;

        if cancel.is_cancelled() {
            return ChainRun {
                signal: ChainSignal::Aborted,
                arg_stack: arg_stack.clone(),
                telemetry: Vec::new(),
            };
        }

        let futures: Vec<_> = steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.enabled)
            .map(|(index, step)| {
                let run_event = Event::caused_by(
                    EventSource::Core,
                    "subaction.run",
                    json!({ "step_index": index, "kind": step.kind_id }),
                    parent_event_id,
                );
                let run_event_id = run_event.id;
                self.publisher.publish(run_event);

                let run_ctx = RunContext {
                    arg_stack,
                    index,
                    parent_event_id: run_event_id,
                    publisher: self.publisher.as_ref(),
                };

                async move {
                    match self.registry.get(&step.kind_id) {
                        Some(runner) => {
                            let resolved = effective_config(&runner.default_config(), &step.config);
                            runner.execute(&resolved, &run_ctx).await
                        }
                        None => {
                            warn!(
                                "unknown sub-action kind_id: {} — skipping step",
                                step.kind_id
                            );
                            (skipped_telemetry(index, &step.kind_id), None)
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;

        let mut telemetry = Vec::new();
        let mut first_failure: Option<String> = None;
        for (tel, _) in results {
            if first_failure.is_none()
                && let SubActionOutcome::Failed(msg) = &tel.outcome
            {
                first_failure = Some(msg.clone());
            }
            telemetry.push(tel);
        }

        let signal = match first_failure {
            Some(msg) => ChainSignal::Error(msg),
            None => ChainSignal::Completed,
        };

        ChainRun {
            signal,
            arg_stack: arg_stack.clone(),
            telemetry,
        }
    }
}

pub struct ChainScope {
    engine: Arc<ChainEngine>,
    depth: u32,
    cancel: CancelSignal,
}

#[async_trait]
impl ChainExecutor for ChainScope {
    async fn run_child_chain(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError> {
        let child_depth = self.depth + 1;
        if child_depth > self.engine.config.max_nesting_depth {
            return Err(RegistryError::DepthExceeded(child_depth));
        }

        let run = self
            .engine
            .run_sequential(steps, arg_stack, parent_event_id, &self.cancel)
            .await;

        Ok(ChildChainOutcome {
            signal: run.signal,
            arg_stack: run.arg_stack,
            telemetry: run.telemetry,
        })
    }

    fn cancel_signal(&self) -> CancelSignal {
        self.cancel.clone()
    }
}
