use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    ActivePresentation, OverlayPosition, PushToast, ToastAction, ToastKind, overlay, tr,
};
use forge_transfer::{
    SetupImportOutcome, SetupImportRequest, StagedSetup, TransferError, apply_setup_import,
    discard_staged_setup, stage_setup,
};
use gpui::{App, ClickEvent, Context, EventEmitter, Window, div, prelude::*};

use crate::async_bridge;
use crate::pack_import_live::{LiveRefresh, SetupLiveHandles, refresh_setup_live, voice_baseline};
use crate::pack_services::PackServices;
use crate::restore_close::RestoreClose;
use crate::setup_confirm::{SetupConfirmCard, SetupCount, setup_counts};

const SETUP_TOAST_DURATION: Duration = Duration::from_secs(10);
const STICKY: Duration = Duration::ZERO;
const DISCARD_CONTEXT: &str = "staged setup discard";

pub(crate) enum SetupApplied {
    Done {
        outcome: SetupImportOutcome,
        live: LiveRefresh,
    },
    Failed {
        staged: Box<StagedSetup>,
        message: String,
    },
    RollbackIncomplete {
        message: String,
        snapshot: PathBuf,
    },
}

struct Confirming {
    counts: Vec<SetupCount>,
    staged: Option<StagedSetup>,
    failure: Option<String>,
}

impl Confirming {
    fn is_applying(&self) -> bool {
        self.staged.is_none()
    }
}

enum SetupImportStep {
    Idle,
    Staging,
    Confirming(Box<Confirming>),
}

pub struct SetupImported;

pub struct SetupImportFlow {
    services: PackServices,
    live: SetupLiveHandles,
    data_dir: PathBuf,
    step: SetupImportStep,
}

impl EventEmitter<SetupImported> for SetupImportFlow {}

pub(crate) fn needs_restart(outcome: &SetupImportOutcome, live: LiveRefresh) -> bool {
    outcome.restart_recommended || live == LiveRefresh::NeedsRestart
}

impl SetupImportFlow {
    pub fn new(services: PackServices, data_dir: PathBuf) -> Self {
        Self {
            services,
            live: SetupLiveHandles::default(),
            data_dir,
            step: SetupImportStep::Idle,
        }
    }

    pub fn set_live(&mut self, live: SetupLiveHandles) {
        self.live = live;
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.step, SetupImportStep::Idle)
    }

    pub fn open(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.step = SetupImportStep::Staging;
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                stage_setup(bundle, data_dir)
                    .await
                    .map_err(|error| error.to_string())
            },
            |this, result, cx| this.staged(result, cx),
            cx,
        );
        cx.notify();
    }

    fn staged(&mut self, result: Result<StagedSetup, String>, cx: &mut Context<Self>) {
        match result {
            Ok(staged) => {
                self.step = SetupImportStep::Confirming(Box::new(Confirming {
                    counts: setup_counts(staged.documents()),
                    staged: Some(staged),
                    failure: None,
                }));
            }
            Err(message) => {
                self.step = SetupImportStep::Idle;
                tracing::warn!(error = %message, "setup could not be staged");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("home_import_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let SetupImportStep::Confirming(confirming) = &mut self.step else {
            return;
        };
        let Some(staged) = confirming.staged.take() else {
            return;
        };
        confirming.failure = None;
        let services = self.services.clone();
        let live = self.live.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { apply_staged(services, live, data_dir, staged).await },
            |this, result, cx| this.applied(result, cx),
            cx,
        );
        cx.notify();
    }

    fn applied(&mut self, result: SetupApplied, cx: &mut Context<Self>) {
        match result {
            SetupApplied::Done { outcome, live } => {
                self.step = SetupImportStep::Idle;
                self.report_success(&outcome, live, cx);
                cx.emit(SetupImported);
            }
            SetupApplied::Failed { staged, message } => {
                tracing::warn!(error = %message, "setup import failed and was undone");
                match &mut self.step {
                    SetupImportStep::Confirming(confirming) => {
                        confirming.staged = Some(*staged);
                        confirming.failure =
                            Some(tr!("setup_import_failed", error = message.as_str()));
                    }
                    SetupImportStep::Idle | SetupImportStep::Staging => self.discard(*staged),
                }
            }
            SetupApplied::RollbackIncomplete { message, snapshot } => {
                self.step = SetupImportStep::Idle;
                report_rollback_incomplete(&message, snapshot, cx);
            }
        }
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if let SetupImportStep::Confirming(confirming) = &self.step
            && confirming.is_applying()
        {
            return;
        }
        let step = std::mem::replace(&mut self.step, SetupImportStep::Idle);
        if let SetupImportStep::Confirming(mut confirming) = step
            && let Some(staged) = confirming.staged.take()
        {
            self.discard(staged);
        }
        cx.notify();
    }

    fn discard(&self, staged: StagedSetup) {
        async_bridge::detached(
            &self.services.rt_handle,
            DISCARD_CONTEXT,
            discard_staged_setup(staged),
        );
    }

    fn report_success(
        &mut self,
        outcome: &SetupImportOutcome,
        live: LiveRefresh,
        cx: &mut Context<Self>,
    ) {
        let restart = needs_restart(outcome, live);
        tracing::info!(
            ?live,
            restart_recommended = outcome.restart_recommended,
            cleanup_failures = outcome.cleanup_failures.len(),
            "setup imported"
        );
        if restart {
            let action = ToastAction::new(tr!("setup_import_restart"), |_window, app| {
                request_restart(app)
            });
            cx.push_toast_full(
                ToastKind::Success,
                tr!("setup_import_done_restart"),
                None,
                Some(action),
                STICKY,
            );
        } else {
            let snapshot = outcome.safety_snapshot.clone();
            let action =
                ToastAction::new(tr!("pack_import_reveal_snapshot"), move |_window, app| {
                    app.reveal_path(&snapshot)
                });
            cx.push_toast_full(
                ToastKind::Success,
                tr!("setup_import_done"),
                None,
                Some(action),
                SETUP_TOAST_DURATION,
            );
        }
        if !outcome.cleanup_failures.is_empty() {
            for failure in &outcome.cleanup_failures {
                tracing::warn!(%failure, "setup import left a leftover behind");
            }
            let count = outcome.cleanup_failures.len().to_string();
            cx.push_toast(
                ToastKind::Warn,
                tr!("setup_import_cleanup_failed", count = count.as_str()),
            );
        }
    }
}

