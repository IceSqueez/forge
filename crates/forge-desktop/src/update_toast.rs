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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use forge_storage::Language;
    use forge_update::{
        Artifact, CancelSignal, DownloadProgress, InstallLayout, Sha256Digest, StagedArtifact,
        UpdatableInstall,
    };

    use super::*;
    use crate::i18n::install_language;
    use crate::update_flow::InstallTarget;

    const VERSION: &str = "0.9.0";

    fn candidate(installable: bool) -> UpdateCandidate {
        UpdateCandidate {
            version: VERSION.to_owned(),
            release_page: "https://example.invalid/releases/v0.9.0".to_owned(),
            install: installable.then(|| InstallTarget {
                install: UpdatableInstall {
                    layout: InstallLayout::LinuxAppImage,
                    target: PathBuf::from("/opt/forge/forge.AppImage"),
                },
                artifact: Artifact {
                    platform: "linux-x64".to_owned(),
                    kind: "appimage".to_owned(),
                    name: "forge.AppImage".to_owned(),
                    url: "https://example.invalid/forge.AppImage".to_owned(),
                    sha256: Sha256Digest::of(b"forge"),
                    size: 100,
                },
            }),
        }
    }

    fn staged() -> StagedArtifact {
        StagedArtifact {
            path: PathBuf::from("/opt/forge/staged"),
            size: 100,
        }
    }

    fn commands(toast: &UpdateToast) -> (Option<UpdateCommand>, Option<UpdateCommand>) {
        (
            toast.primary.as_ref().map(|b| b.command),
            toast.secondary.as_ref().map(|b| b.command),
        )
    }

    fn failed(failure: UpdateFailure) -> UpdateStage {
        UpdateStage::Failed {
            candidate: candidate(true),
            failure,
            staged: None,
        }
    }

    #[test]
    fn each_stage_offers_the_buttons_that_move_it_forward() {
        install_language(Language::En);
        for (label, stage, expected) in [
            (
                "installable offer",
                UpdateStage::Offered(candidate(true)),
                (
                    Some(UpdateCommand::StartDownload),
                    Some(UpdateCommand::Skip),
                ),
            ),
            (
                "download-only offer",
                UpdateStage::Offered(candidate(false)),
                (
                    Some(UpdateCommand::OpenReleasePage),
                    Some(UpdateCommand::Skip),
                ),
            ),
            (
                "downloading",
                UpdateStage::Downloading {
                    candidate: candidate(true),
                    progress: DownloadProgress {
                        received: 0,
                        total: 100,
                    },
                    cancel: CancelSignal::new(),
                },
                (None, Some(UpdateCommand::Cancel)),
            ),
            (
                "ready",
                UpdateStage::Ready {
                    candidate: candidate(true),
                    staged: staged(),
                },
                (Some(UpdateCommand::Restart), None),
            ),
            (
                "failed",
                failed(UpdateFailure::Download),
                (
                    Some(UpdateCommand::Retry),
                    Some(UpdateCommand::OpenReleasePage),
                ),
            ),
            (
                "restarting",
                UpdateStage::HandedOff {
                    candidate: candidate(true),
                    mode: ApplyMode::Restart,
                },
                (None, None),
            ),
        ] {
            let toast = update_toast(&stage).unwrap_or_else(|| panic!("{label}: no toast"));
            assert_eq!(commands(&toast), expected, "{label}");
        }
    }

    #[test]
    fn idle_and_a_handed_off_apply_on_quit_update_show_no_toast() {
        install_language(Language::En);
        for stage in [
            UpdateStage::Idle,
            UpdateStage::HandedOff {
                candidate: candidate(true),
                mode: ApplyMode::OnQuit,
            },
        ] {
            assert_eq!(update_toast(&stage), None, "{stage:?}");
        }
    }

    #[test]
    fn a_handoff_in_either_mode_shows_a_full_preparing_bar_without_actions() {
        install_language(Language::En);
        for mode in [ApplyMode::OnQuit, ApplyMode::Restart] {
            let toast = update_toast(&UpdateStage::HandingOff {
                candidate: candidate(true),
                staged: staged(),
                mode,
            })
            .unwrap_or_else(|| panic!("{mode:?}: no toast"));

            assert!(
                toast.message.starts_with("Preparing"),
                "{mode:?}: {}",
                toast.message
            );
            assert!(
                toast.message.contains(VERSION),
                "{mode:?}: {}",
                toast.message
            );
            assert_eq!(toast.progress, Some(1.0), "{mode:?}");
            assert_eq!(commands(&toast), (None, None), "{mode:?}");
        }
    }

    #[test]
    fn the_downloading_toast_shows_the_percent_and_a_matching_bar() {
        install_language(Language::En);
        let toast = update_toast(&UpdateStage::Downloading {
            candidate: candidate(true),
            progress: DownloadProgress {
                received: 42,
                total: 100,
            },
            cancel: CancelSignal::new(),
        })
        .expect("downloading toast");

        assert!(toast.message.contains("42"), "{}", toast.message);
        assert_eq!(toast.progress, Some(0.42));
    }

    #[test]
    fn every_failure_kind_is_an_error_toast_with_its_own_wording() {
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            let toasts: Vec<UpdateToast> = [
                UpdateFailure::Download,
                UpdateFailure::Unverified,
                UpdateFailure::Handoff,
            ]
            .into_iter()
            .map(|failure| update_toast(&failed(failure)).expect("failure toast"))
            .collect();

            assert!(
                toasts.iter().all(|t| t.kind == ToastKind::Error),
                "{lang:?}"
            );
            assert!(
                toasts.iter().all(|t| t.message.contains(VERSION)),
                "{lang:?}"
            );
            let distinct: std::collections::BTreeSet<&str> =
                toasts.iter().map(|t| t.message.as_str()).collect();
            assert_eq!(distinct.len(), toasts.len(), "{lang:?}");
        }
    }
}
