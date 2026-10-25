use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{ActivePresentation, OverlayPosition, PushToast, ToastKind, overlay, tr};
use forge_storage_sqlite::SqliteDatabaseFileSchema;
use forge_transfer::{
    BUNDLE_EXTENSION, BundleSummary, Passphrase, StageRequest, StagedRestore, backups_dir,
    discard_staged_restore, read_bundle_summary, record_pending_restore, stage_full_restore,
};
use gpui::{ClickEvent, Context, Entity, Subscription, Window, div, prelude::*};
use tokio::runtime::Handle;

use crate::async_bridge::{self, DIALOG_CANCELLED, DialogFilter};
use crate::import_busy::{busy_overlay, refuse_while_busy};
use crate::restore_close::RestoreClose;
use crate::restore_confirm::{ConfirmCard, ConfirmFacts};
use crate::restore_passphrase_modal::{RestorePassphraseEvent, RestorePassphraseModal};
use crate::restore_problem::RestoreProblem;

pub(crate) const LEGACY_ARCHIVE_DIALOG_EXTENSION: &str = "gz";

struct PassphraseStep {
    bundle: PathBuf,
    modal: Entity<RestorePassphraseModal>,
    _sub: Subscription,
}

struct ConfirmStep {
    staged: StagedRestore,
    facts: ConfirmFacts,
    recording: bool,
    problem: Option<RestoreProblem>,
}

enum RestoreStep {
    Idle,
    Picking,
    Reading,
    Passphrase(PassphraseStep),
    Staging,
    Confirming(ConfirmStep),
}

pub struct RestoreFlow {
    rt_handle: Handle,
    data_dir: PathBuf,
    step: RestoreStep,
}

impl RestoreFlow {
    pub fn new(rt_handle: Handle, data_dir: PathBuf) -> Self {
        Self {
            rt_handle,
            data_dir,
            step: RestoreStep::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.step, RestoreStep::Idle)
    }

