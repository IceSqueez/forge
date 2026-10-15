use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{Icon, ToastAction, ToastKind, tr};
use forge_platform_core::paths;
use forge_storage::DataProvider;
use forge_update::{
    ApplyFailure, DatabaseRestore, RollbackReason, UpdateError, UpdateReport, take_update_report,
    write_health_marker,
};
use gpui::{App, AsyncApp};
use tokio::runtime::Handle;

use crate::async_bridge::{detached, open_path};
use crate::toasts::PushToast;
use crate::update_check::{CURRENT_VERSION, REPO_URL, release_page_url, remember_skipped};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportOffer {
    Nothing,
    Download { version: String },
    SkipOrDownload { version: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportNotice {
    pub kind: ToastKind,
    pub message: String,
    pub offer: ReportOffer,
}

pub fn report_notice(report: &UpdateReport) -> ReportNotice {
    match report {
        UpdateReport::Updated { version, .. } => ReportNotice {
            kind: ToastKind::Info,
            message: tr!("update_report_updated", version = version.as_str()),
            offer: ReportOffer::Nothing,
        },
        UpdateReport::RolledBack {
            failed_version,
            restored_version,
            reason,
            database,
        } => {
            let key = match database {
                DatabaseRestore::Unchanged => "update_report_rolled_back",
                DatabaseRestore::Restored => "update_report_rolled_back_data_restored",
                DatabaseRestore::Failed { .. } => "update_report_rolled_back_data_failed",
            };
            ReportNotice {
                kind: ToastKind::Warn,
                message: tr!(
                    key,
                    failed = failed_version.as_str(),
                    restored = restored_version.as_str(),
                    reason = rollback_reason(reason),
                ),
                offer: ReportOffer::SkipOrDownload {
                    version: failed_version.clone(),
                },
            }
        }
        UpdateReport::RollbackFailed {
            failed_version,
            reason,
            ..
        } => ReportNotice {
            kind: ToastKind::Error,
            message: tr!(
                "update_report_rollback_failed",
                failed = failed_version.as_str(),
                reason = rollback_reason(reason),
            ),
            offer: ReportOffer::Download {
                version: failed_version.clone(),
            },
        },
        UpdateReport::NotApplied { version, reason } => ReportNotice {
            kind: ToastKind::Warn,
            message: tr!(
                "update_report_not_applied",
                version = version.as_str(),
                reason = apply_failure(reason),
            ),
            offer: ReportOffer::Nothing,
        },
    }
}

fn rollback_reason(reason: &RollbackReason) -> String {
    match reason {
        RollbackReason::Crashed { .. } => tr!("update_reason_crashed"),
        RollbackReason::NoHealthSignal => tr!("update_reason_no_health_signal"),
        RollbackReason::LaunchFailed { .. } => tr!("update_reason_launch_failed"),
    }
}

fn apply_failure(reason: &ApplyFailure) -> String {
    match reason {
        ApplyFailure::ParentStillRunning => tr!("update_reason_parent_still_running"),
        ApplyFailure::InstallerFailed { .. } => tr!("update_reason_installer_failed"),
        ApplyFailure::InstallerTimedOut => tr!("update_reason_installer_timed_out"),
        ApplyFailure::MalformedPayload => tr!("update_reason_malformed_payload"),
        ApplyFailure::FileOperation { .. } => tr!("update_reason_file_operation"),
        ApplyFailure::Interrupted => tr!("update_reason_interrupted"),
    }
}

struct BootUpdateState {
    report: Result<Option<UpdateReport>, UpdateError>,
    health: Result<bool, UpdateError>,
}

fn settle_boot_update(data_dir: PathBuf) -> BootUpdateState {
    let report = take_update_report(&data_dir, CURRENT_VERSION);
    let health = write_health_marker(&data_dir, CURRENT_VERSION);
    BootUpdateState { report, health }
}

pub fn start_update_report(cx: &mut AsyncApp, backend: Arc<dyn DataProvider>, rt_handle: Handle) {
    let data_dir = paths::data_dir();
    let settle = cx
        .background_executor()
        .spawn(async move { settle_boot_update(data_dir) });
    cx.spawn(async move |cx| {
        let state = settle.await;
        match state.health {
            Ok(true) => tracing::info!("update health marker written"),
            Ok(false) => {}
            Err(error) => tracing::warn!(error = %error, "update health marker not written"),
        }
        match state.report {
            Ok(Some(report)) => {
                tracing::info!(?report, "update outcome reported");
                cx.update(|cx| raise_report_toast(&report, backend, rt_handle, cx));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(error = %error, "update outcome could not be read"),
        }
    })
    .detach();
}

fn raise_report_toast(
    report: &UpdateReport,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    cx: &mut App,
) {
    let notice = report_notice(report);
    match notice.offer {
        ReportOffer::Nothing => cx.push_toast_full(
            notice.kind,
            notice.message,
            Some(Icon::Sparkles),
            None,
            Duration::ZERO,
        ),
        ReportOffer::Download { version } => cx.push_toast_full(
            notice.kind,
            notice.message,
            None,
            Some(download_action(&version, rt_handle)),
            Duration::ZERO,
        ),
        ReportOffer::SkipOrDownload { version } => {
            let download = download_action(&version, rt_handle.clone());
            let skip = ToastAction::new(
                tr!("update_toast_skip_action"),
                move |_window, _app: &mut App| {
                    remember_skipped(version.clone(), Arc::clone(&backend), &rt_handle);
                },
            );
            cx.push_toast_with_secondary(
                notice.kind,
                notice.message,
                None,
                download,
                skip,
                Duration::ZERO,
            );
        }
    }
}

fn download_action(version: &str, rt_handle: Handle) -> ToastAction {
    let page = release_page_url(REPO_URL, version);
    ToastAction::new(
        tr!("update_toast_open_action"),
        move |_window, _app: &mut App| {
            if let Some(page) = page.clone() {
                detached(&rt_handle, "open the release page", open_path(page));
            }
        },
    )
}
