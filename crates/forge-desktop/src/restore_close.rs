use std::path::PathBuf;

use forge_components::{PushToast, ToastKind, tr};
use forge_transfer::{
    StagedRestore, TransferError, UpdateActivity, cancel_pending_restore, discard_staged_restore,
};
use gpui::{App, Context, Entity, EventEmitter, Global};
use tokio::runtime::Handle;

use crate::async_bridge::run_async;
use crate::update_exit::ExitHandoff;
use crate::update_flow::UpdateFlow;

pub enum RestoreCloseEvent {
    CloseRequested,
}

enum PendingClose {
    Restore(StagedRestore),
    Restart,
}

pub struct RestoreClose {
    pending: Option<PendingClose>,
    update_flow: Entity<UpdateFlow>,
    data_dir: PathBuf,
    rt_handle: Handle,
}

pub struct RestoreRoute(pub Entity<RestoreClose>);

impl Global for RestoreRoute {}

impl EventEmitter<RestoreCloseEvent> for RestoreClose {}

impl RestoreClose {
    pub fn new(update_flow: Entity<UpdateFlow>, data_dir: PathBuf, rt_handle: Handle) -> Self {
        Self {
            pending: None,
            update_flow,
            data_dir,
            rt_handle,
        }
    }

    pub fn route(cx: &App) -> Option<Entity<RestoreClose>> {
        cx.try_global::<RestoreRoute>().map(|route| route.0.clone())
    }

    pub fn update_activity(&self, cx: &App) -> UpdateActivity {
        if self.update_flow.read(cx).has_staged_update() {
            UpdateActivity::Staged
        } else {
            UpdateActivity::Idle
        }
    }

    pub fn request(&mut self, staged: StagedRestore, cx: &mut Context<Self>) {
        self.arm(PendingClose::Restore(staged), cx);
    }

    pub fn restart(&mut self, cx: &mut Context<Self>) {
        if matches!(self.pending, Some(PendingClose::Restore(_))) {
            return;
        }
        self.arm(PendingClose::Restart, cx);
    }

    fn arm(&mut self, pending: PendingClose, cx: &mut Context<Self>) {
        self.pending = Some(pending);
        if let Some(exit) = cx.try_global::<ExitHandoff>() {
            exit.arm_restore();
        }
        cx.emit(RestoreCloseEvent::CloseRequested);
        cx.notify();
    }

    pub fn abort(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if let Some(exit) = cx.try_global::<ExitHandoff>() {
            exit.disarm_restore();
        }
        let PendingClose::Restore(staged) = pending else {
            cx.notify();
            return;
        };
        let data_dir = self.data_dir.clone();
        run_async(
            &self.rt_handle,
            withdraw(data_dir, staged),
            |_: &mut Self, result, cx| match result {
                Ok(()) => cx.push_toast(ToastKind::Info, tr!("restore_close_cancelled")),
                Err(error) => {
                    tracing::warn!(error = %error, "cancelled restore could not be withdrawn");
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("restore_close_cancel_failed", error = error.as_str()),
                    );
                }
            },
            cx,
        );
        cx.notify();
    }
}

