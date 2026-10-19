use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use forge_transfer::{
    PackImportOutcome, PackImportPlan, PackImportRequest, StagedPack, TransferError,
    apply_pack_import, backups_dir, discard_staged_pack, plan_pack_import, stage_pack,
};
use gpui::{Context, Entity, EventEmitter, Subscription, Window, div, prelude::*};

use crate::async_bridge;
use crate::pack_import_live::{LiveRefresh, LiveTargets, refresh_live};
use crate::pack_import_modal::{PackHeader, PackImportModal, PackImportModalEvent, carry_choices};
use crate::pack_services::PackServices;

const PACK_TOAST_DURATION: Duration = Duration::from_secs(10);
const STICKY: Duration = Duration::ZERO;
const DISCARD_CONTEXT: &str = "staged pack discard";

pub(crate) struct Planned {
    staged: StagedPack,
    plan: PackImportPlan,
}

pub(crate) enum Applied {
    Done {
        outcome: PackImportOutcome,
        live: LiveRefresh,
    },
    Stale {
        staged: StagedPack,
        plan: PackImportPlan,
    },
    Failed {
        staged: StagedPack,
        message: String,
    },
    RollbackIncomplete {
        message: String,
        snapshot_taken: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportCounts {
    pub written: usize,
    pub skipped: usize,
}

impl ImportCounts {
    pub(crate) fn of(outcome: &PackImportOutcome) -> Self {
        Self {
            written: outcome.written.len(),
            skipped: outcome.skipped.len(),
        }
    }
}

pub(crate) fn success_message(counts: &ImportCounts, live: LiveRefresh) -> String {
    let written = counts.written.to_string();
    let skipped = counts.skipped.to_string();
    match live {
        LiveRefresh::Applied => tr!(
            "pack_import_done",
            written = written.as_str(),
            skipped = skipped.as_str()
        ),
        LiveRefresh::NeedsRestart => tr!(
            "pack_import_done_restart",
            written = written.as_str(),
            skipped = skipped.as_str()
        ),
    }
}

struct Preview {
    modal: Entity<PackImportModal>,
    staged: Option<StagedPack>,
    _sub: Subscription,
}

enum PackImportStep {
    Idle,
    Planning,
    Previewing(Box<Preview>),
}

pub struct PackImported;

pub struct PackImportFlow {
    services: PackServices,
    data_dir: PathBuf,
    step: PackImportStep,
}

impl EventEmitter<PackImported> for PackImportFlow {}

impl PackImportFlow {
    pub fn new(services: PackServices, data_dir: PathBuf) -> Self {
        Self {
            services,
            data_dir,
            step: PackImportStep::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.step, PackImportStep::Idle)
    }

    pub fn open(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.step = PackImportStep::Planning;
        let services = self.services.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { stage_and_plan(services, bundle, data_dir).await },
            |this, result, cx| this.planned(result, cx),
            cx,
        );
        cx.notify();
    }

    fn planned(&mut self, result: Result<Planned, String>, cx: &mut Context<Self>) {
        match result {
            Ok(Planned { staged, plan }) => {
                let header = PackHeader::of(staged.manifest());
                let modal = cx.new(|_| PackImportModal::new(header, plan));
                let sub =
                    cx.subscribe(
                        &modal,
                        |this, _, event: &PackImportModalEvent, cx| match event {
                            PackImportModalEvent::Confirmed => this.apply(cx),
                            PackImportModalEvent::Cancelled => this.cancel(cx),
                        },
                    );
                self.step = PackImportStep::Previewing(Box::new(Preview {
                    modal,
                    staged: Some(staged),
                    _sub: sub,
                }));
            }
            Err(message) => {
                self.step = PackImportStep::Idle;
                tracing::warn!(error = %message, "pack import planning failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("home_import_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }

    fn apply(&mut self, cx: &mut Context<Self>) {
        let PackImportStep::Previewing(preview) = &mut self.step else {
            return;
        };
        if preview.modal.read(cx).is_applying() {
            return;
        }
        let Some(staged) = preview.staged.take() else {
            return;
        };
        let plan = preview.modal.read(cx).plan().clone();
        preview.modal.update(cx, |modal, cx| modal.begin_apply(cx));
        let services = self.services.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { apply_staged(services, data_dir, staged, plan).await },
            |this, result, cx| this.applied(result, cx),
            cx,
        );
        cx.notify();
    }

    fn applied(&mut self, result: Applied, cx: &mut Context<Self>) {
        match result {
            Applied::Done { outcome, live } => {
                self.step = PackImportStep::Idle;
                self.report_success(outcome, live, cx);
                cx.emit(PackImported);
            }
            Applied::Stale { staged, plan } => {
                if let Some(modal) = self.restore_staged(staged) {
                    modal.update(cx, |modal, cx| modal.replan(plan, cx));
                    cx.push_toast(ToastKind::Warn, tr!("pack_import_stale"));
                }
            }
            Applied::Failed { staged, message } => {
                tracing::warn!(error = %message, "pack import failed and was undone");
                if let Some(modal) = self.restore_staged(staged) {
                    let shown = tr!("pack_import_failed", error = message.as_str());
                    modal.update(cx, |modal, cx| modal.fail(shown, cx));
                }
            }
            Applied::RollbackIncomplete {
                message,
                snapshot_taken,
            } => {
                self.step = PackImportStep::Idle;
                self.report_rollback_incomplete(&message, snapshot_taken, cx);
            }
        }
        cx.notify();
    }

    fn restore_staged(&mut self, staged: StagedPack) -> Option<Entity<PackImportModal>> {
        match &mut self.step {
            PackImportStep::Previewing(preview) => {
                preview.staged = Some(staged);
                Some(preview.modal.clone())
            }
            PackImportStep::Idle | PackImportStep::Planning => {
                self.discard(staged);
                None
            }
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        let step = std::mem::replace(&mut self.step, PackImportStep::Idle);
        if let PackImportStep::Previewing(mut preview) = step
            && let Some(staged) = preview.staged.take()
        {
            self.discard(staged);
        }
        cx.notify();
    }

    fn discard(&self, staged: StagedPack) {
        async_bridge::detached(
            &self.services.rt_handle,
            DISCARD_CONTEXT,
            discard_staged_pack(staged),
        );
    }

    fn report_success(
        &mut self,
        outcome: PackImportOutcome,
        live: LiveRefresh,
        cx: &mut Context<Self>,
    ) {
        let counts = ImportCounts::of(&outcome);
        tracing::info!(
            written = counts.written,
            skipped = counts.skipped,
            ?live,
            "pack imported"
        );
        let message = success_message(&counts, live);
        let duration = match live {
            LiveRefresh::Applied => PACK_TOAST_DURATION,
            LiveRefresh::NeedsRestart => STICKY,
        };
        let action = outcome.safety_snapshot.map(|snapshot| {
            ToastAction::new(tr!("pack_import_reveal_snapshot"), move |_window, app| {
                app.reveal_path(&snapshot)
            })
        });
        cx.push_toast_full(ToastKind::Success, message, None, action, duration);
    }

    fn report_rollback_incomplete(
        &mut self,
        message: &str,
        snapshot_taken: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::error!(error = %message, "pack import could not be fully undone");
        let folder = backups_dir(&self.data_dir);
        let shown = folder.display().to_string();
        let text = if snapshot_taken {
            tr!(
                "pack_import_rollback_incomplete",
                error = message,
                folder = shown.as_str()
            )
        } else {
            tr!(
                "pack_import_rollback_incomplete_no_snapshot",
                error = message
            )
        };
        let action = snapshot_taken.then(|| {
            ToastAction::new(tr!("pack_import_reveal_snapshot"), move |_window, app| {
                app.reveal_path(&folder)
            })
        });
        cx.push_toast_full(ToastKind::Error, text, None, action, STICKY);
    }
}

impl Drop for PackImportFlow {
    fn drop(&mut self) {
        if let PackImportStep::Previewing(preview) = &mut self.step
            && let Some(staged) = preview.staged.take()
        {
            self.discard(staged);
        }
    }
}

async fn stage_and_plan(
    services: PackServices,
    bundle: PathBuf,
    data_dir: PathBuf,
) -> Result<Planned, String> {
    let staged = stage_pack(bundle, data_dir)
        .await
        .map_err(|error| error.to_string())?;
    match services.load_source().await {
        Ok(source) => {
            let plan = plan_pack_import(&staged, &source.inventory, services.registries(&source));
            Ok(Planned { staged, plan })
        }
        Err(message) => {
            discard_quietly(staged).await;
            Err(message)
        }
    }
}

async fn apply_staged(
    services: PackServices,
    data_dir: PathBuf,
    staged: StagedPack,
    plan: PackImportPlan,
) -> Applied {
    let source = match services.load_source().await {
        Ok(source) => source,
        Err(message) => return Applied::Failed { staged, message },
    };
    let snapshot_taken = plan.replaces_anything();
    let overlay_root = services.overlays.root().await;
    let applied = apply_pack_import(PackImportRequest {
        provider: Arc::clone(&services.backend),
        registries: services.registries(&source),
        staged: &staged,
        plan: plan.clone(),
        data_dir,
        overlay_root,
    })
    .await;
    match applied {
        Ok(outcome) => {
            discard_quietly(staged).await;
            let live = refresh_live(&services, LiveTargets::of(&outcome.written)).await;
            Applied::Done { outcome, live }
        }
        Err(TransferError::PackPlanStale) => replan(&services, staged, &plan).await,
        Err(error @ TransferError::PackRollbackIncomplete { .. }) => {
            discard_quietly(staged).await;
            Applied::RollbackIncomplete {
                message: error.to_string(),
                snapshot_taken,
            }
        }
        Err(error) => Applied::Failed {
            staged,
            message: error.to_string(),
        },
    }
}

async fn replan(services: &PackServices, staged: StagedPack, previous: &PackImportPlan) -> Applied {
    match services.load_source().await {
        Ok(source) => {
            let mut plan =
                plan_pack_import(&staged, &source.inventory, services.registries(&source));
            carry_choices(previous, &mut plan);
            Applied::Stale { staged, plan }
        }
        Err(message) => Applied::Failed { staged, message },
    }
}

async fn discard_quietly(staged: StagedPack) {
    if let Err(error) = discard_staged_pack(staged).await {
        tracing::warn!(%error, "staged pack could not be discarded");
    }
}

impl Render for PackImportFlow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.step {
            PackImportStep::Previewing(preview) => div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(preview.modal.clone())
                .into_any_element(),
            PackImportStep::Idle | PackImportStep::Planning => div().into_any_element(),
        }
    }
}
