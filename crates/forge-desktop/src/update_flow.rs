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
use crate::update_check::remember_skipped;
use crate::update_exit::ExitHandoff;
use crate::update_handoff::{HandoffInput, prepare_handoff};
use crate::update_toast::{UpdateCommand, toast_data, update_toast};
use forge_components::Toasts;

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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use forge_storage::{Language, reserved_keys};
    use forge_update::{InstallLayout, Sha256Digest};
    use gpui::{AppContext, Entity, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{SettingWrite, pump, runtime, test_backend};

    const VERSION: &str = "0.9.0";
    const RELEASE_PAGE: &str = "https://example.invalid/releases/v0.9.0";
    const ARTIFACT_SIZE: u64 = 100;

    struct Harness {
        flow: Entity<UpdateFlow>,
        events: Rc<RefCell<Vec<UpdateFlowEvent>>>,
        rt: tokio::runtime::Runtime,
        writes: tokio::sync::mpsc::UnboundedReceiver<SettingWrite>,
    }

    fn harness(cx: &mut TestAppContext) -> Harness {
        install_language(Language::En);
        let rt = runtime();
        let (backend, writes) = test_backend();
        let handle = rt.handle().clone();
        cx.update(|cx| cx.set_global(Toasts::new()));
        let flow = cx.new(|_| {
            UpdateFlow::new(
                backend as Arc<dyn DataProvider>,
                handle,
                PathBuf::from("/data/forge"),
            )
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        cx.update(|cx| {
            cx.subscribe(&flow, move |_, event: &UpdateFlowEvent, _| {
                sink.borrow_mut().push(*event);
            })
            .detach();
        });
        Harness {
            flow,
            events,
            rt,
            writes,
        }
    }

    fn install_target(target: &str) -> InstallTarget {
        InstallTarget {
            install: UpdatableInstall {
                layout: InstallLayout::LinuxAppImage,
                target: PathBuf::from(target),
            },
            artifact: Artifact {
                platform: "linux-x64".to_owned(),
                kind: "appimage".to_owned(),
                name: "forge.AppImage".to_owned(),
                url: "https://example.invalid/forge.AppImage".to_owned(),
                sha256: Sha256Digest::of(b"forge"),
                size: ARTIFACT_SIZE,
            },
        }
    }

    fn installable() -> UpdateCandidate {
        UpdateCandidate {
            version: VERSION.to_owned(),
            release_page: RELEASE_PAGE.to_owned(),
            install: Some(install_target("/opt/forge/forge.AppImage")),
        }
    }

    fn download_only() -> UpdateCandidate {
        UpdateCandidate {
            install: None,
            ..installable()
        }
    }

    fn staged() -> StagedArtifact {
        StagedArtifact {
            path: PathBuf::from("/opt/forge/.forge-staged-forge.AppImage"),
            size: ARTIFACT_SIZE,
        }
    }

    fn progress(received: u64, total: u64) -> DownloadProgress {
        DownloadProgress { received, total }
    }

    fn stage(cx: &mut TestAppContext, h: &Harness) -> UpdateStage {
        h.flow.read_with(cx, |flow, _| flow.stage.clone())
    }

    fn ticket(cx: &mut TestAppContext, h: &Harness) -> u64 {
        h.flow.read_with(cx, |flow, _| flow.ticket)
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(u64, String, Option<f32>, Option<String>)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.id,
                        toast.message.to_string(),
                        toast.progress,
                        toast.action.as_ref().map(|a| a.label.to_string()),
                    )
                })
                .collect()
        })
    }

    fn offer(cx: &mut TestAppContext, h: &Harness, candidate: UpdateCandidate) {
        h.flow.update(cx, |flow, cx| flow.offer(candidate, cx));
    }

    fn run(cx: &mut TestAppContext, h: &Harness, command: UpdateCommand) {
        h.flow.update(cx, |flow, cx| flow.run(command, cx));
    }

    fn downloading(cx: &mut TestAppContext, h: &Harness) -> u64 {
        offer(cx, h, installable());
        run(cx, h, UpdateCommand::StartDownload);
        ticket(cx, h)
    }

    fn finish(
        cx: &mut TestAppContext,
        h: &Harness,
        ticket: u64,
        result: Result<StagedArtifact, UpdateError>,
    ) {
        h.flow
            .update(cx, |flow, cx| flow.finish_download(ticket, result, cx));
    }

    fn tick(cx: &mut TestAppContext, h: &Harness, ticket: u64, update: DownloadProgress) {
        h.flow
            .update(cx, |flow, cx| flow.apply_progress(ticket, update, cx));
    }

    fn ready(cx: &mut TestAppContext, h: &Harness) {
        let ticket = downloading(cx, h);
        finish(cx, h, ticket, Ok(staged()));
    }

    fn close(cx: &mut TestAppContext, h: &Harness) -> CloseDecision {
        h.flow.update(cx, |flow, cx| flow.close_decision(cx))
    }

    fn settle(cx: &mut TestAppContext, h: &Harness, result: Result<(), String>) {
        h.flow
            .update(cx, |flow, cx| flow.handoff_settled(result, cx));
        cx.run_until_parked();
    }

    fn events(cx: &mut TestAppContext, h: &Harness) -> Vec<UpdateFlowEvent> {
        cx.run_until_parked();
        h.events.borrow().clone()
    }

    #[test]
    fn download_percent_floors_and_clamps_across_the_byte_range() {
        for (received, total, expected) in [
            (0, 100, 0),
            (42, 100, 42),
            (99, 100, 99),
            (100, 100, 100),
            (101, 100, 100),
            (u64::MAX, u64::MAX, 100),
            (u64::MAX - 1, u64::MAX, 99),
            (1, 3, 33),
            (5, 0, 0),
            (0, 0, 0),
        ] {
            assert_eq!(
                download_percent(progress(received, total)),
                expected,
                "{received}/{total}"
            );
        }
    }

    #[test]
    fn download_fraction_stays_within_the_unit_range_even_without_a_total() {
        for (received, total, expected) in [
            (0, 100, 0.0),
            (42, 100, 0.42),
            (100, 100, 1.0),
            (250, 100, 1.0),
            (5, 0, 0.0),
            (0, 0, 0.0),
        ] {
            let fraction = download_fraction(progress(received, total));
            assert!(
                (fraction - expected).abs() < f32::EPSILON,
                "{received}/{total} gave {fraction}"
            );
        }
    }

    #[test]
    fn failure_of_marks_integrity_errors_unverified_and_the_rest_as_download_failures() {
        for (error, expected) in [
            (UpdateError::ArtifactHashMismatch, UpdateFailure::Unverified),
            (
                UpdateError::ArtifactSizeMismatch {
                    expected: 10,
                    actual: 9,
                },
                UpdateFailure::Unverified,
            ),
            (
                UpdateError::ArtifactOversized { expected: 10 },
                UpdateFailure::Unverified,
            ),
            (UpdateError::InvalidArtifactName, UpdateFailure::Unverified),
            (
                UpdateError::Network("reset".to_owned()),
                UpdateFailure::Download,
            ),
            (UpdateError::Status(404), UpdateFailure::Download),
            (UpdateError::RateLimited, UpdateFailure::Download),
            (
                UpdateError::Io(std::io::Error::other("disk full")),
                UpdateFailure::Download,
            ),
        ] {
            assert_eq!(failure_of(&error), expected, "{error}");
        }
    }

    #[gpui::test]
    fn start_download_moves_an_installable_offer_to_downloading(cx: &mut TestAppContext) {
        let h = harness(cx);
        downloading(cx, &h);

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading { .. }));
    }

    #[gpui::test]
    fn start_download_ignores_a_download_only_offer(cx: &mut TestAppContext) {
        let h = harness(cx);
        offer(cx, &h, download_only());
        run(cx, &h, UpdateCommand::StartDownload);

        assert!(matches!(stage(cx, &h), UpdateStage::Offered(_)));
    }

    #[gpui::test]
    fn start_download_fails_when_the_install_has_no_staging_directory(cx: &mut TestAppContext) {
        let h = harness(cx);
        offer(
            cx,
            &h,
            UpdateCandidate {
                install: Some(install_target("/")),
                ..installable()
            },
        );
        run(cx, &h, UpdateCommand::StartDownload);

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Failed {
                failure: UpdateFailure::Download,
                staged: None,
                ..
            }
        ));
    }

    #[gpui::test]
    fn an_offer_arriving_mid_download_does_not_reset_the_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        downloading(cx, &h);
        offer(cx, &h, download_only());

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading { .. }));
    }

    #[gpui::test]
    fn a_verified_download_becomes_ready_with_the_staged_file(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Ready { staged: s, .. } if s == staged()
        ));
    }

    #[gpui::test]
    fn cancel_returns_to_the_offer_without_a_staged_file(cx: &mut TestAppContext) {
        let h = harness(cx);
        downloading(cx, &h);
        run(cx, &h, UpdateCommand::Cancel);

        assert!(matches!(stage(cx, &h), UpdateStage::Offered(_)));
    }

    #[gpui::test]
    fn cancel_signals_the_running_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        downloading(cx, &h);
        let UpdateStage::Downloading { cancel, .. } = stage(cx, &h) else {
            panic!("not downloading");
        };
        run(cx, &h, UpdateCommand::Cancel);

        assert!(cancel.is_cancelled());
    }

    #[gpui::test]
    fn a_download_that_reports_cancelled_returns_to_the_offer(cx: &mut TestAppContext) {
        let h = harness(cx);
        let ticket = downloading(cx, &h);
        finish(cx, &h, ticket, Err(UpdateError::Cancelled));

        assert!(matches!(stage(cx, &h), UpdateStage::Offered(_)));
    }

    #[gpui::test]
    fn a_failed_download_fails_with_its_kind_and_nothing_staged(cx: &mut TestAppContext) {
        let h = harness(cx);
        let ticket = downloading(cx, &h);
        finish(cx, &h, ticket, Err(UpdateError::ArtifactHashMismatch));

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Failed {
                failure: UpdateFailure::Unverified,
                staged: None,
                ..
            }
        ));
    }

    #[gpui::test]
    fn retry_after_a_failed_download_starts_a_fresh_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        let first = downloading(cx, &h);
        finish(cx, &h, first, Err(UpdateError::Network("reset".to_owned())));
        run(cx, &h, UpdateCommand::Retry);

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading { .. }));
        assert_ne!(ticket(cx, &h), first);
    }

    #[gpui::test]
    fn a_result_from_a_cancelled_download_cannot_finish_the_next_one(cx: &mut TestAppContext) {
        let h = harness(cx);
        let stale = downloading(cx, &h);
        run(cx, &h, UpdateCommand::Cancel);
        run(cx, &h, UpdateCommand::StartDownload);
        finish(cx, &h, stale, Ok(staged()));

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading { .. }));
    }

    #[gpui::test]
    fn progress_from_a_cancelled_download_cannot_move_the_next_one(cx: &mut TestAppContext) {
        let h = harness(cx);
        let stale = downloading(cx, &h);
        run(cx, &h, UpdateCommand::Cancel);
        run(cx, &h, UpdateCommand::StartDownload);
        tick(cx, &h, stale, progress(90, 100));

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Downloading { progress: p, .. } if p.received == 0
        ));
    }

    #[gpui::test]
    fn skip_forgets_the_offer_and_remembers_its_version(cx: &mut TestAppContext) {
        let mut h = harness(cx);
        offer(cx, &h, installable());
        run(cx, &h, UpdateCommand::Skip);
        pump(&h.rt);

        assert!(toasts(cx).is_empty());
        assert_eq!(
            h.writes.try_recv().ok(),
            Some((
                reserved_keys::UPDATES_SKIPPED_VERSION.to_owned(),
                VERSION.to_owned()
            ))
        );
    }

    #[gpui::test]
    fn progress_ticks_replace_the_offer_toast_in_place(cx: &mut TestAppContext) {
        let h = harness(cx);
        offer(cx, &h, installable());
        let offered_id = toasts(cx)[0].0;
        run(cx, &h, UpdateCommand::StartDownload);
        let ticket = ticket(cx, &h);
        tick(cx, &h, ticket, progress(42, 100));

        let shown = toasts(cx);
        assert_eq!(shown.len(), 1);
        let (id, message, fraction, _) = &shown[0];
        assert_eq!(*id, offered_id);
        assert!(message.contains("42"), "{message}");
        assert_eq!(*fraction, Some(0.42));
    }

    #[gpui::test]
    fn a_dismissed_progress_toast_stays_hidden_on_later_ticks(cx: &mut TestAppContext) {
        let h = harness(cx);
        let ticket = downloading(cx, &h);
        let id = toasts(cx)[0].0;
        cx.update(|cx| cx.global_mut::<Toasts>().dismiss(id));
        tick(cx, &h, ticket, progress(42, 100));

        assert!(toasts(cx).is_empty());
    }

    #[gpui::test]
    fn the_ready_toast_shows_after_the_progress_toast_was_dismissed(cx: &mut TestAppContext) {
        let h = harness(cx);
        let ticket = downloading(cx, &h);
        let id = toasts(cx)[0].0;
        cx.update(|cx| cx.global_mut::<Toasts>().dismiss(id));
        finish(cx, &h, ticket, Ok(staged()));

        let actions: Vec<Option<String>> = toasts(cx).into_iter().map(|(_, _, _, a)| a).collect();
        assert_eq!(actions, vec![Some("Restart to update".to_owned())]);
    }

    #[gpui::test]
    fn close_proceeds_while_no_update_is_ready(cx: &mut TestAppContext) {
        let h = harness(cx);
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "idle");
        offer(cx, &h, installable());
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "offered");
        run(cx, &h, UpdateCommand::StartDownload);
        let ticket = ticket(cx, &h);
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "downloading");
        finish(cx, &h, ticket, Err(UpdateError::Status(500)));
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "failed");
    }

    #[gpui::test]
    fn close_holds_a_ready_update_and_hands_it_off_on_quit(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);

        assert_eq!(close(cx, &h), CloseDecision::Hold);
        assert!(matches!(
            stage(cx, &h),
            UpdateStage::HandingOff {
                mode: ApplyMode::OnQuit,
                ..
            }
        ));
    }

    #[gpui::test]
    fn a_second_close_during_the_handoff_keeps_holding(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        close(cx, &h);

        assert_eq!(close(cx, &h), CloseDecision::Hold);
    }

    #[gpui::test]
    fn restart_on_a_ready_update_asks_the_shell_to_close(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        run(cx, &h, UpdateCommand::Restart);

        assert_eq!(events(cx, &h), vec![UpdateFlowEvent::RestartRequested]);
    }

    #[gpui::test]
    fn restart_before_the_update_is_ready_is_ignored(cx: &mut TestAppContext) {
        let h = harness(cx);
        downloading(cx, &h);
        run(cx, &h, UpdateCommand::Restart);

        assert!(events(cx, &h).is_empty());
    }

    #[gpui::test]
    fn a_settled_handoff_after_restart_approves_the_close_in_restart_mode(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        run(cx, &h, UpdateCommand::Restart);
        close(cx, &h);
        settle(cx, &h, Ok(()));

        assert_eq!(
            events(cx, &h),
            vec![
                UpdateFlowEvent::RestartRequested,
                UpdateFlowEvent::CloseApproved
            ]
        );
        assert!(matches!(
            stage(cx, &h),
            UpdateStage::HandedOff {
                mode: ApplyMode::Restart,
                ..
            }
        ));
    }

    #[gpui::test]
    fn a_settled_handoff_on_plain_close_approves_the_close_on_quit(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        close(cx, &h);
        settle(cx, &h, Ok(()));

        assert_eq!(events(cx, &h), vec![UpdateFlowEvent::CloseApproved]);
        assert!(matches!(
            stage(cx, &h),
            UpdateStage::HandedOff {
                mode: ApplyMode::OnQuit,
                ..
            }
        ));
    }

    #[gpui::test]
    fn a_failed_handoff_after_restart_keeps_the_window_open(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        run(cx, &h, UpdateCommand::Restart);
        close(cx, &h);
        settle(cx, &h, Err("snapshot failed".to_owned()));

        assert_eq!(events(cx, &h), vec![UpdateFlowEvent::RestartRequested]);
        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Failed {
                failure: UpdateFailure::Handoff,
                staged: Some(_),
                ..
            }
        ));
    }

    #[gpui::test]
    fn a_failed_handoff_on_quit_still_lets_the_app_quit(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        close(cx, &h);
        settle(cx, &h, Err("snapshot failed".to_owned()));

        assert_eq!(events(cx, &h), vec![UpdateFlowEvent::CloseApproved]);
        assert_eq!(close(cx, &h), CloseDecision::Proceed);
    }

    #[gpui::test]
    fn retry_after_a_failed_handoff_requests_the_restart_again(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        run(cx, &h, UpdateCommand::Restart);
        close(cx, &h);
        settle(cx, &h, Err("snapshot failed".to_owned()));
        run(cx, &h, UpdateCommand::Retry);

        assert_eq!(
            events(cx, &h),
            vec![
                UpdateFlowEvent::RestartRequested,
                UpdateFlowEvent::RestartRequested
            ]
        );
        assert!(matches!(stage(cx, &h), UpdateStage::Ready { .. }));
    }

    #[gpui::test]
    fn a_cancelled_leave_dialog_shows_the_ready_toast_again(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        let id = toasts(cx)[0].0;
        cx.update(|cx| cx.global_mut::<Toasts>().take(id));
        run(cx, &h, UpdateCommand::Restart);
        h.flow.update(cx, |flow, cx| flow.abort_restart(cx));

        let actions: Vec<Option<String>> = toasts(cx).into_iter().map(|(_, _, _, a)| a).collect();
        assert_eq!(actions, vec![Some("Restart to update".to_owned())]);
    }

    #[gpui::test]
    fn a_cancelled_leave_dialog_falls_back_to_installing_on_quit(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        run(cx, &h, UpdateCommand::Restart);
        h.flow.update(cx, |flow, cx| flow.abort_restart(cx));
        close(cx, &h);

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::HandingOff {
                mode: ApplyMode::OnQuit,
                ..
            }
        ));
    }

    #[gpui::test]
    fn a_lost_download_task_fails_the_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        let ticket = downloading(cx, &h);
        h.flow
            .update(cx, |flow, cx| flow.download_task_lost(ticket, cx));

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Failed {
                failure: UpdateFailure::Download,
                staged: None,
                ..
            }
        ));
    }

    #[gpui::test]
    fn a_lost_task_from_a_cancelled_download_leaves_the_next_one_running(cx: &mut TestAppContext) {
        let h = harness(cx);
        let stale = downloading(cx, &h);
        run(cx, &h, UpdateCommand::Cancel);
        run(cx, &h, UpdateCommand::StartDownload);
        h.flow
            .update(cx, |flow, cx| flow.download_task_lost(stale, cx));

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading { .. }));
    }

    #[gpui::test]
    fn cancel_without_a_running_download_keeps_the_offer(cx: &mut TestAppContext) {
        let h = harness(cx);
        offer(cx, &h, installable());
        run(cx, &h, UpdateCommand::Cancel);

        assert!(matches!(stage(cx, &h), UpdateStage::Offered(_)));
    }

    #[gpui::test]
    fn retry_on_a_ready_update_keeps_it_ready(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);
        run(cx, &h, UpdateCommand::Retry);

        assert!(matches!(stage(cx, &h), UpdateStage::Ready { .. }));
    }

    #[gpui::test]
    fn a_handoff_result_without_a_running_handoff_keeps_the_offer(cx: &mut TestAppContext) {
        let h = harness(cx);
        offer(cx, &h, installable());
        settle(cx, &h, Ok(()));

        assert!(matches!(stage(cx, &h), UpdateStage::Offered(_)));
        assert!(events(cx, &h).is_empty());
    }

    #[gpui::test]
    fn a_download_result_arriving_after_ready_keeps_the_staged_update(cx: &mut TestAppContext) {
        let h = harness(cx);
        let ticket = downloading(cx, &h);
        finish(cx, &h, ticket, Ok(staged()));
        finish(cx, &h, ticket, Err(UpdateError::Status(500)));

        assert!(matches!(stage(cx, &h), UpdateStage::Ready { .. }));
    }
}
