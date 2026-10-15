use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{Icon, ToastAction, ToastKind, tr};
use forge_platform_core::paths;
use forge_storage::DataProvider;
use forge_update::{
    ApplyFailure, DatabaseRestore, GithubRepo, RollbackReason, UpdateError, UpdateReport,
    take_update_report, write_health_marker,
};
use gpui::{App, AsyncApp};
use tokio::runtime::Handle;

use crate::async_bridge::{detached, open_path};
use crate::update_check::{CURRENT_VERSION, REPO_URL, remember_skipped};
use forge_components::PushToast;

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
    let page = GithubRepo::parse(REPO_URL).and_then(|repo| repo.release_page_url(version));
    ToastAction::new(
        tr!("update_toast_open_action"),
        move |_window, _app: &mut App| {
            if let Some(page) = page.clone() {
                detached(&rt_handle, "open the release page", open_path(page));
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forge_components::ToastKind;
    use forge_storage::Language;
    use forge_update::{ApplyFailure, DatabaseRestore, RollbackReason, UpdateReport};

    use super::{ReportOffer, report_notice};
    use crate::i18n::install_language;

    const FAILED: &str = "0.7.1";
    const RESTORED: &str = "0.7.0";
    const LOCALES: [Language; 2] = [Language::En, Language::Uk];

    fn rollback_reasons() -> [RollbackReason; 3] {
        [
            RollbackReason::Crashed { exit_code: Some(9) },
            RollbackReason::NoHealthSignal,
            RollbackReason::LaunchFailed {
                detail: "exec format error".to_owned(),
            },
        ]
    }

    fn database_outcomes() -> [DatabaseRestore; 3] {
        [
            DatabaseRestore::Unchanged,
            DatabaseRestore::Restored,
            DatabaseRestore::Failed {
                detail: "disk full".to_owned(),
            },
        ]
    }

    fn apply_failures() -> [ApplyFailure; 6] {
        [
            ApplyFailure::ParentStillRunning,
            ApplyFailure::InstallerFailed { exit_code: Some(1) },
            ApplyFailure::InstallerTimedOut,
            ApplyFailure::MalformedPayload,
            ApplyFailure::FileOperation {
                detail: "access denied".to_owned(),
            },
            ApplyFailure::Interrupted,
        ]
    }

    fn updated() -> UpdateReport {
        UpdateReport::Updated {
            previous_version: RESTORED.to_owned(),
            version: FAILED.to_owned(),
        }
    }

    fn rolled_back(reason: RollbackReason, database: DatabaseRestore) -> UpdateReport {
        UpdateReport::RolledBack {
            failed_version: FAILED.to_owned(),
            restored_version: RESTORED.to_owned(),
            reason,
            database,
        }
    }

    fn rollback_failed(reason: RollbackReason) -> UpdateReport {
        UpdateReport::RollbackFailed {
            failed_version: FAILED.to_owned(),
            reason,
            detail: "rename failed".to_owned(),
        }
    }

    fn not_applied(reason: ApplyFailure) -> UpdateReport {
        UpdateReport::NotApplied {
            version: FAILED.to_owned(),
            reason,
        }
    }

    fn every_rolled_back() -> Vec<UpdateReport> {
        rollback_reasons()
            .into_iter()
            .flat_map(|reason| {
                database_outcomes()
                    .into_iter()
                    .map(move |database| rolled_back(reason.clone(), database))
            })
            .collect()
    }

    fn every_rollback_failed() -> Vec<UpdateReport> {
        rollback_reasons()
            .into_iter()
            .map(rollback_failed)
            .collect()
    }

    fn every_not_applied() -> Vec<UpdateReport> {
        apply_failures().into_iter().map(not_applied).collect()
    }

    fn every_report() -> Vec<UpdateReport> {
        let mut reports = vec![updated()];
        reports.extend(every_rolled_back());
        reports.extend(every_rollback_failed());
        reports.extend(every_not_applied());
        reports
    }

    fn versions_named_by(report: &UpdateReport) -> Vec<&str> {
        match report {
            UpdateReport::Updated { .. } | UpdateReport::NotApplied { .. } => vec![FAILED],
            UpdateReport::RolledBack { .. } => vec![FAILED, RESTORED],
            UpdateReport::RollbackFailed { .. } => vec![FAILED],
        }
    }

    fn distinct_messages(reports: &[UpdateReport]) -> usize {
        reports
            .iter()
            .map(|report| report_notice(report).message)
            .collect::<BTreeSet<_>>()
            .len()
    }

    #[test]
    fn severity_is_info_for_success_warn_for_a_kept_old_version_and_error_when_undo_failed() {
        for (reports, expected) in [
            (vec![updated()], ToastKind::Info),
            (every_rolled_back(), ToastKind::Warn),
            (every_not_applied(), ToastKind::Warn),
            (every_rollback_failed(), ToastKind::Error),
        ] {
            for report in reports {
                assert_eq!(report_notice(&report).kind, expected, "for {report:?}");
            }
        }
    }

    #[test]
    fn only_a_failed_new_version_is_offered_and_only_a_rollback_may_skip_it() {
        let failed = || FAILED.to_owned();
        for (reports, expected) in [
            (vec![updated()], ReportOffer::Nothing),
            (every_not_applied(), ReportOffer::Nothing),
            (
                every_rolled_back(),
                ReportOffer::SkipOrDownload { version: failed() },
            ),
            (
                every_rollback_failed(),
                ReportOffer::Download { version: failed() },
            ),
        ] {
            for report in reports {
                assert_eq!(report_notice(&report).offer, expected, "for {report:?}");
            }
        }
    }

    #[test]
    fn every_message_resolves_its_versions_and_reason_in_both_locales() {
        for lang in LOCALES {
            install_language(lang);
            for report in every_report() {
                let message = report_notice(&report).message;
                for leak in ["{$", "update_report_", "update_reason_"] {
                    assert!(
                        !message.contains(leak),
                        "{lang:?} {report:?} rendered unresolved text: {message:?}"
                    );
                }
                for version in versions_named_by(&report) {
                    assert!(
                        message.contains(version),
                        "{lang:?} {report:?} omits {version}: {message:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_rollback_message_tells_apart_untouched_restored_and_unrestored_data() {
        for lang in LOCALES {
            install_language(lang);
            for reason in rollback_reasons() {
                let reports: Vec<UpdateReport> = database_outcomes()
                    .into_iter()
                    .map(|database| rolled_back(reason.clone(), database))
                    .collect();
                assert_eq!(
                    distinct_messages(&reports),
                    reports.len(),
                    "{lang:?} {reason:?}: two data outcomes read the same"
                );
            }
        }
    }

    #[test]
    fn every_failure_reason_reads_differently_in_both_locales() {
        for lang in LOCALES {
            install_language(lang);
            for (label, reports) in [
                (
                    "rolled back",
                    rollback_reasons()
                        .into_iter()
                        .map(|reason| rolled_back(reason, DatabaseRestore::Unchanged))
                        .collect::<Vec<_>>(),
                ),
                ("rollback failed", every_rollback_failed()),
                ("not applied", every_not_applied()),
            ] {
                assert_eq!(
                    distinct_messages(&reports),
                    reports.len(),
                    "{lang:?} {label}: two reasons read the same"
                );
            }
        }
    }
}
