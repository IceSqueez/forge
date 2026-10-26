use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_update::{
    ApplyMode, Artifact, CancelSignal, StagedArtifact, UpdatableInstall, UpdateClient, UpdateError,
    download_artifact, progress_channel,
};
use gpui::{App, Context, EventEmitter, WeakEntity};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use crate::async_bridge::{detached, open_path, run_async};
use crate::update_exit::ExitHandoff;
use crate::update_handoff::{HandoffError, HandoffInput, prepare_handoff, prepare_handoff_holding};
use crate::update_toast::{UpdateCommand, toast_data, update_toast};
use forge_components::Toasts;
use forge_transfer::{ScheduleState, ScheduledBackupHandle, TransferExclusivity, TransferGuard};

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
    Downloading(UpdateCandidate),
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
    WaitingForTransfer {
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
    silent_retry_spent: bool,
    toast: Option<u64>,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    data_dir: PathBuf,
    exclusivity: TransferExclusivity,
    scheduled: Option<ScheduledBackupHandle>,
    scheduled_suspended: bool,
    close_asked_while_waiting: bool,
    wait: Option<CancellationToken>,
}

impl EventEmitter<UpdateFlowEvent> for UpdateFlow {}

impl UpdateFlow {
    pub fn new(backend: Arc<dyn DataProvider>, rt_handle: Handle, data_dir: PathBuf) -> Self {
        Self {
            stage: UpdateStage::Idle,
            restart_requested: false,
            ticket: 0,
            silent_retry_spent: false,
            toast: None,
            backend,
            rt_handle,
            data_dir,
            exclusivity: TransferExclusivity::new(),
            scheduled: None,
            scheduled_suspended: false,
            close_asked_while_waiting: false,
            wait: None,
        }
    }

    #[must_use]
    pub fn with_scheduled_backups(mut self, scheduled: ScheduledBackupHandle) -> Self {
        self.scheduled = Some(scheduled);
        self
    }

    #[must_use]
    pub fn with_exclusivity(mut self, exclusivity: TransferExclusivity) -> Self {
        self.exclusivity = exclusivity;
        self
    }

    pub fn offer(&mut self, candidate: UpdateCandidate, cx: &mut Context<Self>) {
        if !matches!(self.stage, UpdateStage::Idle | UpdateStage::Offered(_)) {
            return;
        }
        self.stage = UpdateStage::Offered(candidate);
        self.show_toast(cx);
        cx.notify();
    }

    pub fn download(&mut self, candidate: UpdateCandidate, cx: &mut Context<Self>) {
        if !matches!(self.stage, UpdateStage::Idle | UpdateStage::Offered(_)) {
            return;
        }
        self.start_download(candidate, cx);
    }

    pub fn run(&mut self, command: UpdateCommand, cx: &mut Context<Self>) {
        match command {
            UpdateCommand::OpenReleasePage => self.open_release_page(cx),
            UpdateCommand::Restart => self.request_restart(cx),
            UpdateCommand::Retry => self.retry(cx),
        }
    }

