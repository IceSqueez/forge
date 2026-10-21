use std::path::PathBuf;

use forge_components::{PushToast, ToastKind, tr};
use forge_transfer::{
    ScheduledBackupHandle, StagedRestore, TransferError, UpdateActivity, cancel_pending_restore,
    discard_staged_restore,
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
    scheduled: Option<ScheduledBackupHandle>,
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
            scheduled: None,
        }
    }

    #[must_use]
    pub fn with_scheduled_backups(mut self, scheduled: ScheduledBackupHandle) -> Self {
        self.scheduled = Some(scheduled);
        self
    }

    fn suspend_scheduled_backups(&self, suspended: bool) {
        if let Some(scheduled) = &self.scheduled {
            scheduled.set_suspended(suspended);
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
        if matches!(pending, PendingClose::Restore(_)) {
            self.suspend_scheduled_backups(true);
        }
        self.pending = Some(pending);
        if let Some(exit) = cx.try_global::<ExitHandoff>() {
            exit.arm_restore();
        }
        cx.emit(RestoreCloseEvent::CloseRequested);
        cx.notify();
    }

    pub fn abort(&mut self, cx: &mut Context<Self>) {
        self.withdraw(WithdrawNotice::Cancelled, cx);
    }

    pub fn yield_to_update(&mut self, cx: &mut Context<Self>) {
        if matches!(self.pending, Some(PendingClose::Restore(_))) {
            self.withdraw(WithdrawNotice::YieldedToUpdate, cx);
        }
    }

    fn withdraw(&mut self, notice: WithdrawNotice, cx: &mut Context<Self>) {
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
        self.suspend_scheduled_backups(false);
        let data_dir = self.data_dir.clone();
        run_async(
            &self.rt_handle,
            withdraw(data_dir, staged),
            move |_: &mut Self, result, cx| match result {
                Ok(()) => cx.push_toast(notice.kind(), notice.message()),
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

#[derive(Clone, Copy)]
enum WithdrawNotice {
    Cancelled,
    YieldedToUpdate,
}

impl WithdrawNotice {
    fn kind(self) -> ToastKind {
        match self {
            Self::Cancelled => ToastKind::Info,
            Self::YieldedToUpdate => ToastKind::Warn,
        }
    }

    fn message(self) -> String {
        match self {
            Self::Cancelled => tr!("restore_close_cancelled"),
            Self::YieldedToUpdate => tr!("restore_close_yielded_to_update"),
        }
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    use forge_components::{ToastKind, Toasts, tr};
    use forge_storage::{DataProvider, Language};
    use forge_transfer::{
        RESTORE_MARKER_FILE, ScheduleState, ScheduledBackupHandle, ScheduledBackupStatus,
        StagedRestore, UpdateActivity, record_pending_restore,
    };
    use gpui::{AppContext, Entity, TestAppContext};
    use tempfile::TempDir;

    use super::{RestoreClose, RestoreCloseEvent};
    use crate::i18n::install_language;
    use crate::test_support::{
        GatedSchedule, keyless_enabled_schedule, pump, runtime, staged_restore_in, test_backend,
    };
    use crate::update_exit::ExitHandoff;
    use crate::update_flow::UpdateFlow;

    struct Harness {
        close: Entity<RestoreClose>,
        exit: ExitHandoff,
        data: TempDir,
        rt: tokio::runtime::Runtime,
    }

    fn harness(cx: &mut TestAppContext) -> Harness {
        harness_over(cx, runtime(), None)
    }

    fn scheduled_harness(cx: &mut TestAppContext) -> (Harness, GatedSchedule) {
        let rt = runtime();
        let schedule = rt.block_on(async { keyless_enabled_schedule() });
        let scheduled = schedule.handle.clone();
        (harness_over(cx, rt, Some(scheduled)), schedule)
    }

    fn harness_over(
        cx: &mut TestAppContext,
        rt: tokio::runtime::Runtime,
        scheduled: Option<ScheduledBackupHandle>,
    ) -> Harness {
        install_language(Language::En);
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
            let close = RestoreClose::new(update_flow, data_dir, handle);
            match scheduled {
                Some(scheduled) => close.with_scheduled_backups(scheduled),
                None => close,
            }
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

    #[gpui::test]
    fn yielding_to_an_update_withdraws_a_pending_restore_with_a_warning(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staged = recorded_restore(&h);
        let staging = staged.staging_dir().to_path_buf();
        h.close.update(cx, |close, cx| close.request(staged, cx));

        h.close.update(cx, |close, cx| close.yield_to_update(cx));
        settle_withdrawal(cx, &h);
        let toasts: Vec<(ToastKind, String)> = cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| (toast.kind, toast.message.to_string()))
                .collect()
        });

        assert_eq!(
            (
                toasts,
                h.exit.is_disarmed(),
                h.data.path().join(RESTORE_MARKER_FILE).exists(),
                staging.exists(),
            ),
            (
                vec![(ToastKind::Warn, tr!("restore_close_yielded_to_update"))],
                true,
                false,
                false
            )
        );
    }

    #[gpui::test]
    fn yielding_to_an_update_leaves_a_pending_restart_armed(cx: &mut TestAppContext) {
        let h = harness(cx);
        recorded_restore(&h);
        h.close.update(cx, |close, cx| close.restart(cx));

        h.close.update(cx, |close, cx| close.yield_to_update(cx));
        pump(&h.rt);
        cx.run_until_parked();

        assert_eq!(
            (
                h.exit.restore_is_armed(),
                h.data.path().join(RESTORE_MARKER_FILE).exists(),
                toast_kinds(cx)
            ),
            (true, true, vec![])
        );
    }

    const SCHEDULE_SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

    fn settled_state(
        h: &Harness,
        schedule: &GatedSchedule,
        reached: impl Fn(&ScheduledBackupStatus) -> bool,
    ) -> ScheduleState {
        let mut status = schedule.handle.status();
        h.rt.block_on(async {
            tokio::time::timeout(
                SCHEDULE_SETTLE_DEADLINE,
                status.wait_for(|status| reached(status)),
            )
            .await
            .expect("the schedule never reached the awaited state")
            .unwrap()
            .state
        })
    }

    fn idle(status: &ScheduledBackupStatus) -> bool {
        status.state == ScheduleState::Idle
    }

    #[gpui::test]
    fn arming_a_restore_suspends_scheduled_backups(cx: &mut TestAppContext) {
        let (h, schedule) = scheduled_harness(cx);
        settled_state(&h, &schedule, idle);
        let staged = recorded_restore(&h);

        h.close.update(cx, |close, cx| close.request(staged, cx));

        assert_eq!(
            settled_state(&h, &schedule, |status| status.state != ScheduleState::Idle),
            ScheduleState::Suspended
        );
    }

    #[gpui::test]
    fn withdrawing_an_armed_restore_resumes_scheduled_backups(cx: &mut TestAppContext) {
        type Withdraw = fn(&mut RestoreClose, &mut gpui::Context<RestoreClose>);
        let routes: [(&str, Withdraw); 2] = [
            ("abort", |close, cx| close.abort(cx)),
            ("yield to update", |close, cx| close.yield_to_update(cx)),
        ];
        for (label, withdraw) in routes {
            let (h, schedule) = scheduled_harness(cx);
            settled_state(&h, &schedule, idle);
            let staged = recorded_restore(&h);
            h.close.update(cx, |close, cx| close.request(staged, cx));
            settled_state(&h, &schedule, |status| {
                status.state == ScheduleState::Suspended
            });

            h.close.update(cx, withdraw);

            assert_eq!(
                settled_state(&h, &schedule, |status| {
                    status.state != ScheduleState::Suspended
                }),
                ScheduleState::Idle,
                "{label}"
            );
        }
    }
}