    pub fn is_preparing(&self) -> bool {
        matches!(
            self.step,
            RestoreStep::Picking | RestoreStep::Reading | RestoreStep::Staging
        )
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.step = RestoreStep::Picking;
        let filter = DialogFilter {
            name: tr!("restore_dialog_filter"),
            extensions: vec![BUNDLE_EXTENSION, LEGACY_ARCHIVE_DIALOG_EXTENSION],
        };
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file_in(Some(filter), backups_dir(&self.data_dir)),
            |this, result, cx| this.picked(result, cx),
            cx,
        );
        cx.notify();
    }

    fn picked(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        match result {
            Ok(bundle) => self.read(bundle, cx),
            Err(message) => {
                self.step = RestoreStep::Idle;
                if message != DIALOG_CANCELLED {
                    tracing::warn!(error = %message, "restore file picker failed");
                    cx.push_toast(ToastKind::Error, RestoreProblem::Failed(message).message());
                }
            }
        }
        cx.notify();
    }

    pub fn open(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            refuse_while_busy(cx);
            return;
        }
        self.read(bundle, cx);
        cx.notify();
    }

    fn read(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        self.step = RestoreStep::Reading;
        let path = bundle.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                read_bundle_summary(path)
                    .await
                    .map_err(|error| RestoreProblem::of(&error))
            },
            move |this, result, cx| this.summarized(bundle, result, cx),
            cx,
        );
    }

    fn summarized(
        &mut self,
        bundle: PathBuf,
        result: Result<BundleSummary, RestoreProblem>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(summary) if summary.needs_passphrase() => self.ask_passphrase(bundle, cx),
            Ok(_) => self.stage(bundle, None, cx),
            Err(problem) => self.refuse(problem, cx),
        }
        cx.notify();
    }

    fn ask_passphrase(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        let modal = cx.new(RestorePassphraseModal::new);
        let sub = cx.subscribe(&modal, |this, modal, event: &RestorePassphraseEvent, cx| {
            match event {
                RestorePassphraseEvent::Submitted => {
                    let passphrase = modal.update(cx, |modal, cx| modal.take_passphrase(cx));
                    if let (Some(passphrase), RestoreStep::Passphrase(step)) =
                        (passphrase, &this.step)
                    {
                        let bundle = step.bundle.clone();
                        this.stage(bundle, Some(passphrase), cx);
                    }
                }
                RestorePassphraseEvent::Cancelled => {
                    this.step = RestoreStep::Idle;
                }
            }
            cx.notify();
        });
        self.step = RestoreStep::Passphrase(PassphraseStep {
            bundle,
            modal,
            _sub: sub,
        });
    }

    fn stage(&mut self, bundle: PathBuf, passphrase: Option<Passphrase>, cx: &mut Context<Self>) {
        if passphrase.is_none() {
            self.step = RestoreStep::Staging;
        }
        let request = StageRequest {
            bundle,
            data_dir: self.data_dir.clone(),
            passphrase,
            database_schema: Arc::new(SqliteDatabaseFileSchema),
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                stage_full_restore(request)
                    .await
                    .map_err(|error| RestoreProblem::of(&error))
            },
            |this, result, cx| this.staged(result, cx),
            cx,
        );
    }

    fn staged(&mut self, result: Result<StagedRestore, RestoreProblem>, cx: &mut Context<Self>) {
        match (result, &self.step) {
            (Ok(staged), RestoreStep::Passphrase(_) | RestoreStep::Staging) => {
                let facts = ConfirmFacts::of(staged.summary());
                self.step = RestoreStep::Confirming(ConfirmStep {
                    staged,
                    facts,
                    recording: false,
                    problem: None,
                });
            }
            (Ok(staged), _) => self.discard(staged),
            (Err(problem), RestoreStep::Passphrase(step)) => {
                step.modal
                    .update(cx, |modal, cx| modal.show_problem(problem, cx));
            }
            (Err(problem), RestoreStep::Staging) => self.refuse(problem, cx),
            (Err(_), _) => {}
        }
        cx.notify();
    }

    fn refuse(&mut self, problem: RestoreProblem, cx: &mut Context<Self>) {
        self.step = RestoreStep::Idle;
        tracing::warn!(?problem, "restore refused");
        cx.push_toast(ToastKind::Error, problem.message());
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let RestoreStep::Confirming(step) = &mut self.step else {
            return;
        };
        if step.recording {
            return;
        }
        let Some(route) = RestoreClose::route(cx) else {
            step.problem = Some(RestoreProblem::Unavailable);
            cx.notify();
            return;
        };
        let activity = route.read(cx).update_activity(cx);
        step.recording = true;
        step.problem = None;
        let staged = step.staged.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                record_pending_restore(&data_dir, &staged, activity)
                    .await
                    .map_err(|error| RestoreProblem::of(&error))
            },
            move |this, result, cx| this.recorded(route, result, cx),
            cx,
        );
        cx.notify();
    }

    fn recorded(
        &mut self,
        route: Entity<RestoreClose>,
        result: Result<(), RestoreProblem>,
        cx: &mut Context<Self>,
    ) {
        let RestoreStep::Confirming(step) = &mut self.step else {
            return;
        };
        match result {
            Ok(()) => {
                let RestoreStep::Confirming(step) =
                    std::mem::replace(&mut self.step, RestoreStep::Idle)
                else {
                    return;
                };
                tracing::info!("restore staged; requesting close to apply it");
                route.update(cx, |close, cx| close.request(step.staged, cx));
            }
            Err(problem) => {
                tracing::warn!(?problem, "restore could not be scheduled");
                step.recording = false;
                step.problem = Some(problem);
            }
        }
        cx.notify();
    }

    fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        if let RestoreStep::Confirming(step) = &self.step
            && step.recording
        {
            return;
        }
        if let RestoreStep::Confirming(step) = std::mem::replace(&mut self.step, RestoreStep::Idle)
        {
            self.discard(step.staged);
        }
        cx.notify();
    }

    fn discard(&self, staged: StagedRestore) {
        self.rt_handle.spawn(async move {
            if let Err(error) = discard_staged_restore(staged).await {
                tracing::warn!(error = %error, "staged restore could not be discarded");
            }
        });
    }
}