    fn start_download(&mut self, candidate: UpdateCandidate, cx: &mut Context<Self>) {
        let Some(target) = candidate.install.clone() else {
            return;
        };
        let Some(staging_dir) = target.install.staging_dir().map(PathBuf::from) else {
            self.fail(candidate, UpdateFailure::Download, None, cx);
            return;
        };

        self.ticket = self.ticket.wrapping_add(1);
        let ticket = self.ticket;
        self.stage = UpdateStage::Downloading(candidate);

        let artifact = target.artifact;
        let download = async move {
            let (progress, _) = progress_channel();
            match UpdateClient::for_github() {
                Ok(client) => {
                    download_artifact(
                        &client,
                        &artifact,
                        &staging_dir,
                        &progress,
                        &CancelSignal::new(),
                    )
                    .await
                }
                Err(error) => Err(error),
            }
        };
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = done_tx.send(download.await);
        });

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = match done_rx.await {
                Ok(result) => this.update(cx, |flow, cx| flow.finish_download(ticket, result, cx)),
                Err(_) => this.update(cx, |flow, cx| flow.download_task_lost(ticket, cx)),
            };
        })
        .detach();

        self.show_toast(cx);
        cx.notify();
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
        let Some(UpdateStage::Downloading(candidate)) =
            self.take_stage(|stage| matches!(stage, UpdateStage::Downloading(_)))
        else {
            return;
        };
        match result {
            Ok(staged) => {
                tracing::info!(version = %candidate.version, "update downloaded and verified");
                self.stage = UpdateStage::Ready { candidate, staged };
                self.show_toast(cx);
            }
            Err(error) => {
                tracing::warn!(error = %error, "update download failed");
                self.download_failed(candidate, failure_of(&error), cx);
            }
        }
        cx.notify();
    }

    pub fn download_task_lost(&mut self, ticket: u64, cx: &mut Context<Self>) {
        if ticket != self.ticket {
            return;
        }
        let Some(UpdateStage::Downloading(candidate)) =
            self.take_stage(|stage| matches!(stage, UpdateStage::Downloading(_)))
        else {
            return;
        };
        tracing::warn!(version = %candidate.version, "update download task ended without a result");
        self.download_failed(candidate, UpdateFailure::Download, cx);
    }

    fn download_failed(
        &mut self,
        candidate: UpdateCandidate,
        failure: UpdateFailure,
        cx: &mut Context<Self>,
    ) {
        if self.silent_retry_spent {
            self.fail(candidate, failure, None, cx);
            return;
        }
        self.silent_retry_spent = true;
        tracing::info!(version = %candidate.version, "retrying the update download");
        self.start_download(candidate, cx);
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

    pub fn has_staged_update(&self) -> bool {
        matches!(
            self.stage,
            UpdateStage::Ready { .. }
                | UpdateStage::HandingOff { .. }
                | UpdateStage::WaitingForTransfer { .. }
                | UpdateStage::HandedOff { .. }
        )
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
            None => self.start_download(candidate, cx),
        }
        cx.notify();
    }

    pub fn close_decision(&mut self, cx: &mut Context<Self>) -> CloseDecision {
        match &self.stage {
            UpdateStage::HandingOff { .. } => CloseDecision::Hold,
            UpdateStage::WaitingForTransfer { .. } => self.close_while_waiting(cx),
            UpdateStage::Ready { .. } => self.begin_handoff(cx),
            _ => CloseDecision::Proceed,
        }
    }

    fn close_while_waiting(&mut self, cx: &mut Context<Self>) -> CloseDecision {
        if !self.close_asked_while_waiting {
            self.close_asked_while_waiting = true;
            return CloseDecision::Hold;
        }
        let Some(UpdateStage::WaitingForTransfer {
            candidate, staged, ..
        }) = self.take_stage(|stage| matches!(stage, UpdateStage::WaitingForTransfer { .. }))
        else {
            return CloseDecision::Proceed;
        };
        self.stop_waiting();
        self.restart_requested = false;
        self.resume_scheduled_backups();
        self.stage = UpdateStage::Ready { candidate, staged };
        self.show_toast(cx);
        cx.notify();
        CloseDecision::Proceed
    }

    fn stop_waiting(&mut self) {
        self.close_asked_while_waiting = false;
        if let Some(wait) = self.wait.take() {
            wait.cancel();
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
            exclusivity: self.exclusivity.clone(),
        };
        self.stage = UpdateStage::HandingOff {
            candidate,
            staged,
            mode,
        };
        run_async(
            &self.rt_handle,
            prepare_handoff(Arc::clone(&self.backend), input.clone()),
            move |flow: &mut Self, result, cx| flow.handoff_attempted(result, input, cx),
            cx,
        );
        self.show_toast(cx);
        cx.notify();
        CloseDecision::Hold
    }

    fn handoff_attempted(
        &mut self,
        result: Result<(), HandoffError>,
        input: HandoffInput,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => self.handoff_settled(Ok(()), cx),
            Err(HandoffError::Failed(error)) => self.handoff_settled(Err(error), cx),
            Err(HandoffError::Busy) => self.wait_for_transfer(input, cx),
        }
    }

    fn wait_for_transfer(&mut self, input: HandoffInput, cx: &mut Context<Self>) {
        let Some(UpdateStage::HandingOff {
            candidate,
            staged,
            mode,
        }) = self.take_stage(|stage| matches!(stage, UpdateStage::HandingOff { .. }))
        else {
            return;
        };
        if let Some(scheduled) = &self.scheduled
            && scheduled.status().borrow().state == ScheduleState::Running
        {
            scheduled.set_suspended(true);
            self.scheduled_suspended = true;
        }
        self.stage = UpdateStage::WaitingForTransfer {
            candidate,
            staged,
            mode,
        };
        let exclusivity = input.exclusivity.clone();
        let cancel = CancellationToken::new();
        self.wait = Some(cancel.clone());
        self.close_asked_while_waiting = true;
        run_async(
            &self.rt_handle,
            async move {
                tokio::select! {
                    guard = exclusivity.acquire_when_free() => Some(guard),
                    () = cancel.cancelled() => None,
                }
            },
            move |flow: &mut Self, guard, cx| {
                if let Some(guard) = guard {
                    flow.transfer_finished(guard, input, cx);
                }
            },
            cx,
        );
        self.show_toast(cx);
        cx.notify();
    }

    fn transfer_finished(
        &mut self,
        guard: TransferGuard,
        input: HandoffInput,
        cx: &mut Context<Self>,
    ) {
        let Some(UpdateStage::WaitingForTransfer {
            candidate,
            staged,
            mode,
        }) = self.take_stage(|stage| matches!(stage, UpdateStage::WaitingForTransfer { .. }))
        else {
            return;
        };
        self.stop_waiting();
        self.stage = UpdateStage::HandingOff {
            candidate,
            staged,
            mode,
        };
        run_async(
            &self.rt_handle,
            prepare_handoff_holding(Arc::clone(&self.backend), input, guard),
            |flow: &mut Self, result, cx| flow.handoff_settled(result, cx),
            cx,
        );
        self.show_toast(cx);
        cx.notify();
    }

    fn resume_scheduled_backups(&mut self) {
        if std::mem::take(&mut self.scheduled_suspended)
            && let Some(scheduled) = &self.scheduled
        {
            scheduled.set_suspended(false);
        }
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
                    exit.arm_update(self.data_dir.clone());
                }
                tracing::info!(version = %candidate.version, ?mode, "update handed off");
                self.stage = UpdateStage::HandedOff { candidate, mode };
                self.show_toast(cx);
                cx.emit(UpdateFlowEvent::CloseApproved);
            }
            Err(error) => {
                self.resume_scheduled_backups();
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
            | UpdateStage::Downloading(candidate)
            | UpdateStage::Ready { candidate, .. }
            | UpdateStage::Failed { candidate, .. }
            | UpdateStage::HandingOff { candidate, .. }
            | UpdateStage::WaitingForTransfer { candidate, .. }
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

    fn hide_toast(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.toast.take() {
            cx.global_mut::<Toasts>().dismiss(id);
        }
    }
}

