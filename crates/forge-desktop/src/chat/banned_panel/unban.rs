use forge_components::tr;
use forge_types::PlatformId;
use gpui::Context;

use super::ban_group::UnbanState;
use super::{BannedPanel, UNBAN_RELIST_SUPPRESSION};
use crate::async_bridge;
use crate::chat::viewer_actions::ViewerAction;
use crate::repo_failure::RepoFailure;

impl BannedPanel {
    pub(super) fn unban(
        &mut self,
        platform: PlatformId,
        viewer_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .group_mut(platform)
            .and_then(|group| group.row_mut(&viewer_id))
        else {
            return;
        };
        if !row.can_unban(platform) {
            return;
        }
        let target = row.target(platform);
        let Some(step) = target.step(&ViewerAction::Unban) else {
            return;
        };
        row.unban = UnbanState::Running;

        let engine = self.action_engine.clone();
        let builtin_id = target.builtin_id();
        let label = target.label(&ViewerAction::Unban);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = async_bridge::run_quick_step(engine, step, builtin_id, label).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err(RepoFailure::localized(tr!("chat_dispatch_cancelled"))));
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                this.apply_unban(platform, &viewer_id, outcome, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn apply_unban(
        &mut self,
        platform: PlatformId,
        viewer_id: &str,
        outcome: Result<(), RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let Some(group) = self.group_mut(platform) else {
            return;
        };
        match outcome {
            Ok(()) => {
                group.rows.retain(|row| row.entry.viewer_id != viewer_id);
                group
                    .recently_unbanned
                    .push((viewer_id.to_owned(), now + UNBAN_RELIST_SUPPRESSION));
            }
            Err(error) => {
                if let Some(row) = group.row_mut(viewer_id) {
                    row.unban = UnbanState::Failed(error.text());
                }
            }
        }
        cx.notify();
    }
}
