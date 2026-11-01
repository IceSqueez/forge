use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_storage::ActionRepo;
use forge_types::{ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_action_shared::{ACTION, ACTION_SPEC};
use crate::core_messages;

#[derive(Clone, Copy)]
struct StepCopy {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon_name: &'static str,
}

#[derive(Clone, Copy)]
enum StateChange {
    Set(bool),
    Flip,
}

const ENABLE_COPY: StepCopy = StepCopy {
    id: "core.action.enable",
    label: "Enable Action",
    summary: "Allow an action to run when triggered",
    search_text: "enable action allow activate",
    icon_name: "toggle-on",
};

const DISABLE_COPY: StepCopy = StepCopy {
    id: "core.action.disable",
    label: "Disable Action",
    summary: "Prevent an action from running when triggered",
    search_text: "disable action block deactivate stop",
    icon_name: "toggle-off",
};

const TOGGLE_COPY: StepCopy = StepCopy {
    id: "core.action.toggle",
    label: "Toggle Action",
    summary: "Flip an action between enabled and disabled",
    search_text: "toggle action flip switch enable disable",
    icon_name: "switch",
};

pub struct CoreActionStateRunner {
    actions: Arc<dyn ActionRepo>,
    change: StateChange,
    copy: StepCopy,
}

impl CoreActionStateRunner {
    pub fn enable(actions: Arc<dyn ActionRepo>) -> Self {
        Self {
            actions,
            change: StateChange::Set(true),
            copy: ENABLE_COPY,
        }
    }

    pub fn disable(actions: Arc<dyn ActionRepo>) -> Self {
        Self {
            actions,
            change: StateChange::Set(false),
            copy: DISABLE_COPY,
        }
    }

    pub fn toggle(actions: Arc<dyn ActionRepo>) -> Self {
        Self {
            actions,
            change: StateChange::Flip,
            copy: TOGGLE_COPY,
        }
    }

    async fn run(&self, action_id: ActionId) -> SubActionOutcome {
        let found = match self.change {
            StateChange::Set(enabled) => self.actions.set_enabled(action_id, enabled).await,
            StateChange::Flip => self
                .actions
                .toggle_enabled(action_id)
                .await
                .map(|state| state.is_some()),
        };
        match found {
            Ok(true) => SubActionOutcome::Success,
            Ok(false) => SubActionOutcome::Failed(core_messages::unknown_action(action_id).into()),
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        }
    }
}

impl StepCatalogEntry for CoreActionStateRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }
    fn label(&self) -> &str {
        self.copy.label
    }
    fn summary(&self) -> &str {
        self.copy.summary
    }
    fn search_text(&self) -> &str {
        self.copy.search_text
    }
    fn icon_name(&self) -> &str {
        self.copy.icon_name
    }
}

#[async_trait]
impl SubActionRunner for CoreActionStateRunner {
    fn id(&self) -> &str {
        self.copy.id
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ACTION_SPEC)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let action_id = match ACTION.read(config, ctx.arg_stack) {
            Ok(action_id) => action_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(action_id).await;
        (timer.finish(outcome), None)
    }
}
