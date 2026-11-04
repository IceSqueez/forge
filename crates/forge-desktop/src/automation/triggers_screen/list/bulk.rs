use super::*;
use forge_components::{icon_button, selection_bar};

fn count_arg(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

struct BulkArchive {
    archived: Vec<TriggerInstanceId>,
    failure: Option<RepoFailure>,
    rows: Result<Vec<TriggerInstanceRow>, RepoFailure>,
}

impl TriggerList {
    fn marked_where(&self, keep: impl Fn(&TriggerInstanceRow) -> bool) -> Vec<TriggerInstanceId> {
        self.marked
            .ids()
            .iter()
            .copied()
            .filter(|id| self.find(*id).is_some_and(&keep))
            .collect()
    }

    fn bulk_set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.menu_open = None;
        let changing = self.marked_where(|row| row.enabled != enabled);
        if changing.is_empty() {
            cx.notify();
            return;
        }
        let used = changing
            .iter()
            .filter(|id| self.find(**id).is_some_and(|row| row.used_in_count > 0))
            .count();
        if !enabled && used > 0 {
            self.confirm_bulk_disable.request(changing);
            cx.notify();
            return;
        }
        self.persist_bulk_enabled(changing, enabled, cx);
    }

    fn persist_bulk_enabled(
        &mut self,
        ids: Vec<TriggerInstanceId>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        for id in &ids {
            self.show_enabled(*id, enabled, cx);
        }
        cx.notify();
        let repo = Arc::clone(&self.deps.repo);
        self.spawn_write(
            async move {
                repo.set_all_enabled(&ids, enabled)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn confirm_bulk_disable_now(&mut self, cx: &mut Context<Self>) {
        match self.confirm_bulk_disable.take() {
            Some(ids) => self.persist_bulk_enabled(ids, false, cx),
            None => cx.notify(),
        }
    }

    fn cancel_bulk_disable(&mut self, cx: &mut Context<Self>) {
        self.confirm_bulk_disable.cancel();
        cx.notify();
    }

    fn request_bulk_delete(&mut self, cx: &mut Context<Self>) {
        if !self.marked.is_bulk() {
            return;
        }
        self.menu_open = None;
        self.pending_bulk_delete.request(self.marked.ids().to_vec());
        cx.notify();
    }

    fn cancel_bulk_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_bulk_delete.cancel();
        cx.notify();
    }

    fn deletable(&self, ids: &[TriggerInstanceId]) -> Vec<TriggerInstanceId> {
        ids.iter()
            .copied()
            .filter(|id| self.find(*id).is_some_and(|row| row.used_in_count == 0))
            .collect()
    }

    fn confirm_bulk_delete(&mut self, cx: &mut Context<Self>) {
        let Some(ids) = self.pending_bulk_delete.take() else {
            return;
        };
        let deletable = self.deletable(&ids);
        cx.notify();
        if deletable.is_empty() {
            return;
        }
        self.clear_marked(cx);
        let repo = Arc::clone(&self.deps.repo);
        let restore_repo = Arc::clone(&self.deps.repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                let mut archived = Vec::with_capacity(deletable.len());
                let mut failure = None;
                for id in deletable {
                    match repo.archive(id).await {
                        Ok(_) => archived.push(id),
                        Err(error) => {
                            failure = Some(RepoFailure::from(error));
                            break;
                        }
                    }
                }
                let rows = load_rows(&*repo).await;
                BulkArchive {
                    archived,
                    failure,
                    rows,
                }
            },
            move |this, outcome, cx| this.land_bulk_archive(outcome, restore_repo, cx),
            cx,
        );
    }

    fn land_bulk_archive(
        &mut self,
        outcome: BulkArchive,
        restore_repo: Arc<dyn TriggerInstanceRepo>,
        cx: &mut Context<Self>,
    ) {
        if let Some(failure) = &outcome.failure {
            on_repo_error(failure, cx);
        }
        match outcome.rows {
            Ok(rows) => self.apply_rows(rows, cx),
            Err(failure) => on_repo_error(&failure, cx),
        }
        if !outcome.archived.is_empty() {
            self.raise_bulk_undo_toast(outcome.archived, restore_repo, cx);
        }
    }

    fn raise_bulk_undo_toast(
        &self,
        ids: Vec<TriggerInstanceId>,
        repo: Arc<dyn TriggerInstanceRepo>,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let message = tr!("bulk_deleted_triggers_toast", count = count_arg(ids.len()));
        cx.push_undo_toast(message, move |_window, app: &mut App| {
            let repo = Arc::clone(&repo);
            let ids = ids.clone();
            view.update(app, |this, cx| {
                this.spawn_write(
                    async move {
                        for id in ids {
                            repo.restore(id).await.map_err(RepoFailure::from)?;
                        }
                        Ok(())
                    },
                    cx,
                );
            });
        });
    }

    pub(super) fn render_bulk_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.marked.is_bulk() {
            return None;
        }
        let density = cx.density();
        let any_disabled = !self.marked_where(|row| !row.enabled).is_empty();
        let any_enabled = !self.marked_where(|row| row.enabled).is_empty();
        let bar = selection_bar(
            tr!("bulk_selected_count", count = count_arg(self.marked.len())),
            palette,
        )
        .density(density)
        .action(
            icon_button(Icon::CircleCheckFilled, palette)
                .tooltip(tr!("bulk_enable_selected", count = count_arg(self.marked_where(|row| !row.enabled).len())))
                .disabled(!any_disabled)
                .on_click(
                    "triggers-bulk-enable",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.bulk_set_enabled(true, cx)),
                ),
        )
        .action(
            icon_button(Icon::Circle, palette)
                .tooltip(tr!("bulk_disable_selected", count = count_arg(self.marked_where(|row| row.enabled).len())))
                .disabled(!any_enabled)
                .on_click(
                    "triggers-bulk-disable",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.bulk_set_enabled(false, cx)),
                ),
        )
        .action(
            icon_button(Icon::Eraser, palette)
                .tooltip(tr!("bulk_delete_selected", count = count_arg(self.marked.len())))
                .ink(palette.danger)
                .on_click(
                    "triggers-bulk-delete",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.request_bulk_delete(cx)),
                ),
        )
        .clear(
            icon_button(Icon::X, palette)
                .tooltip(tr!("bulk_clear"))
                .on_click(
                    "triggers-bulk-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_marked(cx)),
                ),
        );
        Some(bar.into_any_element())
    }

    pub(super) fn render_bulk_disable_confirm(
        &self,
        ids: &[TriggerInstanceId],
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let used = ids
            .iter()
            .filter(|id| self.find(**id).is_some_and(|row| row.used_in_count > 0))
            .count();
        let card = confirm_modal(
            tr!("bulk_disable_triggers_title", count = count_arg(ids.len())),
            tr!("bulk_disable_triggers_body", count = count_arg(used)),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "triggers-bulk-disable-cancel",
            tr!("triggers_confirm_disable_dismiss"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_bulk_disable(cx)),
        )
        .on_confirm(
            "triggers-bulk-disable-confirm",
            tr!("triggers_confirm_disable_accept"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_bulk_disable_now(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-bulk-disable-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_bulk_disable(cx));
            })
            .into_any_element()
    }

    pub(super) fn render_bulk_delete_confirm(
        &self,
        ids: &[TriggerInstanceId],
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let deletable = self.deletable(ids).len();
        let kept = ids.len().saturating_sub(deletable);
        let body = match (deletable, kept) {
            (0, _) => tr!("bulk_delete_triggers_blocked"),
            (_, 0) => tr!("bulk_delete_triggers_body"),
            _ => tr!("bulk_delete_triggers_body_kept", kept = count_arg(kept)),
        };
        let mut card = confirm_modal(
            tr!("bulk_delete_triggers_title", count = count_arg(deletable)),
            body,
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "triggers-bulk-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_bulk_delete(cx)),
        );
        if deletable > 0 {
            card = card.on_confirm(
                "triggers-bulk-delete-confirm",
                tr!("bulk_delete_selected", count = count_arg(deletable)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_bulk_delete(cx)),
            );
        }

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-bulk-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_bulk_delete(cx));
            })
            .into_any_element()
    }
}
