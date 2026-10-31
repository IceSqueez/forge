use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{ActivePresentation, PushToast, ToastAction, ToastKind, tr};
use forge_transfer::{
    PackImportOutcome, PackImportPlan, PackImportRequest, StagedPack, TransferError,
    apply_pack_import, backups_dir, discard_staged_pack, plan_pack_import, stage_pack,
};
use gpui::{Context, Entity, EventEmitter, Subscription, Window, div, prelude::*};

use crate::async_bridge;
use crate::import_busy::{busy_overlay, refuse_while_busy};
use crate::pack_import_live::{LiveRefresh, LiveTargets, refresh_live};
use crate::pack_import_modal::{PackHeader, PackImportModal, PackImportModalEvent, carry_choices};
use crate::pack_services::PackServices;
use crate::repo_failure::RepoFailure;
use crate::restore_problem::RestoreProblem;

const PACK_TOAST_DURATION: Duration = Duration::from_secs(10);
const STICKY: Duration = Duration::ZERO;
const DISCARD_CONTEXT: &str = "staged pack discard";

pub(crate) struct Planned {
    staged: StagedPack,
    plan: PackImportPlan,
}

enum PlanFailure {
    Refused(RestoreProblem),
    SourceUnreadable(RepoFailure),
}

impl PlanFailure {
    fn into_failure(self) -> RepoFailure {
        match self {
            Self::Refused(problem) => RepoFailure::localized(problem.message()),
            Self::SourceUnreadable(failure) => failure,
        }
    }
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
        message: RepoFailure,
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
            refuse_while_busy(cx);
            return;
        }
        self.step = PackImportStep::Planning;
        let services = self.services.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { stage_and_plan(services, bundle, data_dir).await },
            |this, result, cx| this.planned(result.map_err(PlanFailure::into_failure), cx),
            cx,
        );
        cx.notify();
    }

    fn planned(&mut self, result: Result<Planned, RepoFailure>, cx: &mut Context<Self>) {
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
                    tr!("home_import_failed", error = message.text()),
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
                    let shown = tr!("pack_import_failed", error = message.text());
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
) -> Result<Planned, PlanFailure> {
    let mut staged = stage_pack(bundle, data_dir)
        .await
        .map_err(|error| PlanFailure::Refused(RestoreProblem::of(&error)))?;
    match services.load_source().await {
        Ok(source) => {
            let registries = services.registries(&source);
            staged.upgrade_retired_ids(registries);
            match plan_pack_import(&staged, &source.inventory, registries) {
                Ok(plan) => Ok(Planned { staged, plan }),
                Err(error) => {
                    discard_quietly(staged).await;
                    Err(PlanFailure::Refused(RestoreProblem::of(&error)))
                }
            }
        }
        Err(message) => {
            discard_quietly(staged).await;
            Err(PlanFailure::SourceUnreadable(message))
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
        Err(message) => {
            return Applied::Failed { staged, message };
        }
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
        Err(TransferError::PackRollbackIncomplete { cause, failure, .. }) => {
            discard_quietly(staged).await;
            Applied::RollbackIncomplete {
                message: format!("{cause}; {failure}"),
                snapshot_taken,
            }
        }
        Err(error) => {
            tracing::warn!(%error, "pack import failed");
            Applied::Failed {
                staged,
                message: RepoFailure::from(error),
            }
        }
    }
}

async fn replan(services: &PackServices, staged: StagedPack, previous: &PackImportPlan) -> Applied {
    match services.load_source().await {
        Ok(source) => {
            match plan_pack_import(&staged, &source.inventory, services.registries(&source)) {
                Ok(mut plan) => {
                    carry_choices(previous, &mut plan);
                    Applied::Stale { staged, plan }
                }
                Err(error) => {
                    tracing::warn!(%error, "pack replan failed");
                    Applied::Failed {
                        staged,
                        message: RepoFailure::from(error),
                    }
                }
            }
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        match &self.step {
            PackImportStep::Previewing(preview) => div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(preview.modal.clone())
                .into_any_element(),
            PackImportStep::Planning => busy_overlay(
                "pack-import-busy",
                tr!("import_busy_pack_title"),
                tr!("import_busy_pack_detail"),
                &cx.palette(),
                density,
            ),
            PackImportStep::Idle => div().into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use forge_components::{ToastKind, Toasts};
    use forge_overlay::OverlayError;
    use forge_storage::Language;
    use forge_storage::OverlayConfig;
    use forge_transfer::{
        PackImportOutcome, PackImportPlan, PackItemKey, PackItemKind, PackResolution, StagedPack,
        TransferError, pack_staging_root,
    };
    use forge_types::Variant;
    use gpui::{AppContext, Entity, TestAppContext};

    use super::{
        Applied, PACK_TOAST_DURATION, PackImportFlow, PackImportStep, PackImported, Planned,
        apply_staged, discard_staged_pack,
    };
    use crate::i18n::install_language;
    use crate::pack_import_live::LiveRefresh;
    use crate::restore_problem::RestoreProblem;
    use crate::test_support::pack_flow::{
        PACKED_OVERLAY_NAME, PackWorld, export_pack, export_pack_carrying, pack_world, queue_named,
        seed_overlay, seed_queued_action, stage_into,
    };
    use crate::test_support::{
        alive_tasks, install_presentation, pump_until, runtime, settle_until,
        wait_until_worker_drains_to, worker_runtime,
    };
    use forge_components::tr;

    const PAST_ANY_TOAST_TIMER: Duration = Duration::from_secs(120);

    struct Rig {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        flow: Entity<PackImportFlow>,
    }

    fn rig(cx: &mut TestAppContext) -> Rig {
        rig_on(cx, runtime())
    }

    fn rig_on(cx: &mut TestAppContext, rt: tokio::runtime::Runtime) -> Rig {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let world = pack_world(&rt);
        let services = world.services.clone();
        let data_dir = world.data_dir();
        let flow = cx.new(|_| PackImportFlow::new(services, data_dir));
        Rig { rt, world, flow }
    }

    fn staged_reimport(rig: &Rig) -> (StagedPack, PackImportPlan) {
        let queue = queue_named("Alerts");
        let action = seed_queued_action(&rig.rt, &rig.world, "Greeter", &queue);
        let bundle = export_pack(&rig.rt, &rig.world, &[action]);
        stage_into(&rig.rt, &rig.world, &bundle)
    }

    fn previewing(cx: &mut TestAppContext, rig: &Rig) -> PathBuf {
        let (staged, plan) = staged_reimport(rig);
        let staging = staged.staging_dir().to_path_buf();
        rig.flow.update(cx, |flow, cx| {
            flow.planned(Ok(Planned { staged, plan }), cx)
        });
        staging
    }

    fn take_staged_as_apply_does(cx: &mut TestAppContext, rig: &Rig) -> StagedPack {
        rig.flow.update(cx, |flow, cx| {
            let PackImportStep::Previewing(preview) = &mut flow.step else {
                panic!("the flow is not previewing");
            };
            preview.modal.update(cx, |modal, cx| modal.begin_apply(cx));
            preview
                .staged
                .take()
                .expect("the preview holds the staged pack")
        })
    }

    fn step(cx: &mut TestAppContext, rig: &Rig) -> &'static str {
        rig.flow.read_with(cx, |flow, _| match &flow.step {
            PackImportStep::Idle => "idle",
            PackImportStep::Planning => "planning",
            PackImportStep::Previewing(preview) if preview.staged.is_some() => "previewing",
            PackImportStep::Previewing(_) => "previewing-without-staging",
        })
    }

    fn modal_state(cx: &mut TestAppContext, rig: &Rig) -> (bool, Option<String>, PackImportPlan) {
        rig.flow.read_with(cx, |flow, cx| {
            let PackImportStep::Previewing(preview) = &flow.step else {
                panic!("the flow is not previewing");
            };
            let modal = preview.modal.read(cx);
            (
                modal.is_applying(),
                modal.shown_failure(),
                modal.plan().clone(),
            )
        })
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String, bool)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.kind,
                        toast.message.to_string(),
                        toast.action.is_some(),
                    )
                })
                .collect()
        })
    }

    fn outcome() -> PackImportOutcome {
        PackImportOutcome {
            safety_snapshot: None,
            written: Vec::new(),
            skipped: Vec::new(),
        }
    }

    #[gpui::test]
    fn a_planning_failure_returns_to_idle_with_an_error_toast(cx: &mut TestAppContext) {
        let rig = rig(cx);
        rig.flow.update(cx, |flow, cx| {
            flow.step = PackImportStep::Planning;
            flow.planned(
                Err(crate::repo_failure::RepoFailure::operation("not a pack")),
                cx,
            );
        });

        let kinds: Vec<ToastKind> = toasts(cx).into_iter().map(|toast| toast.0).collect();
        assert_eq!((step(cx, &rig), kinds), ("idle", vec![ToastKind::Error]));
    }

    #[gpui::test]
    fn cancelling_the_preview_discards_the_staged_pack(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = previewing(cx, &rig);

        rig.flow.update(cx, |flow, cx| flow.cancel(cx));
        settle_until(cx, &rig.rt, |_| !staging.exists());

        assert_eq!(step(cx, &rig), "idle");
    }

    #[gpui::test]
    fn dropping_the_flow_mid_preview_discards_the_staged_pack(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = previewing(cx, &rig);
        let Rig { rt, world, flow } = rig;

        cx.update(move |_| drop(flow));
        pump_until(&rt, || !staging.exists());
        drop(world);
    }

    #[gpui::test]
    fn confirming_the_preview_applies_the_pack_and_discards_staging(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = previewing(cx, &rig);
        let imported = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = std::rc::Rc::clone(&imported);
        cx.update(|cx| {
            cx.subscribe(&rig.flow, move |_, _: &PackImported, _| seen.set(true))
                .detach();
        });

        rig.flow.update(cx, |flow, cx| flow.apply(cx));
        settle_until(cx, &rig.rt, |cx| {
            step(cx, &rig) == "idle" && !staging.exists()
        });

        let kinds: Vec<ToastKind> = toasts(cx).into_iter().map(|toast| toast.0).collect();
        assert_eq!((kinds, imported.get()), (vec![ToastKind::Success], true));
    }

    #[gpui::test]
    fn a_failed_apply_shows_the_error_in_the_modal_and_keeps_staging(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = previewing(cx, &rig);
        let staged = take_staged_as_apply_does(cx, &rig);

        rig.flow.update(cx, |flow, cx| {
            flow.applied(
                Applied::Failed {
                    staged,
                    message: crate::repo_failure::RepoFailure::localized("disk full"),
                },
                cx,
            )
        });

        let (applying, failure, _) = modal_state(cx, &rig);
        assert_eq!(
            (
                step(cx, &rig),
                applying,
                failure.is_some_and(|shown| shown.contains("disk full")),
                staging.exists()
            ),
            ("previewing", false, true, true)
        );
    }

    #[gpui::test]
    fn after_a_failed_apply_confirming_again_starts_a_fresh_apply(cx: &mut TestAppContext) {
        let rig = rig(cx);
        previewing(cx, &rig);
        let staged = take_staged_as_apply_does(cx, &rig);
        rig.flow.update(cx, |flow, cx| {
            flow.applied(
                Applied::Failed {
                    staged,
                    message: crate::repo_failure::RepoFailure::localized("disk full"),
                },
                cx,
            )
        });

        rig.flow.update(cx, |flow, cx| flow.apply(cx));

        let (applying, _, _) = modal_state(cx, &rig);
        assert_eq!(
            (step(cx, &rig), applying),
            ("previewing-without-staging", true)
        );
    }

    #[gpui::test]
    fn a_stale_plan_is_swapped_into_the_modal_with_a_warning_and_staging_kept(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let staging = previewing(cx, &rig);
        let staged = take_staged_as_apply_does(cx, &rig);
        let (_, _, mut replanned) = modal_state(cx, &rig);
        let first = replanned.items()[0].key.clone();
        replanned
            .set_resolution(&first, PackResolution::Replace)
            .unwrap();

        rig.flow.update(cx, |flow, cx| {
            flow.applied(
                Applied::Stale {
                    staged,
                    plan: replanned.clone(),
                },
                cx,
            )
        });

        let (applying, _, shown) = modal_state(cx, &rig);
        let kinds: Vec<ToastKind> = toasts(cx).into_iter().map(|toast| toast.0).collect();
        assert_eq!(
            (
                step(cx, &rig),
                applying,
                shown == replanned,
                kinds,
                staging.exists()
            ),
            ("previewing", false, true, vec![ToastKind::Warn], true)
        );
    }

    #[test]
    fn a_stale_apply_replans_and_keeps_the_choices_whose_items_still_conflict() {
        let rt = runtime();
        let source = pack_world(&rt);
        let queue = queue_named("Alerts");
        let action = seed_queued_action(&rt, &source, "Greeter", &queue);
        let bundle = export_pack(&rt, &source, &[action]);
        let target = pack_world(&rt);
        rt.block_on(target.provider().queue_repo().save(&queue_named("Alerts")))
            .unwrap();
        let (staged, mut plan) = stage_into(&rt, &target, &bundle);
        let queue_key = PackItemKey {
            kind: PackItemKind::Queue,
            id: queue.id.to_string(),
        };
        plan.set_resolution(&queue_key, PackResolution::Replace)
            .unwrap();
        seed_queued_action(&rt, &target, "Greeter", &queue_named("Elsewhere"));

        let applied = rt.block_on(apply_staged(
            target.services.clone(),
            target.data_dir(),
            staged,
            plan,
        ));

        let Applied::Stale { plan, .. } = applied else {
            panic!("the changed store did not make the plan stale");
        };
        let carried = plan
            .items()
            .iter()
            .find(|item| item.key == queue_key)
            .and_then(|item| item.resolution);
        assert_eq!(carried, Some(PackResolution::Replace));
    }

    #[gpui::test]
    fn a_success_toast_stays_until_dismissed_only_when_a_restart_is_needed(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let mut survived = Vec::new();

        for live in [LiveRefresh::Applied, LiveRefresh::NeedsRestart] {
            cx.update(|cx| cx.set_global(Toasts::new()));
            rig.flow.update(cx, |flow, cx| {
                flow.applied(
                    Applied::Done {
                        outcome: outcome(),
                        live,
                    },
                    cx,
                )
            });
            cx.executor()
                .advance_clock(PACK_TOAST_DURATION + PAST_ANY_TOAST_TIMER);
            cx.run_until_parked();
            survived.push((live, toasts(cx).len()));
        }

        assert_eq!(
            survived,
            vec![(LiveRefresh::Applied, 0), (LiveRefresh::NeedsRestart, 1)]
        );
    }

    #[gpui::test]
    fn an_incomplete_rollback_leaves_a_sticky_error_offering_the_snapshot_only_if_taken(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let mut shown = Vec::new();

        for snapshot_taken in [true, false] {
            cx.update(|cx| cx.set_global(Toasts::new()));
            previewing(cx, &rig);
            let staged = take_staged_as_apply_does(cx, &rig);
            rig.rt.block_on(discard_staged_pack(staged)).unwrap();
            rig.flow.update(cx, |flow, cx| {
                flow.applied(
                    Applied::RollbackIncomplete {
                        message: "rollback failed".to_owned(),
                        snapshot_taken,
                    },
                    cx,
                )
            });
            cx.executor().advance_clock(PAST_ANY_TOAST_TIMER);
            cx.run_until_parked();
            let summary: Vec<(ToastKind, bool)> = toasts(cx)
                .into_iter()
                .map(|(kind, _, action)| (kind, action))
                .collect();
            shown.push((step(cx, &rig), summary));
        }

        assert_eq!(
            shown,
            vec![
                ("idle", vec![(ToastKind::Error, true)]),
                ("idle", vec![(ToastKind::Error, false)]),
            ]
        );
    }

    #[gpui::test]
    fn opening_another_pack_while_previewing_warns_and_keeps_the_preview(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = previewing(cx, &rig);

        rig.flow.update(cx, |flow, cx| {
            flow.open(PathBuf::from("second.forgebundle"), cx)
        });

        let kinds: Vec<ToastKind> = toasts(cx).into_iter().map(|toast| toast.0).collect();
        assert_eq!(
            (kinds, step(cx, &rig), staging.exists()),
            (vec![ToastKind::Warn], "previewing", true)
        );
    }

    #[gpui::test]
    fn an_unreadable_pack_staged_on_a_worker_thread_toasts_the_refusal_in_the_ui_language(
        cx: &mut TestAppContext,
    ) {
        let rig = rig_on(cx, worker_runtime());
        install_language(Language::Uk);
        let junk = rig.world.data_dir().join("junk.forgebundle");
        std::fs::write(&junk, b"not a bundle").unwrap();
        let background_tasks = alive_tasks(&rig.rt);

        rig.flow.update(cx, |flow, cx| flow.open(junk, cx));
        wait_until_worker_drains_to(&rig.rt, background_tasks);
        cx.run_until_parked();

        let messages: Vec<String> = toasts(cx).into_iter().map(|toast| toast.1).collect();
        assert_eq!(
            (step(cx, &rig), messages),
            (
                "idle",
                vec![tr!(
                    "home_import_failed",
                    error = RestoreProblem::NotABundle.message()
                )]
            )
        );
    }

    #[gpui::test]
    fn a_pack_carrying_overlay_settings_its_look_rejects_is_refused_at_the_plan_step(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let queue = queue_named("Alerts");
        let action = seed_queued_action(&rig.rt, &rig.world, "Greeter", &queue);
        let overlay = seed_overlay(
            &rig.rt,
            &rig.world,
            "overlay.chat",
            OverlayConfig::from([("position".to_owned(), Variant::String("middle".to_owned()))]),
        );
        let bundle = export_pack_carrying(&rig.rt, &rig.world, &[action], &[overlay.id]);

        rig.flow.update(cx, |flow, cx| flow.open(bundle, cx));
        settle_until(cx, &rig.rt, |cx| step(cx, &rig) == "idle");

        let refusal = TransferError::InvalidOverlayConfig {
            overlay: PACKED_OVERLAY_NAME.to_owned(),
            source: OverlayError::UnknownChoice {
                key: "position".to_owned(),
                value: "middle".to_owned(),
            },
        };
        let messages: Vec<String> = toasts(cx).into_iter().map(|toast| toast.1).collect();
        let staged_left = std::fs::read_dir(pack_staging_root(&rig.world.data_dir()))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(
            (messages, staged_left),
            (
                vec![tr!(
                    "home_import_failed",
                    error = RestoreProblem::of(&refusal).message()
                )],
                0
            )
        );
    }
}
