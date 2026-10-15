use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_update::{
    ApplyMode, Artifact, CancelSignal, DownloadProgress, StagedArtifact, UpdatableInstall,
    UpdateClient, UpdateError, download_artifact, progress_channel,
};
use gpui::{App, Context, EventEmitter, WeakEntity};
use tokio::runtime::Handle;

use crate::async_bridge::{detached, open_path, run_async};
use crate::toasts::Toasts;
use crate::update_check::remember_skipped;
use crate::update_exit::ExitHandoff;
use crate::update_handoff::{HandoffInput, prepare_handoff};
use crate::update_toast::{UpdateCommand, toast_data, update_toast};

const PERCENT_SCALE: u128 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTarget {
    pub install: UpdatableInstall,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub version: String,
    pub release_page: String,
    pub install: Option<InstallTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFailure {
    Download,
    Unverified,
    Handoff,
}

#[derive(Debug, Clone)]
pub enum UpdateStage {
    Idle,
    Offered(UpdateCandidate),
    Downloading {
        candidate: UpdateCandidate,
        progress: DownloadProgress,
        cancel: CancelSignal,
    },
    Ready {
        candidate: UpdateCandidate,
        staged: StagedArtifact,
    },
    Failed {
        candidate: UpdateCandidate,
        failure: UpdateFailure,
        staged: Option<StagedArtifact>,
    },
    HandingOff {
        candidate: UpdateCandidate,
        staged: StagedArtifact,
        mode: ApplyMode,
    },
    HandedOff {
        candidate: UpdateCandidate,
        mode: ApplyMode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFlowEvent {
    RestartRequested,
    CloseApproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDecision {
    Proceed,
    Hold,
}

pub fn download_fraction(progress: DownloadProgress) -> f32 {
    if progress.total == 0 {
        return 0.0;
    }
    (progress.received as f64 / progress.total as f64).clamp(0.0, 1.0) as f32
}

pub fn download_percent(progress: DownloadProgress) -> u8 {
    if progress.total == 0 {
        return 0;
    }
    let received = u128::from(progress.received.min(progress.total));
    u8::try_from(received * PERCENT_SCALE / u128::from(progress.total)).unwrap_or(u8::MAX)
}

pub fn failure_of(error: &UpdateError) -> UpdateFailure {
    match error {
        UpdateError::ArtifactHashMismatch
        | UpdateError::ArtifactSizeMismatch { .. }
        | UpdateError::ArtifactOversized { .. }
        | UpdateError::InvalidArtifactName => UpdateFailure::Unverified,
        _ => UpdateFailure::Download,
    }
}

pub fn apply_mode(restart_requested: bool) -> ApplyMode {
    if restart_requested {
        ApplyMode::Restart
    } else {
        ApplyMode::OnQuit
    }
}

pub struct UpdateFlow {
    stage: UpdateStage,
    restart_requested: bool,
    ticket: u64,
    toast: Option<u64>,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    data_dir: PathBuf,
}

impl EventEmitter<UpdateFlowEvent> for UpdateFlow {}

impl UpdateFlow {
    pub fn new(backend: Arc<dyn DataProvider>, rt_handle: Handle, data_dir: PathBuf) -> Self {
        Self {
            stage: UpdateStage::Idle,
            restart_requested: false,
            ticket: 0,
            toast: None,
            backend,
            rt_handle,
            data_dir,
        }
    }

    pub fn offer(&mut self, candidate: UpdateCandidate, cx: &mut Context<Self>) {
        if !matches!(self.stage, UpdateStage::Idle | UpdateStage::Offered(_)) {
            return;
        }
        self.stage = UpdateStage::Offered(candidate);
        self.show_toast(cx);
        cx.notify();
    }

    pub fn run(&mut self, command: UpdateCommand, cx: &mut Context<Self>) {
        match command {
            UpdateCommand::StartDownload => self.start_download(cx),
            UpdateCommand::OpenReleasePage => self.open_release_page(cx),
            UpdateCommand::Skip => self.skip(cx),
            UpdateCommand::Cancel => self.cancel_download(cx),
            UpdateCommand::Restart => self.request_restart(cx),
            UpdateCommand::Retry => self.retry(cx),
        }
    }

    pub fn start_download(&mut self, cx: &mut Context<Self>) {
        let UpdateStage::Offered(candidate) = &self.stage else {
            return;
        };
        let candidate = candidate.clone();
        let Some(target) = candidate.install.clone() else {
            return;
        };
        let Some(staging_dir) = target.install.staging_dir().map(PathBuf::from) else {
            self.fail(candidate, UpdateFailure::Download, None, cx);
            return;
        };

        let (progress_tx, mut progress_rx) = progress_channel();
        let cancel = CancelSignal::new();
        self.ticket = self.ticket.wrapping_add(1);
        let ticket = self.ticket;
        self.stage = UpdateStage::Downloading {
            candidate,
            progress: progress_rx.latest(),
            cancel: cancel.clone(),
        };

        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let artifact = target.artifact;
        self.rt_handle.spawn(async move {
            let result = match UpdateClient::for_github() {
                Ok(client) => {
                    download_artifact(&client, &artifact, &staging_dir, &progress_tx, &cancel).await
                }
                Err(error) => Err(error),
            };
            drop(progress_tx);
            let _ = done_tx.send(result);
        });

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Some(progress) = progress_rx.changed().await {
                if this
                    .update(cx, |flow, cx| flow.apply_progress(ticket, progress, cx))
                    .is_err()
                {
                    return;
                }
            }
            let _ = match done_rx.await {
                Ok(result) => this.update(cx, |flow, cx| flow.finish_download(ticket, result, cx)),
                Err(_) => this.update(cx, |flow, cx| flow.download_task_lost(ticket, cx)),
            };
        })
        .detach();

        self.show_toast(cx);
        cx.notify();
    }

    pub fn apply_progress(
        &mut self,
        ticket: u64,
        progress: DownloadProgress,
        cx: &mut Context<Self>,
    ) {
        if ticket != self.ticket {
            return;
        }
        let UpdateStage::Downloading {
            progress: current, ..
        } = &mut self.stage
        else {
            return;
        };
        let repaint = download_percent(*current) != download_percent(progress);
        *current = progress;
        if repaint {
            self.refresh_toast(cx);
            cx.notify();
        }
    }

    pub fn finish_download(
        &mut self,
        ticket: u64,
        result: Result<StagedArtifact, UpdateError>,
        cx: &mut Context<Self>,
    ) {
        if ticket != self.ticket {
            return;
        }
        let Some(UpdateStage::Downloading { candidate, .. }) =
            self.take_stage(|stage| matches!(stage, UpdateStage::Downloading { .. }))
        else {
            return;
        };
        match result {
            Ok(staged) => {
                tracing::info!(version = %candidate.version, "update downloaded and verified");
                self.stage = UpdateStage::Ready { candidate, staged };
            }
            Err(UpdateError::Cancelled) => {
                self.stage = UpdateStage::Offered(candidate);
            }
            Err(error) => {
                tracing::warn!(error = %error, "update download failed");
                self.stage = UpdateStage::Failed {
                    candidate,
                    failure: failure_of(&error),
                    staged: None,
                };
            }
        }
        self.show_toast(cx);
        cx.notify();
    }

    pub fn download_task_lost(&mut self, ticket: u64, cx: &mut Context<Self>) {
        if ticket != self.ticket {
            return;
        }
        let Some(UpdateStage::Downloading { candidate, .. }) =
            self.take_stage(|stage| matches!(stage, UpdateStage::Downloading { .. }))
        else {
            return;
        };
        tracing::warn!(version = %candidate.version, "update download task ended without a result");
        self.fail(candidate, UpdateFailure::Download, None, cx);
    }

    pub fn cancel_download(&mut self, cx: &mut Context<Self>) {
        let Some(UpdateStage::Downloading {
            candidate, cancel, ..
        }) = self.take_stage(|stage| matches!(stage, UpdateStage::Downloading { .. }))
        else {
            return;
        };
        cancel.cancel();
        self.ticket = self.ticket.wrapping_add(1);
        self.stage = UpdateStage::Offered(candidate);
        self.show_toast(cx);
        cx.notify();
    }

    pub fn skip(&mut self, cx: &mut Context<Self>) {
        let Some(candidate) = self.candidate() else {
            return;
        };
        remember_skipped(
            candidate.version.clone(),
            Arc::clone(&self.backend),
            &self.rt_handle,
        );
        self.stage = UpdateStage::Idle;
        self.hide_toast(cx);
        cx.notify();
    }

    pub fn open_release_page(&mut self, cx: &mut Context<Self>) {
        let Some(candidate) = self.candidate() else {
            return;
        };
        detached(
            &self.rt_handle,
            "open the release page",
            open_path(candidate.release_page.clone()),
        );
        self.stage = UpdateStage::Idle;
        self.hide_toast(cx);
        cx.notify();
    }

    pub fn request_restart(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.stage, UpdateStage::Ready { .. }) {
            return;
        }
        self.restart_requested = true;
        cx.emit(UpdateFlowEvent::RestartRequested);
        cx.notify();
    }

    pub fn abort_restart(&mut self, cx: &mut Context<Self>) {
        if !self.restart_requested {
            return;
        }
        self.restart_requested = false;
        self.show_toast(cx);
        cx.notify();
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(UpdateStage::Failed {
            candidate, staged, ..
        }) = self.take_stage(|stage| matches!(stage, UpdateStage::Failed { .. }))
        else {
            return;
        };
        match staged {
            Some(staged) => {
                self.stage = UpdateStage::Ready { candidate, staged };
                self.show_toast(cx);
                self.request_restart(cx);
            }
            None => {
                self.stage = UpdateStage::Offered(candidate);
                self.start_download(cx);
            }
        }
        cx.notify();
    }

    pub fn close_decision(&mut self, cx: &mut Context<Self>) -> CloseDecision {
        match &self.stage {
            UpdateStage::HandingOff { .. } => CloseDecision::Hold,
            UpdateStage::Ready { .. } => self.begin_handoff(cx),
            _ => CloseDecision::Proceed,
        }
    }

    fn begin_handoff(&mut self, cx: &mut Context<Self>) -> CloseDecision {
        let Some(UpdateStage::Ready { candidate, staged }) =
            self.take_stage(|stage| matches!(stage, UpdateStage::Ready { .. }))
        else {
            return CloseDecision::Proceed;
        };
        let Some(target) = candidate.install.clone() else {
            self.stage = UpdateStage::Ready { candidate, staged };
            return CloseDecision::Proceed;
        };
        let mode = apply_mode(self.restart_requested);
        let input = HandoffInput {
            install: target.install,
            staged: staged.clone(),
            to_version: candidate.version.clone(),
            mode,
            data_dir: self.data_dir.clone(),
        };
        self.stage = UpdateStage::HandingOff {
            candidate,
            staged,
            mode,
        };
        run_async(
            &self.rt_handle,
            prepare_handoff(Arc::clone(&self.backend), input),
            |flow: &mut Self, result, cx| flow.handoff_settled(result, cx),
            cx,
        );
        self.show_toast(cx);
        cx.notify();
        CloseDecision::Hold
    }

    pub fn handoff_settled(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        let Some(UpdateStage::HandingOff {
            candidate,
            staged,
            mode,
        }) = self.take_stage(|stage| matches!(stage, UpdateStage::HandingOff { .. }))
        else {
            return;
        };
        self.restart_requested = false;
        match result {
            Ok(()) => {
                if let Some(exit) = cx.try_global::<ExitHandoff>() {
                    exit.arm(self.data_dir.clone());
                }
                tracing::info!(version = %candidate.version, ?mode, "update handed off");
                self.stage = UpdateStage::HandedOff { candidate, mode };
                self.show_toast(cx);
                cx.emit(UpdateFlowEvent::CloseApproved);
            }
            Err(error) => {
                tracing::warn!(error = %error, ?mode, "update could not be handed off");
                self.stage = UpdateStage::Failed {
                    candidate,
                    failure: UpdateFailure::Handoff,
                    staged: Some(staged),
                };
                self.show_toast(cx);
                if mode == ApplyMode::OnQuit {
                    cx.emit(UpdateFlowEvent::CloseApproved);
                }
            }
        }
        cx.notify();
    }

    fn take_stage(&mut self, wanted: impl Fn(&UpdateStage) -> bool) -> Option<UpdateStage> {
        wanted(&self.stage).then(|| std::mem::replace(&mut self.stage, UpdateStage::Idle))
    }

    fn candidate(&self) -> Option<&UpdateCandidate> {
        match &self.stage {
            UpdateStage::Idle => None,
            UpdateStage::Offered(candidate)
            | UpdateStage::Downloading { candidate, .. }
            | UpdateStage::Ready { candidate, .. }
            | UpdateStage::Failed { candidate, .. }
            | UpdateStage::HandingOff { candidate, .. }
            | UpdateStage::HandedOff { candidate, .. } => Some(candidate),
        }
    }

    fn fail(
        &mut self,
        candidate: UpdateCandidate,
        failure: UpdateFailure,
        staged: Option<StagedArtifact>,
        cx: &mut Context<Self>,
    ) {
        self.stage = UpdateStage::Failed {
            candidate,
            failure,
            staged,
        };
        self.show_toast(cx);
        cx.notify();
    }

    fn show_toast(&mut self, cx: &mut Context<Self>) {
        let Some(content) = update_toast(&self.stage) else {
            self.hide_toast(cx);
            return;
        };
        let data = toast_data(content, cx.weak_entity(), run_command);
        let toasts = cx.global_mut::<Toasts>();
        match self.toast {
            Some(id) if toasts.contains(id) => {
                toasts.replace(id, data);
            }
            _ => self.toast = Some(toasts.show(data)),
        }
    }

    fn refresh_toast(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.toast else {
            return;
        };
        if !cx.global::<Toasts>().contains(id) {
            return;
        }
        let Some(content) = update_toast(&self.stage) else {
            return;
        };
        let data = toast_data(content, cx.weak_entity(), run_command);
        cx.global_mut::<Toasts>().replace(id, data);
    }

    fn hide_toast(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.toast.take() {
            cx.global_mut::<Toasts>().dismiss(id);
        }
    }
}

fn run_command(flow: &WeakEntity<UpdateFlow>, command: UpdateCommand, cx: &mut App) {
    let _ = flow.update(cx, |flow, cx| flow.run(command, cx));
}
