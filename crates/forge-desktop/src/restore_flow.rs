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
use crate::restore_close::RestoreClose;
use crate::restore_confirm::{ConfirmCard, ConfirmFacts};
use crate::restore_passphrase_modal::{RestorePassphraseEvent, RestorePassphraseModal};
use crate::restore_problem::RestoreProblem;

const LEGACY_ARCHIVE_DIALOG_EXTENSION: &str = "gz";

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
            Ok(bundle) => {
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
            RestoreStep::Idle
            | RestoreStep::Picking
            | RestoreStep::Reading
            | RestoreStep::Staging => div().into_any_element(),
        }
    }
}