impl Drop for RestoreFlow {
    fn drop(&mut self) {
        if let RestoreStep::Confirming(step) = std::mem::replace(&mut self.step, RestoreStep::Idle)
            && !step.recording
        {
            self.discard(step.staged);
        }
    }
}

impl Render for RestoreFlow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().absolute().top_0().left_0().size_full();
        match &self.step {
            RestoreStep::Passphrase(step) => root.child(step.modal.clone()).into_any_element(),
            RestoreStep::Confirming(step) => {
                let palette = cx.palette();
                let density = cx.density();
                let card = ConfirmCard {
                    facts: &step.facts,
                    recording: step.recording,
                    problem: step.problem.as_ref(),
                }
                .render(
                    &palette,
                    density,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_confirm(cx)),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                );
                let view = cx.entity();
                root.child(
                    overlay(card, &palette)
                        .position(OverlayPosition::Center)
                        .on_dismiss("restore-confirm-scrim", move |_window, cx| {
                            view.update(cx, |this, cx| this.cancel_confirm(cx));
                        }),
                )
                .into_any_element()
            }
            RestoreStep::Reading | RestoreStep::Staging => busy_overlay(
                "restore-busy",
                tr!("import_busy_restore_title"),
                tr!("import_busy_restore_detail"),
                &cx.palette(),
            ),
            RestoreStep::Idle | RestoreStep::Picking => div().into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_components::Toasts;
    use forge_storage::{DataProvider, Language};
    use gpui::{AppContext, Entity, TestAppContext};
    use tempfile::TempDir;

    use super::{RestoreFlow, RestoreStep};
    use crate::async_bridge::DIALOG_CANCELLED;
    use crate::i18n::install_language;
    use crate::restore_close::RestoreClose;
    use crate::restore_passphrase_modal::tests::type_passphrase;
    use crate::restore_problem::RestoreProblem;
    use crate::test_support::{
        error_toasts, install_presentation, pump, runtime, staged_restore_in, test_backend,
    };
    use crate::update_exit::ExitHandoff;
    use crate::update_flow::UpdateFlow;

    struct Harness {
        flow: Entity<RestoreFlow>,
        data: TempDir,
        rt: tokio::runtime::Runtime,
    }

    fn harness(cx: &mut TestAppContext) -> Harness {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let data = TempDir::new().unwrap();
        let handle = rt.handle().clone();
        let data_dir = data.path().to_path_buf();
        let flow = cx.new(|_| RestoreFlow::new(handle, data_dir));
        Harness { flow, data, rt }
    }

    fn finish_background_work(rt: tokio::runtime::Runtime) {
        pump(&rt);
        drop(rt);
    }

    fn step(cx: &mut TestAppContext, h: &Harness) -> &'static str {
        h.flow.read_with(cx, |flow, _| match &flow.step {
            RestoreStep::Idle => "idle",
            RestoreStep::Picking => "picking",
            RestoreStep::Reading => "reading",
            RestoreStep::Passphrase(_) => "passphrase",
            RestoreStep::Staging => "staging",
            RestoreStep::Confirming(_) => "confirming",
        })
    }

    fn confirmation(cx: &mut TestAppContext, h: &Harness) -> (bool, Option<RestoreProblem>) {
        h.flow.read_with(cx, |flow, _| match &flow.step {
            RestoreStep::Confirming(step) => (step.recording, step.problem.clone()),
            _ => panic!("the flow is not confirming"),
        })
    }

    fn confirming(cx: &mut TestAppContext, h: &Harness) -> PathBuf {
        let staged = staged_restore_in(&h.rt, h.data.path());
        let staging = staged.staging_dir().to_path_buf();
        h.flow.update(cx, |flow, cx| {
            flow.step = RestoreStep::Staging;
            flow.staged(Ok(staged), cx);
        });
        staging
    }

    fn mark_recording(cx: &mut TestAppContext, h: &Harness) {
        h.flow.update(cx, |flow, _| {
            if let RestoreStep::Confirming(step) = &mut flow.step {
                step.recording = true;
            }
        });
    }

    fn close_route(cx: &mut TestAppContext, h: &Harness) -> (Entity<RestoreClose>, ExitHandoff) {
        let exit = ExitHandoff::new(None);
        cx.update(|cx| cx.set_global(exit.clone()));
        let (backend, _) = test_backend();
        let handle = h.rt.handle().clone();
        let data_dir = h.data.path().to_path_buf();
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
        (close, exit)
    }

    #[gpui::test]
    fn a_closed_picker_is_silent_but_a_failed_picker_is_reported(cx: &mut TestAppContext) {
        for (result, expected_errors) in [(DIALOG_CANCELLED, 0), ("portal is unavailable", 1)] {
            let h = harness(cx);
            h.flow.update(cx, |flow, cx| {
                flow.step = RestoreStep::Picking;
                flow.picked(Err(result.to_owned()), cx);
            });

            assert_eq!(
                (step(cx, &h), cx.update(|cx| error_toasts(cx))),
                ("idle", expected_errors),
                "{result}"
            );
        }
    }

    #[gpui::test]
    fn a_staging_failure_returns_to_idle_with_an_error(cx: &mut TestAppContext) {
        let h = harness(cx);

        h.flow.update(cx, |flow, cx| {
            flow.step = RestoreStep::Staging;
            flow.staged(Err(RestoreProblem::Damaged), cx);
        });

        assert_eq!(
            (step(cx, &h), cx.update(|cx| error_toasts(cx))),
            ("idle", 1)
        );
    }

    #[gpui::test]
    fn a_wrong_passphrase_keeps_the_prompt_open_for_another_try(cx: &mut TestAppContext) {
        let h = harness(cx);
        h.flow.update(cx, |flow, cx| {
            flow.ask_passphrase(PathBuf::from("/backups/forge.forgebundle"), cx)
        });
        let modal = h.flow.read_with(cx, |flow, _| match &flow.step {
            RestoreStep::Passphrase(step) => step.modal.clone(),
            _ => panic!("no passphrase prompt"),
        });
        type_passphrase(cx, &modal, "correct horse battery");
        modal.update(cx, |modal, cx| modal.take_passphrase(cx));

        h.flow.update(cx, |flow, cx| {
            flow.staged(Err(RestoreProblem::WrongPassphrase), cx)
        });
        type_passphrase(cx, &modal, "second attempt here");

        assert_eq!(
            (
                step(cx, &h),
                modal.update(cx, |modal, cx| modal.can_submit(cx)),
                cx.update(|cx| error_toasts(cx)),
            ),
            ("passphrase", true, 0)
        );
    }

    #[gpui::test]
    fn cancelling_the_passphrase_prompt_returns_to_idle(cx: &mut TestAppContext) {
        let h = harness(cx);
        h.flow.update(cx, |flow, cx| {
            flow.ask_passphrase(PathBuf::from("/backups/forge.forgebundle"), cx)
        });
        let modal = h.flow.read_with(cx, |flow, _| match &flow.step {
            RestoreStep::Passphrase(step) => step.modal.clone(),
            _ => panic!("no passphrase prompt"),
        });

        modal.update(cx, |_, cx| {
            cx.emit(crate::restore_passphrase_modal::RestorePassphraseEvent::Cancelled)
        });
        cx.run_until_parked();

        assert_eq!(step(cx, &h), "idle");
    }

    #[gpui::test]
    fn a_staging_that_lands_after_the_flow_went_idle_is_discarded(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staged = staged_restore_in(&h.rt, h.data.path());
        let staging = staged.staging_dir().to_path_buf();

        h.flow.update(cx, |flow, cx| flow.staged(Ok(staged), cx));
        let idle = step(cx, &h);
        finish_background_work(h.rt);

        assert_eq!((idle, staging.exists()), ("idle", false));
    }

    #[gpui::test]
    fn cancelling_the_confirmation_discards_the_staging(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staging = confirming(cx, &h);

        h.flow.update(cx, |flow, cx| flow.cancel_confirm(cx));
        let idle = step(cx, &h);
        finish_background_work(h.rt);

        assert_eq!((idle, staging.exists()), ("idle", false));
    }

    #[gpui::test]
    fn cancelling_while_the_restore_is_being_recorded_is_ignored(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staging = confirming(cx, &h);
        mark_recording(cx, &h);

        h.flow.update(cx, |flow, cx| flow.cancel_confirm(cx));
        let still = step(cx, &h);
        finish_background_work(h.rt);

        assert_eq!((still, staging.exists()), ("confirming", true));
    }

    #[gpui::test]
    fn closing_settings_mid_confirmation_discards_the_staging(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staging = confirming(cx, &h);
        let Harness {
            flow,
            data: _data,
            rt,
        } = h;

        drop(flow);
        cx.update(|_| {});
        cx.run_until_parked();
        finish_background_work(rt);

        assert!(!staging.exists());
    }

    #[gpui::test]
    fn confirming_without_a_close_route_reports_restore_unavailable(cx: &mut TestAppContext) {
        let h = harness(cx);
        confirming(cx, &h);

        h.flow.update(cx, |flow, cx| flow.confirm(cx));

        assert_eq!(
            confirmation(cx, &h),
            (false, Some(RestoreProblem::Unavailable))
        );
    }

    #[gpui::test]
    fn a_recorded_restore_is_handed_to_the_close_route_intact(cx: &mut TestAppContext) {
        let h = harness(cx);
        let staging = confirming(cx, &h);
        mark_recording(cx, &h);
        let (close, exit) = close_route(cx, &h);

        h.flow
            .update(cx, |flow, cx| flow.recorded(close.clone(), Ok(()), cx));
        let after = (step(cx, &h), exit.restore_is_armed());
        finish_background_work(h.rt);

        assert_eq!((after, staging.exists()), (("idle", true), true));
    }

    #[gpui::test]
    fn a_failed_recording_reopens_the_confirmation_with_the_problem(cx: &mut TestAppContext) {
        let h = harness(cx);
        confirming(cx, &h);
        mark_recording(cx, &h);
        let (close, exit) = close_route(cx, &h);

        h.flow.update(cx, |flow, cx| {
            flow.recorded(
                close.clone(),
                Err(RestoreProblem::RestoreAlreadyPending),
                cx,
            )
        });

        assert_eq!(
            (confirmation(cx, &h), exit.restore_is_armed()),
            ((false, Some(RestoreProblem::RestoreAlreadyPending)), false)
        );
    }

    #[gpui::test]
    fn opening_another_backup_while_confirming_warns_and_keeps_the_confirmation(
        cx: &mut TestAppContext,
    ) {
        let h = harness(cx);
        let staging = confirming(cx, &h);

        h.flow.update(cx, |flow, cx| {
            flow.open(PathBuf::from("second.forgebundle"), cx)
        });
        let kinds: Vec<forge_components::ToastKind> = cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| toast.kind)
                .collect()
        });

        assert_eq!(
            (kinds, step(cx, &h), staging.exists()),
            (vec![forge_components::ToastKind::Warn], "confirming", true)
        );
    }
}
