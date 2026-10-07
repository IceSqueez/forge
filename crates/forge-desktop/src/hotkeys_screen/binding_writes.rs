use std::sync::Arc;

use forge_components::tr;
use forge_types::TriggerInstanceId;
use gpui::Context;

use super::HotkeysScreenView;
use crate::async_bridge::{self, ErrorSink};
use crate::hotkey_action_modal::BindingDraft;
use crate::hotkey_bindings::{
    do_bind, rebind_combo, relink_action, set_binding_edge, set_binding_enabled,
};

impl HotkeysScreenView {
    pub(super) fn persist_draft(&mut self, draft: BindingDraft, cx: &mut Context<Self>) {
        let reconciler = Arc::clone(&self.reconciler);
        let backend = Arc::clone(&self.backend);
        let BindingDraft {
            instance_id,
            combo,
            edge,
            action_id,
        } = draft;
        let Some(instance_id) = instance_id else {
            self.run_reload(do_bind(reconciler, backend, combo, edge, action_id), cx);
            return;
        };
        let previous = self
            .row_of_instance(instance_id)
            .map(|row| row.combo.clone())
            .unwrap_or_else(|| combo.clone());
        self.run_reload(
            async move {
                rebind_combo(reconciler, Arc::clone(&backend), previous, combo).await?;
                set_binding_edge(Arc::clone(&backend), instance_id, edge).await?;
                relink_action(backend, instance_id, action_id).await
            },
            cx,
        );
    }

    pub(super) fn rebind(&mut self, key: TriggerInstanceId, combo: String, cx: &mut Context<Self>) {
        let Some(previous) = self.row_by_key(key).map(|row| row.combo.clone()) else {
            cx.notify();
            return;
        };
        let reconciler = Arc::clone(&self.reconciler);
        let backend = Arc::clone(&self.backend);
        self.run_reload(rebind_combo(reconciler, backend, previous, combo), cx);
    }

    pub(super) fn run_reload(
        &mut self,
        work: impl Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            work,
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => this.load(cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn toggle_binding(&mut self, key: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(row) = self.bindings.iter_mut().find(|row| row.key == key) else {
            return;
        };
        let previous: Vec<(TriggerInstanceId, bool)> = row
            .halves()
            .map(|(_, half)| (half.instance_id, half.enabled))
            .collect();
        let enabled = !previous.iter().all(|(_, was)| *was);
        let ids: Vec<TriggerInstanceId> = previous.iter().map(|(id, _)| *id).collect();
        for half in [row.press.as_mut(), row.release.as_mut()]
            .into_iter()
            .flatten()
        {
            half.enabled = enabled;
        }
        let backend = Arc::clone(&self.backend);
        async_bridge::optimistic(
            &self.rt_handle,
            previous,
            set_binding_enabled(backend, ids, enabled),
            move |this, previous, _message, cx| {
                if let Some(row) = this.bindings.iter_mut().find(|row| row.key == key) {
                    for half in [row.press.as_mut(), row.release.as_mut()]
                        .into_iter()
                        .flatten()
                    {
                        if let Some((_, was)) =
                            previous.iter().find(|(id, _)| *id == half.instance_id)
                        {
                            half.enabled = *was;
                        }
                    }
                }
                ErrorSink::Toast.report(tr!("hotkeys_toggle_binding_failed"), cx);
            },
            cx,
        );
        cx.notify();
    }
}
