use super::*;

impl ActionEditor {
    pub(in crate::actions_screen) fn open_action(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.detail = None;
        self.telemetry = None;
        self.last_outcome = None;
        self.nav_path.clear();
        self.step_open = None;
        self.sync_case_fields(cx);
        self.load_detail_for(id, cx);
        cx.notify();
    }

    pub(in crate::actions_screen) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.detail = None;
        self.telemetry = None;
        self.last_outcome = None;
        self.nav_path.clear();
        self.case_fields.clear();
        cx.notify();
    }

    pub(in crate::actions_screen) fn reload_detail(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected {
            self.load_detail_for(id, cx);
        }
    }

    pub(super) fn load_detail_for(&self, id: ActionId, cx: &mut Context<Self>) {
        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { service.load_detail(id).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(detail) => this.apply_detail(id, detail, cx),
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );

        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { service.load_telemetry(id).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(telemetry) => this.apply_telemetry(id, telemetry, cx),
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );

        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { service.recent_runs(id, 1).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(runs) => {
                    let outcome = runs.into_iter().next().map(|ctx| ctx.outcome);
                    this.apply_last_outcome(id, outcome, cx);
                }
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    pub(super) fn apply_detail(
        &mut self,
        id: ActionId,
        mut detail: ActionDetail,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        if let Some(latest) = self.latest_written_action(id) {
            detail.action = latest.clone();
        } else if self.has_pending_action_writes(id)
            && let Some(current) = self.detail.as_ref()
            && current.action.id == id
        {
            detail.action = current.action.clone();
        }
        cx.emit(ActionEditorEvent::TriggerKindsRead {
            id,
            kinds: detail
                .trigger_instances
                .iter()
                .map(|instance| instance.kind_id.clone())
                .collect(),
        });
        self.detail = Some(detail);
        self.sync_case_fields(cx);
        self.recompute_step_health();
        cx.notify();
    }

    pub(super) fn recompute_step_health(&mut self) {
        self.step_health = match &self.detail {
            Some(detail) => {
                let mut health = forge_registry::analyze(
                    &detail.action,
                    &detail.trigger_instances,
                    &detail.last_step_outcomes,
                    &self.deps.sub_action_registry,
                    &self.deps.trigger_registry,
                );
                if let Some(integrations) = &self.integrations {
                    forge_registry::flag_switched_off(
                        &detail.action.sub_actions,
                        &self.deps.sub_action_registry,
                        integrations.off(),
                        &mut health,
                    );
                    forge_registry::flag_failed(
                        &detail.action.sub_actions,
                        &self.deps.sub_action_registry,
                        integrations.failed(),
                        &mut health,
                    );
                }
                health
            }
            None => Vec::new(),
        };
    }

    fn apply_telemetry(
        &mut self,
        id: ActionId,
        telemetry: ActionTelemetry,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        self.telemetry = Some(telemetry);
        cx.notify();
    }

    fn apply_last_outcome(
        &mut self,
        id: ActionId,
        outcome: Option<ExecutionOutcome>,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        self.last_outcome = outcome;
        cx.notify();
    }

    pub(super) fn persist_chain_mutation(
        &mut self,
        mutate: impl FnOnce(&mut Vec<nav::ChainItem>),
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let id = detail.action.id;
        let path = self.nav_path.clone();
        let mut chain = nav::resolve_chain(&detail.action.sub_actions, &path);
        let unchanged = chain.clone();
        mutate(&mut chain);
        if chain == unchanged {
            return;
        }
        self.write_action(id, action_writes::ActionWrite::SetChain { path, chain }, cx);
    }
}
