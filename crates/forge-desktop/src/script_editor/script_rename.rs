use super::ScriptEditorView;
use super::console::LogTag;
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use crate::script_lifecycle::{ScriptRenamed, counted_names, has_referrers, referrer_count};
use forge_components::PushToast;
use forge_components::{
    ActivePresentation, FONT_XS, InlineEdit, InlineEditEvent, ToastKind, inline_edit, tr,
};
use forge_storage::ScriptReferences;
use forge_types::ScriptId;
use gpui::{Context, Entity, Subscription, Window};

pub(super) struct PendingRename {
    pub(super) id: ScriptId,
    pub(super) old_name: String,
    pub(super) new_name: String,
    pub(super) references: ScriptReferences,
}

pub(super) struct RenameState {
    pub(super) target: ScriptId,
    pub(super) editor: Entity<InlineEdit>,
    pub(super) _sub: Subscription,
}

impl ScriptEditorView {
    pub(super) fn start_rename(
        &mut self,
        id: ScriptId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.find_entry(id).map(|e| e.name.clone()) else {
            return;
        };
        let palette = cx.palette();
        let editor = inline_edit(current, palette, FONT_XS, window, cx);
        let sub = cx.subscribe(
            &editor,
            |this, _e, event: &InlineEditEvent, cx| match event {
                InlineEditEvent::Commit(next) => this.commit_rename(next.clone(), cx),
                InlineEditEvent::Cancel => this.cancel_rename(cx),
            },
        );
        self.row_menu = None;
        self.rename = Some(RenameState {
            target: id,
            editor,
            _sub: sub,
        });
        cx.notify();
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(state) = self.rename.take() else {
            return;
        };
        cx.notify();
        let id = state.target;
        let name = name.trim().to_owned();
        let Some(old_name) = self.find_entry(id).map(|e| e.name.clone()) else {
            return;
        };
        if name.is_empty() || name == old_name {
            return;
        }
        let taken = self
            .scripts
            .iter()
            .any(|e| e.id != id && e.name.to_lowercase() == name.to_lowercase());
        if taken {
            self.push_console(
                LogTag::Err,
                tr!("script_console_name_taken", name = name.as_str()),
            );
            return;
        }

        let lifecycle = self.lifecycle();
        let lookup = old_name.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { lifecycle.referrers(lookup).await },
            move |this, result, cx| {
                this.stage_rename(
                    PendingRename {
                        id,
                        old_name,
                        new_name: name,
                        references: ScriptReferences::default(),
                    },
                    result,
                    cx,
                );
            },
            cx,
        );
    }

    pub(super) fn stage_rename(
        &mut self,
        pending: PendingRename,
        result: Result<ScriptReferences, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(references) if has_referrers(&references) => {
                self.pending_rename = Some(PendingRename {
                    references,
                    ..pending
                });
                cx.notify();
            }
            Ok(_) => self.run_rename(pending.id, pending.new_name, cx),
            Err(error) => self.on_rename_failed(&error.text(), cx),
        }
    }

    pub(super) fn cancel_pending_rename(&mut self, cx: &mut Context<Self>) {
        self.pending_rename = None;
        cx.notify();
    }

    pub(super) fn confirm_pending_rename(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_rename.take() else {
            return;
        };
        self.run_rename(pending.id, pending.new_name, cx);
        cx.notify();
    }

    fn run_rename(&mut self, id: ScriptId, name: String, cx: &mut Context<Self>) {
        let lifecycle = self.lifecycle();
        async_bridge::run_async(
            &self.rt_handle,
            async move { lifecycle.rename(id, name).await },
            |this, result, cx| this.apply_renamed(result, cx),
            cx,
        );
    }

    fn apply_renamed(
        &mut self,
        result: Result<ScriptRenamed, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        let renamed = match result {
            Ok(renamed) => renamed,
            Err(error) => {
                self.on_rename_failed(&error.text(), cx);
                return;
            }
        };
        for entry in &mut self.scripts {
            if entry.id == renamed.id {
                entry.name = renamed.name.clone();
            }
        }
        if let Some(open) = self.open.as_mut()
            && open.id == renamed.id
        {
            open.record.name = renamed.name.clone();
        }
        let literal = &renamed.references.literal;
        cx.push_toast(
            ToastKind::Success,
            tr!(
                "script_editor_renamed_toast",
                name = renamed.name.as_str(),
                count = referrer_count(literal)
            ),
        );
        let templated = &renamed.references.templated;
        if !templated.is_empty() {
            cx.push_toast(
                ToastKind::Warn,
                counted_names("script_editor_rename_check_toast", templated),
            );
        }
        cx.notify();
    }

    fn on_rename_failed(&mut self, error: &str, cx: &mut Context<Self>) {
        tracing::warn!(%error, "script rename failed");
        cx.push_toast(
            ToastKind::Error,
            tr!("script_editor_rename_failed", error = error),
        );
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        cx.notify();
    }
}