async fn withdraw(data_dir: PathBuf, staged: StagedRestore) -> Result<(), String> {
    let cancelled = cancel_pending_restore(&data_dir).await;
    let discarded = discard_staged_restore(staged).await;
    cancelled
        .and(discarded)
        .map_err(|error: TransferError| error.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    use forge_components::{ToastKind, Toasts};
    use forge_storage::{DataProvider, Language};
    use forge_transfer::{
        RESTORE_MARKER_FILE, StagedRestore, UpdateActivity, record_pending_restore,
    };
    use gpui::{AppContext, Entity, TestAppContext};
    use tempfile::TempDir;

    use super::{RestoreClose, RestoreCloseEvent};
    use crate::i18n::install_language;
    use crate::test_support::{pump, runtime, staged_restore_in, test_backend};
    use crate::update_exit::ExitHandoff;
    use crate::update_flow::UpdateFlow;

    struct Harness {
        close: Entity<RestoreClose>,
        exit: ExitHandoff,
        data: TempDir,
        rt: tokio::runtime::Runtime,
    }

    fn harness(cx: &mut TestAppContext) -> Harness {
        install_language(Language::En);
        let rt = runtime();
        let data = TempDir::new().unwrap();
        let (backend, _) = test_backend();
        let exit = ExitHandoff::new(None);
        cx.update(|cx| {
            cx.set_global(Toasts::new());
            cx.set_global(exit.clone());
        });
        let handle = rt.handle().clone();
        let data_dir = data.path().to_path_buf();
        let close = cx.new(|cx| {
            let update_flow = cx.new(|_| {
                UpdateFlow::new(
                    backend as Arc<dyn DataProvider>,
                    handle.clone(),
                    data_dir.clone(),
                )
            });
            RestoreClose::new(update_flow, data_dir, handle)
        });
        Harness {
            close,
            exit,
            data,
            rt,
        }
    }

    fn recorded_restore(h: &Harness) -> StagedRestore {
        let staged = staged_restore_in(&h.rt, h.data.path());
        h.rt.block_on(record_pending_restore(
            h.data.path(),
            &staged,
            UpdateActivity::Idle,
        ))
        .unwrap();
        staged
    }

    fn toast_kinds(cx: &mut TestAppContext) -> Vec<ToastKind> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| toast.kind)
                .collect()
        })
    }

    fn settle_withdrawal(cx: &mut TestAppContext, h: &Harness) -> Vec<ToastKind> {
        loop {
            pump(&h.rt);
            cx.run_until_parked();
            let kinds = toast_kinds(cx);
            if !kinds.is_empty() {
                return kinds;
            }
        }
    }

    #[gpui::test]
    fn requesting_a_restore_arms_the_relaunch_and_asks_to_close(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staged = recorded_restore(&h);
        let asked = Rc::new(Cell::new(0));
        let sink = Rc::clone(&asked);
        cx.update(|cx| {
            cx.subscribe(
                &h.close,
                move |_, event: &RestoreCloseEvent, _| match event {
                    RestoreCloseEvent::CloseRequested => sink.set(sink.get() + 1),
                },
            )
            .detach();
        });

        h.close.update(cx, |close, cx| close.request(staged, cx));
        cx.run_until_parked();

        assert_eq!((h.exit.restore_is_armed(), asked.get()), (true, 1));
    }

    #[gpui::test]
    fn aborting_the_close_withdraws_the_marker_and_the_staging(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staged = recorded_restore(&h);
        let staging = staged.staging_dir().to_path_buf();
        h.close.update(cx, |close, cx| close.request(staged, cx));

        h.close.update(cx, |close, cx| close.abort(cx));
        let toasts = settle_withdrawal(cx, &h);

        assert_eq!(
            (
                toasts,
                h.data.path().join(RESTORE_MARKER_FILE).exists(),
                staging.exists(),
            ),
            (vec![ToastKind::Info], false, false)
        );
    }

    #[gpui::test]
    fn aborting_the_close_disarms_the_restore_relaunch(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staged = recorded_restore(&h);
        h.close.update(cx, |close, cx| close.request(staged, cx));

        h.close.update(cx, |close, cx| close.abort(cx));
        settle_withdrawal(cx, &h);

        assert!(h.exit.is_disarmed());
    }

    #[gpui::test]
    fn aborting_with_nothing_requested_leaves_a_recorded_restore_alone(cx: &mut TestAppContext) {
        let h = harness(cx);
        recorded_restore(&h);

        h.close.update(cx, |close, cx| close.abort(cx));
        pump(&h.rt);
        cx.run_until_parked();

        assert_eq!(
            (
                h.data.path().join(RESTORE_MARKER_FILE).exists(),
                toast_kinds(cx)
            ),
            (true, vec![])
        );
    }

    #[gpui::test]
    fn restarting_arms_the_relaunch_and_asks_to_close(cx: &mut TestAppContext) {
        let h = harness(cx);
        let asked = Rc::new(Cell::new(0));
        let sink = Rc::clone(&asked);
        cx.update(|cx| {
            cx.subscribe(
                &h.close,
                move |_, event: &RestoreCloseEvent, _| match event {
                    RestoreCloseEvent::CloseRequested => sink.set(sink.get() + 1),
                },
            )
            .detach();
        });

        h.close.update(cx, |close, cx| close.restart(cx));
        cx.run_until_parked();

        assert_eq!((h.exit.restore_is_armed(), asked.get()), (true, 1));
    }

    #[gpui::test]
    fn a_restart_requested_over_a_pending_restore_keeps_the_restore_to_withdraw(
        cx: &mut TestAppContext,
    ) {
        let h = harness(cx);
        let staged = recorded_restore(&h);
        let staging = staged.staging_dir().to_path_buf();
        h.close.update(cx, |close, cx| close.request(staged, cx));
        h.close.update(cx, |close, cx| close.restart(cx));

        h.close.update(cx, |close, cx| close.abort(cx));
        let toasts = settle_withdrawal(cx, &h);

        assert_eq!((toasts, staging.exists()), (vec![ToastKind::Info], false));
    }

    #[gpui::test]
    fn aborting_a_restart_disarms_the_relaunch_and_withdraws_nothing(cx: &mut TestAppContext) {
        let h = harness(cx);
        recorded_restore(&h);
        h.close.update(cx, |close, cx| close.restart(cx));

        h.close.update(cx, |close, cx| close.abort(cx));
        pump(&h.rt);
        cx.run_until_parked();

        assert_eq!(
            (
                h.exit.is_disarmed(),
                h.data.path().join(RESTORE_MARKER_FILE).exists(),
                toast_kinds(cx)
            ),
            (true, true, vec![])
        );
    }
}
