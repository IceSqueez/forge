use forge_components::{Icon, ToastAction, ToastData, ToastKind, tr};
use forge_update::ApplyMode;
use gpui::{App, WeakEntity};

use crate::update_flow::{
    UpdateCandidate, UpdateFailure, UpdateFlow, UpdateStage, download_fraction, download_percent,
};

const PREPARING_PROGRESS: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCommand {
    StartDownload,
    OpenReleasePage,
    Skip,
    Cancel,
    Restart,
    Retry,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToastButton {
    pub label: String,
    pub command: UpdateCommand,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateToast {
    pub kind: ToastKind,
    pub message: String,
    pub icon: Option<Icon>,
    pub progress: Option<f32>,
    pub primary: Option<ToastButton>,
    pub secondary: Option<ToastButton>,
}

fn button(label: String, command: UpdateCommand) -> Option<ToastButton> {
    Some(ToastButton { label, command })
}

fn offered(candidate: &UpdateCandidate) -> UpdateToast {
    let primary = if candidate.install.is_some() {
        button(
            tr!("update_toast_update_action"),
            UpdateCommand::StartDownload,
        )
    } else {
        button(
            tr!("update_toast_open_action"),
            UpdateCommand::OpenReleasePage,
        )
    };
    UpdateToast {
        kind: ToastKind::Info,
        message: tr!(
            "update_toast_available",
            version = candidate.version.as_str()
        ),
        icon: Some(Icon::Sparkles),
        progress: None,
        primary,
        secondary: button(tr!("update_toast_skip_action"), UpdateCommand::Skip),
    }
}

fn failure_message(failure: UpdateFailure, version: &str) -> String {
    match failure {
        UpdateFailure::Download => tr!("update_toast_download_failed", version = version),
        UpdateFailure::Unverified => tr!("update_toast_unverified", version = version),
        UpdateFailure::Handoff => tr!("update_toast_handoff_failed", version = version),
    }
}

pub fn update_toast(stage: &UpdateStage) -> Option<UpdateToast> {
    match stage {
        UpdateStage::Idle => None,
        UpdateStage::Offered(candidate) => Some(offered(candidate)),
        UpdateStage::Downloading {
            candidate,
            progress,
            ..
        } => Some(UpdateToast {
            kind: ToastKind::Info,
            message: tr!(
                "update_toast_downloading",
                version = candidate.version.as_str(),
                percent = i64::from(download_percent(*progress)),
            ),
            icon: Some(Icon::Sparkles),
            progress: Some(download_fraction(*progress)),
            primary: None,
            secondary: button(tr!("update_toast_cancel_action"), UpdateCommand::Cancel),
        }),
        UpdateStage::Ready { candidate, .. } => Some(UpdateToast {
            kind: ToastKind::Info,
            message: tr!("update_toast_ready", version = candidate.version.as_str()),
            icon: Some(Icon::Sparkles),
            progress: None,
            primary: button(tr!("update_toast_restart_action"), UpdateCommand::Restart),
            secondary: None,
        }),
        UpdateStage::Failed {
            candidate, failure, ..
        } => Some(UpdateToast {
            kind: ToastKind::Error,
            message: failure_message(*failure, &candidate.version),
            icon: None,
            progress: None,
            primary: button(tr!("update_toast_retry_action"), UpdateCommand::Retry),
            secondary: button(
                tr!("update_toast_open_action"),
                UpdateCommand::OpenReleasePage,
            ),
        }),
        UpdateStage::HandingOff { candidate, .. } => Some(UpdateToast {
            kind: ToastKind::Info,
            message: tr!(
                "update_toast_preparing",
                version = candidate.version.as_str()
            ),
            icon: Some(Icon::Sparkles),
            progress: Some(PREPARING_PROGRESS),
            primary: None,
            secondary: None,
        }),
        UpdateStage::HandedOff { candidate, mode } => {
            (*mode == ApplyMode::Restart).then(|| UpdateToast {
                kind: ToastKind::Info,
                message: tr!(
                    "update_toast_restarting",
                    version = candidate.version.as_str()
                ),
                icon: Some(Icon::Sparkles),
                progress: None,
                primary: None,
                secondary: None,
            })
        }
    }
}

pub fn toast_data(
    toast: UpdateToast,
    flow: WeakEntity<UpdateFlow>,
    run: fn(&WeakEntity<UpdateFlow>, UpdateCommand, &mut App),
) -> ToastData {
    let action = |button: ToastButton| {
        let flow = flow.clone();
        ToastAction::new(button.label, move |_window, cx: &mut App| {
            run(&flow, button.command, cx);
        })
    };
    let mut data = ToastData::new(0, toast.kind, toast.message);
    data.icon = toast.icon;
    data.progress = toast.progress;
    data.action = toast.primary.map(action);
    data.secondary_action = toast.secondary.map(action);
    data
}