fn request_restart(app: &mut App) {
    match RestoreClose::route(app) {
        Some(close) => close.update(app, |close, cx| close.restart(cx)),
        None => tracing::warn!("restart route is missing; restart forge by hand"),
    }
}

fn report_rollback_incomplete(message: &str, snapshot: PathBuf, cx: &mut Context<SetupImportFlow>) {
    tracing::error!(error = %message, "setup import could not be fully undone");
    let shown = snapshot.display().to_string();
    let text = tr!(
        "setup_import_rollback_incomplete",
        error = message,
        snapshot = shown.as_str()
    );
    let action = ToastAction::new(tr!("pack_import_reveal_snapshot"), move |_window, app| {
        app.reveal_path(&snapshot)
    });
    cx.push_toast_full(ToastKind::Error, text, None, Some(action), STICKY);
}

impl Drop for SetupImportFlow {
    fn drop(&mut self) {
        if let SetupImportStep::Confirming(confirming) = &mut self.step
            && let Some(staged) = confirming.staged.take()
        {
            self.discard(staged);
        }
    }
}

async fn apply_staged(
    services: PackServices,
    live: SetupLiveHandles,
    data_dir: PathBuf,
    staged: StagedSetup,
) -> SetupApplied {
    let source = match services.load_source().await {
        Ok(source) => source,
        Err(message) => {
            return SetupApplied::Failed {
                staged: Box::new(staged),
                message,
            };
        }
    };
    let baseline = voice_baseline(services.backend.as_ref()).await;
    let overlay_root = services.overlays.root().await;
    let applied = apply_setup_import(SetupImportRequest {
        provider: Arc::clone(&services.backend),
        registries: services.registries(&source),
        staged: &staged,
        data_dir,
        overlay_root,
    })
    .await;
    match applied {
        Ok(outcome) => {
            discard_quietly(staged).await;
            let live = refresh_setup_live(&services, &live, &outcome, baseline.as_ref()).await;
            SetupApplied::Done { outcome, live }
        }
        Err(TransferError::SetupRollbackIncomplete {
            cause,
            failure,
            snapshot,
        }) => {
            discard_quietly(staged).await;
            SetupApplied::RollbackIncomplete {
                message: format!("{cause}; {failure}"),
                snapshot,
            }
        }
        Err(error) => SetupApplied::Failed {
            staged: Box::new(staged),
            message: error.to_string(),
        },
    }
}

async fn discard_quietly(staged: StagedSetup) {
    if let Err(error) = discard_staged_setup(staged).await {
        tracing::warn!(%error, "staged setup could not be discarded");
    }
}

impl Render for SetupImportFlow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let SetupImportStep::Confirming(confirming) = &self.step else {
            return div().into_any_element();
        };
        let palette = cx.palette();
        let density = cx.density();
        let card = SetupConfirmCard {
            counts: &confirming.counts,
            applying: confirming.is_applying(),
            failure: confirming.failure.as_deref(),
        }
        .render(
            &palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
        );
        let view = cx.entity();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                overlay(card, &palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("setup-confirm-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel(cx));
                    }),
            )
            .into_any_element()
    }
}
