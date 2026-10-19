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

pub struct RestoreClose {
    pending: Option<StagedRestore>,
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
        self.pending = Some(staged);
        if let Some(exit) = cx.try_global::<ExitHandoff>() {
            exit.arm_restore();
        }
        cx.emit(RestoreCloseEvent::CloseRequested);
        cx.notify();
    }

    pub fn abort(&mut self, cx: &mut Context<Self>) {
        let Some(staged) = self.pending.take() else {
            return;
        };
        if let Some(exit) = cx.try_global::<ExitHandoff>() {
            exit.disarm_restore();
        }
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