fn run_command(flow: &WeakEntity<UpdateFlow>, command: UpdateCommand, cx: &mut App) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the view may close before the result arrives"
    )]
    let _ = flow.update(cx, |flow, cx| flow.run(command, cx));
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use forge_storage::Language;
    use forge_update::{InstallLayout, Sha256Digest};
    use gpui::{AppContext, Entity, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        GatedSchedule, TestBackend, gated_running_schedule_sharing, keyless_enabled_schedule, pump,
        runtime, test_backend, test_backend_with_gated_export,
    };

    const VERSION: &str = "0.9.0";
    const RELEASE_PAGE: &str = "https://example.invalid/releases/v0.9.0";
    const ARTIFACT_SIZE: u64 = 100;
    const DRIVE_ROUNDS: usize = 4;
    const BLOCKING_POOL_TURN: std::time::Duration = std::time::Duration::from_millis(1);
    const SCHEDULE_SETTLE_GUARD: std::time::Duration = std::time::Duration::from_secs(30);

    struct Harness {
        flow: Entity<UpdateFlow>,
        events: Rc<RefCell<Vec<UpdateFlowEvent>>>,
        rt: tokio::runtime::Runtime,
    }

    fn harness(cx: &mut TestAppContext) -> Harness {
        harness_on(
            cx,
            runtime(),
            test_backend().0,
            TransferExclusivity::new(),
            None,
            PathBuf::from("/data/forge"),
        )
    }

    fn harness_on(
        cx: &mut TestAppContext,
        rt: tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
        exclusivity: TransferExclusivity,
        scheduled: Option<ScheduledBackupHandle>,
        data_dir: PathBuf,
    ) -> Harness {
        install_language(Language::En);
        let handle = rt.handle().clone();
        cx.update(|cx| cx.set_global(Toasts::new()));
        let flow = cx.new(|_| {
            let flow = UpdateFlow::new(backend as Arc<dyn DataProvider>, handle, data_dir)
                .with_exclusivity(exclusivity);
            match scheduled {
                Some(scheduled) => flow.with_scheduled_backups(scheduled),
                None => flow,
            }
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        cx.update(|cx| {
            cx.subscribe(&flow, move |_, event: &UpdateFlowEvent, _| {
                sink.borrow_mut().push(*event);
            })
            .detach();
        });
        Harness { flow, events, rt }
    }

    struct Waiting {
        h: Harness,
        handoff_export: Arc<tokio::sync::Notify>,
        schedule: Option<GatedSchedule>,
        _data_dir: tempfile::TempDir,
    }

    fn waiting_harness(
        cx: &mut TestAppContext,
        exclusivity: TransferExclusivity,
        schedule: impl FnOnce(TransferExclusivity) -> Option<GatedSchedule>,
    ) -> Waiting {
        let rt = runtime();
        let schedule = {
            let _entered = rt.enter();
            schedule(exclusivity.clone())
        };
        let (backend, handoff_export) = test_backend_with_gated_export();
        let scheduled = schedule.as_ref().map(|s| s.handle.clone());
        let data_dir = tempfile::tempdir().unwrap();
        let h = harness_on(
            cx,
            rt,
            backend,
            exclusivity,
            scheduled,
            data_dir.path().to_path_buf(),
        );
        Waiting {
            h,
            handoff_export,
            schedule,
            _data_dir: data_dir,
        }
    }

    fn settle_schedule(h: &Harness, schedule: &GatedSchedule, state: ScheduleState) -> bool {
        let mut status = schedule.handle.status();
        h.rt.block_on(async {
            tokio::time::timeout(SCHEDULE_SETTLE_GUARD, status.wait_for(|s| s.state == state))
                .await
                .is_ok()
        })
    }

    fn restart_and_close(cx: &mut TestAppContext, h: &Harness) -> CloseDecision {
        ready(cx, h);
        run(cx, h, UpdateCommand::Restart);
        let decision = close(cx, h);
        drive(cx, h);
        decision
    }

    fn drive_until(
        cx: &mut TestAppContext,
        h: &Harness,
        reached: impl Fn(&UpdateStage) -> bool,
    ) -> bool {
        let started = std::time::Instant::now();
        loop {
            drive(cx, h);
            if reached(&stage(cx, h)) {
                return true;
            }
            if started.elapsed() > SCHEDULE_SETTLE_GUARD {
                return false;
            }
            h.rt.block_on(async { tokio::time::sleep(BLOCKING_POOL_TURN).await });
        }
    }

    fn waiting_to_restart(stage: &UpdateStage) -> bool {
        matches!(
            stage,
            UpdateStage::WaitingForTransfer {
                mode: ApplyMode::Restart,
                ..
            }
        )
    }

    fn waiting_to_install_on_quit(stage: &UpdateStage) -> bool {
        matches!(
            stage,
            UpdateStage::WaitingForTransfer {
                mode: ApplyMode::OnQuit,
                ..
            }
        )
    }

    fn ready_with_the_staged_update(stage: &UpdateStage) -> bool {
        matches!(stage, UpdateStage::Ready { staged: s, .. } if *s == staged())
    }

    fn close_into_the_wait(cx: &mut TestAppContext, h: &Harness) -> bool {
        ready(cx, h);
        close(cx, h);
        drive_until(cx, h, waiting_to_install_on_quit)
    }

    fn handing_off(stage: &UpdateStage) -> bool {
        matches!(stage, UpdateStage::HandingOff { .. })
    }

    fn handoff_failed(stage: &UpdateStage) -> bool {
        matches!(
            stage,
            UpdateStage::Failed {
                failure: UpdateFailure::Handoff,
                staged: Some(_),
                ..
            }
        )
    }

    fn drive(cx: &mut TestAppContext, h: &Harness) {
        for _ in 0..DRIVE_ROUNDS {
            pump(&h.rt);
            cx.run_until_parked();
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

    fn stage(cx: &mut TestAppContext, h: &Harness) -> UpdateStage {
        h.flow.read_with(cx, |flow, _| flow.stage.clone())
    }

    fn ticket(cx: &mut TestAppContext, h: &Harness) -> u64 {
        h.flow.read_with(cx, |flow, _| flow.ticket)
    }

    fn toast_actions(cx: &mut TestAppContext) -> Vec<Option<String>> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| toast.action.as_ref().map(|a| a.label.to_string()))
                .collect()
        })
    }

    fn toast_ids(cx: &mut TestAppContext) -> Vec<u64> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| toast.id)
                .collect()
        })
    }

    fn offer(cx: &mut TestAppContext, h: &Harness, candidate: UpdateCandidate) {
        h.flow.update(cx, |flow, cx| flow.offer(candidate, cx));
    }

    fn run(cx: &mut TestAppContext, h: &Harness, command: UpdateCommand) {
        h.flow.update(cx, |flow, cx| flow.run(command, cx));
    }

    fn download(cx: &mut TestAppContext, h: &Harness, candidate: UpdateCandidate) {
        h.flow.update(cx, |flow, cx| flow.download(candidate, cx));
    }

    fn downloading(cx: &mut TestAppContext, h: &Harness) -> u64 {
        download(cx, h, installable());
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

    fn network_error() -> Result<StagedArtifact, UpdateError> {
        Err(UpdateError::Network("reset".to_owned()))
    }

    fn failed_twice(cx: &mut TestAppContext, h: &Harness) {
        let first = downloading(cx, h);
        finish(cx, h, first, network_error());
        let second = ticket(cx, h);
        finish(cx, h, second, network_error());
    }

    fn lose(cx: &mut TestAppContext, h: &Harness, ticket: u64) {
        h.flow
            .update(cx, |flow, cx| flow.download_task_lost(ticket, cx));
    }

    fn is_download_failure(stage: &UpdateStage) -> bool {
        matches!(
            stage,
            UpdateStage::Failed {
                failure: UpdateFailure::Download,
                staged: None,
                ..
            }
        )
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
    fn download_runs_silently_without_a_toast(cx: &mut TestAppContext) {
        let h = harness(cx);
        downloading(cx, &h);

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
        assert!(toast_ids(cx).is_empty());
    }

    #[gpui::test]
    fn download_takes_over_an_offer_and_replaces_its_toast(cx: &mut TestAppContext) {
        let h = harness(cx);
        offer(cx, &h, download_only());
        downloading(cx, &h);

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
        assert!(toast_ids(cx).is_empty());
    }

    #[gpui::test]
    fn download_fails_when_the_install_has_no_staging_directory(cx: &mut TestAppContext) {
        let h = harness(cx);
        download(
            cx,
            &h,
            UpdateCandidate {
                install: Some(install_target("/")),
                ..installable()
            },
        );

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

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
    }

    #[gpui::test]
    fn a_second_download_request_does_not_restart_a_running_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        let running = downloading(cx, &h);
        download(cx, &h, installable());

        assert_eq!(ticket(cx, &h), running);
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
    fn a_finished_download_raises_the_restart_toast(cx: &mut TestAppContext) {
        let h = harness(cx);
        ready(cx, &h);

        assert_eq!(
            toast_actions(cx),
            vec![Some("Restart to update".to_owned())]
        );
    }

    #[gpui::test]
    fn a_first_download_failure_retries_silently(cx: &mut TestAppContext) {
        let h = harness(cx);
        let first = downloading(cx, &h);
        finish(cx, &h, first, network_error());

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
        assert_ne!(ticket(cx, &h), first);
        assert!(toast_ids(cx).is_empty());
    }

    #[gpui::test]
    fn a_second_download_failure_raises_the_failed_toast_with_retry(cx: &mut TestAppContext) {
        let h = harness(cx);
        let first = downloading(cx, &h);
        finish(cx, &h, first, network_error());
        let second = ticket(cx, &h);
        finish(cx, &h, second, Err(UpdateError::ArtifactHashMismatch));

        assert!(matches!(
            stage(cx, &h),
            UpdateStage::Failed {
                failure: UpdateFailure::Unverified,
                staged: None,
                ..
            }
        ));
        assert_eq!(toast_actions(cx), vec![Some("Retry".to_owned())]);
    }

    #[gpui::test]
    fn retry_after_a_failed_download_starts_a_fresh_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        failed_twice(cx, &h);
        let failed_at = ticket(cx, &h);
        run(cx, &h, UpdateCommand::Retry);

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
        assert_ne!(ticket(cx, &h), failed_at);
        assert!(toast_ids(cx).is_empty());
    }

    #[gpui::test]
    fn a_failing_manual_retry_raises_the_toast_without_another_silent_retry(
        cx: &mut TestAppContext,
    ) {
        let h = harness(cx);
        failed_twice(cx, &h);
        run(cx, &h, UpdateCommand::Retry);
        let retried = ticket(cx, &h);
        finish(cx, &h, retried, network_error());

        assert!(is_download_failure(&stage(cx, &h)));
        assert_eq!(toast_actions(cx), vec![Some("Retry".to_owned())]);
    }

    #[gpui::test]
    fn a_late_result_from_a_superseded_download_cannot_finish_the_retry(cx: &mut TestAppContext) {
        let h = harness(cx);
        let stale = downloading(cx, &h);
        finish(cx, &h, stale, network_error());
        finish(cx, &h, stale, Ok(staged()));

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
    }

    #[gpui::test]
    fn a_late_failure_from_a_superseded_download_does_not_spend_the_retry_again(
        cx: &mut TestAppContext,
    ) {
        let h = harness(cx);
        let stale = downloading(cx, &h);
        finish(cx, &h, stale, network_error());
        let retry = ticket(cx, &h);
        finish(cx, &h, stale, network_error());

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
        assert_eq!(ticket(cx, &h), retry);
        assert!(toast_ids(cx).is_empty());
    }

    #[gpui::test]
    fn close_proceeds_while_no_update_is_ready(cx: &mut TestAppContext) {
        let h = harness(cx);
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "idle");
        offer(cx, &h, download_only());
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "offered");
        downloading(cx, &h);
        assert_eq!(close(cx, &h), CloseDecision::Proceed, "downloading");
        for _ in 0..2 {
            let running = ticket(cx, &h);
            finish(cx, &h, running, network_error());
        }
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
    fn restart_while_a_transfer_holds_the_guard_waits_with_the_window_open(
        cx: &mut TestAppContext,
    ) {
        let exclusivity = TransferExclusivity::new();
        let _running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity, |_| None);

        restart_and_close(cx, &w.h);

        assert!(drive_until(cx, &w.h, waiting_to_restart));
        assert_eq!(events(cx, &w.h), vec![UpdateFlowEvent::RestartRequested]);
    }

    #[gpui::test]
    fn the_close_that_meets_a_running_transfer_is_held_while_the_update_waits(
        cx: &mut TestAppContext,
    ) {
        let exclusivity = TransferExclusivity::new();
        let _running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity, |_| None);

        let decision = restart_and_close(cx, &w.h);

        assert_eq!(decision, CloseDecision::Hold);
        assert!(drive_until(cx, &w.h, waiting_to_restart));
    }

    #[gpui::test]
    fn one_more_close_after_a_restart_that_started_the_wait_quits_as_the_toast_promises(
        cx: &mut TestAppContext,
    ) {
        let exclusivity = TransferExclusivity::new();
        let _running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity, |_| None);
        restart_and_close(cx, &w.h);
        assert!(drive_until(cx, &w.h, waiting_to_restart));

        assert_eq!(close(cx, &w.h), CloseDecision::Proceed);
    }

    #[gpui::test]
    fn the_end_of_the_transfer_resumes_the_handoff_holding_the_guard(cx: &mut TestAppContext) {
        let exclusivity = TransferExclusivity::new();
        let running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity.clone(), |_| None);
        restart_and_close(cx, &w.h);

        drop(running);

        assert!(drive_until(cx, &w.h, handing_off));
        assert!(exclusivity.is_held());
    }

    #[gpui::test]
    fn a_handoff_failing_after_the_wait_is_retryable_and_frees_the_guard(cx: &mut TestAppContext) {
        let exclusivity = TransferExclusivity::new();
        let running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity.clone(), |_| None);
        restart_and_close(cx, &w.h);
        drop(running);
        assert!(drive_until(cx, &w.h, handing_off));

        w.handoff_export.notify_one();

        assert!(drive_until(cx, &w.h, handoff_failed));
        assert!(!exclusivity.is_held());
    }

    #[gpui::test]
    fn a_scheduled_backup_running_at_restart_stays_suspended_through_the_handoff(
        cx: &mut TestAppContext,
    ) {
        let w = waiting_harness(cx, TransferExclusivity::new(), |shared| {
            Some(gated_running_schedule_sharing(shared))
        });
        let schedule = w.schedule.as_ref().unwrap();
        assert!(settle_schedule(&w.h, schedule, ScheduleState::Running));
        restart_and_close(cx, &w.h);

        schedule.export_gate.notify_one();

        assert!(drive_until(cx, &w.h, handing_off));
        assert!(settle_schedule(&w.h, schedule, ScheduleState::Suspended));
    }

    #[gpui::test]
    fn a_scheduled_backup_suspended_for_the_update_resumes_when_the_handoff_fails(
        cx: &mut TestAppContext,
    ) {
        let w = waiting_harness(cx, TransferExclusivity::new(), |shared| {
            Some(gated_running_schedule_sharing(shared))
        });
        let schedule = w.schedule.as_ref().unwrap();
        assert!(settle_schedule(&w.h, schedule, ScheduleState::Running));
        restart_and_close(cx, &w.h);
        schedule.export_gate.notify_one();
        assert!(drive_until(cx, &w.h, handing_off));

        w.handoff_export.notify_one();

        assert!(drive_until(cx, &w.h, handoff_failed));
        assert!(settle_schedule(&w.h, schedule, ScheduleState::Idle));
    }

    #[gpui::test]
    fn waiting_on_another_transfer_leaves_an_idle_schedule_unsuspended(cx: &mut TestAppContext) {
        let exclusivity = TransferExclusivity::new();
        let _running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity, |_| Some(keyless_enabled_schedule()));
        let schedule = w.schedule.as_ref().unwrap();
        assert!(settle_schedule(&w.h, schedule, ScheduleState::Idle));

        restart_and_close(cx, &w.h);

        assert!(drive_until(cx, &w.h, waiting_to_restart));
        assert_eq!(schedule.handle.status().borrow().state, ScheduleState::Idle);
    }

    #[gpui::test]
    fn one_more_close_after_the_close_that_started_the_wait_quits_as_the_toast_promises(
        cx: &mut TestAppContext,
    ) {
        let exclusivity = TransferExclusivity::new();
        let _running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity, |_| None);
        assert!(close_into_the_wait(cx, &w.h));

        assert_eq!(close(cx, &w.h), CloseDecision::Proceed);
    }

    #[gpui::test]
    fn quitting_out_of_the_wait_keeps_the_staged_update_and_starts_no_install(
        cx: &mut TestAppContext,
    ) {
        let exclusivity = TransferExclusivity::new();
        let running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity.clone(), |_| None);
        assert!(close_into_the_wait(cx, &w.h));

        let decision = close(cx, &w.h);
        drop(running);
        drive(cx, &w.h);

        assert_eq!(decision, CloseDecision::Proceed);
        assert!(ready_with_the_staged_update(&stage(cx, &w.h)));
        assert!(!exclusivity.is_held());
        assert!(events(cx, &w.h).is_empty());
    }

    #[gpui::test]
    fn a_guard_freed_just_before_the_quitting_close_is_handed_back_unused(cx: &mut TestAppContext) {
        let exclusivity = TransferExclusivity::new();
        let running = exclusivity.try_acquire().unwrap();
        let w = waiting_harness(cx, exclusivity.clone(), |_| None);
        assert!(close_into_the_wait(cx, &w.h));
        drop(running);
        pump(&w.h.rt);
        let taken_by_the_waiter = exclusivity.is_held();

        let decision = close(cx, &w.h);
        drive(cx, &w.h);

        assert_eq!(
            (taken_by_the_waiter, decision),
            (true, CloseDecision::Proceed)
        );
        assert!(ready_with_the_staged_update(&stage(cx, &w.h)));
        assert!(!exclusivity.is_held());
    }

    #[gpui::test]
    fn quitting_out_of_the_wait_resumes_the_scheduled_backup_the_update_suspended(
        cx: &mut TestAppContext,
    ) {
        let w = waiting_harness(cx, TransferExclusivity::new(), |shared| {
            Some(gated_running_schedule_sharing(shared))
        });
        let schedule = w.schedule.as_ref().unwrap();
        assert!(settle_schedule(&w.h, schedule, ScheduleState::Running));
        assert!(close_into_the_wait(cx, &w.h));
        close(cx, &w.h);

        schedule.export_gate.notify_one();
        drive(cx, &w.h);

        assert!(settle_schedule(&w.h, schedule, ScheduleState::Idle));
        assert!(ready_with_the_staged_update(&stage(cx, &w.h)));
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
        let id = toast_ids(cx)[0];
        cx.update(|cx| cx.global_mut::<Toasts>().take(id));
        run(cx, &h, UpdateCommand::Restart);
        h.flow.update(cx, |flow, cx| flow.abort_restart(cx));

        assert_eq!(
            toast_actions(cx),
            vec![Some("Restart to update".to_owned())]
        );
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
    fn a_lost_download_task_is_retried_once_then_fails_the_download(cx: &mut TestAppContext) {
        let h = harness(cx);
        let first = downloading(cx, &h);
        lose(cx, &h, first);
        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));

        let second = ticket(cx, &h);
        lose(cx, &h, second);
        assert!(is_download_failure(&stage(cx, &h)));
    }

    #[gpui::test]
    fn a_lost_task_from_a_superseded_download_leaves_the_retry_running(cx: &mut TestAppContext) {
        let h = harness(cx);
        let stale = downloading(cx, &h);
        finish(cx, &h, stale, network_error());
        lose(cx, &h, stale);

        assert!(matches!(stage(cx, &h), UpdateStage::Downloading(_)));
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
