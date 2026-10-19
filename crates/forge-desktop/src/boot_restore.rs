use std::path::Path;
use std::time::Duration;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use forge_transfer::{
    RestoreDeferral, RestoreOutcome, RestoreRollback, TransferError, apply_pending_restore,
    restore_in_flight, restore_pending,
};
use gpui::{App, Global};

const STICKY: Duration = Duration::ZERO;

pub(crate) struct BootRestore(pub RestoreOutcome);

impl Global for BootRestore {}

pub(crate) fn raise_boot_restore_toast(cx: &mut App) {
    if cx.try_global::<BootRestore>().is_none() {
        return;
    }
    let BootRestore(outcome) = cx.remove_global::<BootRestore>();
    match outcome {
        RestoreOutcome::NonePending => {}
        RestoreOutcome::Applied {
            pre_restore_snapshot,
        } => {
            let shown = pre_restore_snapshot.display().to_string();
            cx.push_toast_full(
                ToastKind::Success,
                tr!("restore_boot_applied", path = shown.as_str()),
                None,
                Some(ToastAction::new(
                    tr!("settings_storage_backup_reveal"),
                    move |_window, app| app.reveal_path(&pre_restore_snapshot),
                )),
                STICKY,
            );
        }
        RestoreOutcome::RolledBack { reason } => cx.push_toast_full(
            ToastKind::Error,
            tr!(
                "restore_boot_rolled_back",
                reason = rollback_reason(&reason)
            ),
            None,
            None,
            STICKY,
        ),
        RestoreOutcome::Deferred { reason } => cx.push_toast_full(
            ToastKind::Warn,
            tr!("restore_boot_deferred", reason = deferral_reason(reason)),
            None,
            None,
            STICKY,
        ),
    }
}

pub(crate) fn rollback_reason(reason: &RestoreRollback) -> String {
    match reason {
        RestoreRollback::Interrupted => tr!("restore_rollback_interrupted"),
        RestoreRollback::StagingMissing => tr!("restore_rollback_staging_missing"),
        RestoreRollback::MarkerUnreadable => tr!("restore_rollback_marker_unreadable"),
        RestoreRollback::Failed { error } => error.clone(),
    }
}

pub(crate) fn deferral_reason(reason: RestoreDeferral) -> String {
    match reason {
        RestoreDeferral::UpdateInProgress => tr!("restore_deferral_update"),
        RestoreDeferral::InstanceLockUnavailable => tr!("restore_deferral_instance_lock"),
    }
}

pub(crate) fn settle_pending_restore(
    data_dir: &Path,
    lock_acquired: bool,
) -> Result<RestoreOutcome, TransferError> {
    let outcome = if lock_acquired {
        apply_pending_restore(data_dir)?
    } else if restore_in_flight(data_dir)? {
        return Err(TransferError::RestoreUnsettled);
    } else if restore_pending(data_dir)? {
        RestoreOutcome::Deferred {
            reason: RestoreDeferral::InstanceLockUnavailable,
        }
    } else {
        RestoreOutcome::NonePending
    };
    match &outcome {
        RestoreOutcome::NonePending => {}
        RestoreOutcome::Applied {
            pre_restore_snapshot,
        } => tracing::info!(
            snapshot = %pre_restore_snapshot.display(),
            "pending restore applied"
        ),
        RestoreOutcome::RolledBack { reason } => {
            tracing::warn!(?reason, "pending restore rolled back")
        }
        RestoreOutcome::Deferred { reason } => {
            tracing::warn!(?reason, "restore is still pending and was not applied")
        }
    }
    Ok(outcome)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_components::{ToastKind, Toasts};
    use forge_storage::Language;
    use forge_transfer::{RESTORE_MARKER_FILE, RestoreRollback, restore_staging_root};
    use gpui::TestAppContext;
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::i18n::install_language;

    const PAST_ANY_TIMED_TOAST: Duration = Duration::from_secs(60);

    fn toasts_after_boot(
        cx: &mut TestAppContext,
        outcome: RestoreOutcome,
    ) -> Vec<(ToastKind, String)> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Toasts::new());
            cx.set_global(BootRestore(outcome));
            raise_boot_restore_toast(cx);
        });
        cx.executor().advance_clock(PAST_ANY_TIMED_TOAST);
        cx.run_until_parked();
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| (toast.kind, toast.message.to_string()))
                .collect()
        })
    }

    fn kinds(toasts: &[(ToastKind, String)]) -> Vec<ToastKind> {
        toasts.iter().map(|(kind, _)| *kind).collect()
    }

    fn data_dir_with_marker(marker: &str) -> TempDir {
        let data = TempDir::new().unwrap();
        std::fs::write(data.path().join(RESTORE_MARKER_FILE), marker).unwrap();
        std::fs::create_dir_all(restore_staging_root(data.path()).join("staged")).unwrap();
        data
    }

    #[test]
    fn unavailable_lock_defers_a_pending_restore_and_keeps_its_marker() {
        let data = data_dir_with_marker(r#"{"format":1,"staging":"staged"}"#);

        let outcome = settle_pending_restore(data.path(), false).unwrap();

        assert_eq!(
            (outcome, data.path().join(RESTORE_MARKER_FILE).is_file()),
            (
                RestoreOutcome::Deferred {
                    reason: RestoreDeferral::InstanceLockUnavailable
                },
                true
            )
        );
    }

    #[test]
    fn unavailable_lock_without_a_marker_reports_nothing_pending() {
        let data = TempDir::new().unwrap();

        let outcome = settle_pending_restore(data.path(), false).unwrap();

        assert_eq!(outcome, RestoreOutcome::NonePending);
    }

    #[test]
    fn acquired_lock_settles_the_pending_restore() {
        let data = data_dir_with_marker("not json");

        let outcome = settle_pending_restore(data.path(), true).unwrap();

        assert_eq!(
            outcome,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::MarkerUnreadable
            }
        );
    }

    #[gpui::test]
    fn each_settled_restore_raises_one_sticky_toast_of_its_severity(cx: &mut TestAppContext) {
        for (outcome, expected) in [
            (RestoreOutcome::NonePending, vec![]),
            (
                RestoreOutcome::Applied {
                    pre_restore_snapshot: PathBuf::from("/data/backups/pre-restore"),
                },
                vec![ToastKind::Success],
            ),
            (
                RestoreOutcome::RolledBack {
                    reason: RestoreRollback::Interrupted,
                },
                vec![ToastKind::Error],
            ),
            (
                RestoreOutcome::Deferred {
                    reason: RestoreDeferral::UpdateInProgress,
                },
                vec![ToastKind::Warn],
            ),
        ] {
            let label = format!("{outcome:?}");
            assert_eq!(kinds(&toasts_after_boot(cx, outcome)), expected, "{label}");
        }
    }

    #[gpui::test]
    fn the_applied_toast_names_the_pre_restore_snapshot(cx: &mut TestAppContext) {
        let toasts = toasts_after_boot(
            cx,
            RestoreOutcome::Applied {
                pre_restore_snapshot: PathBuf::from("/data/backups/forge-pre-restore-xyz"),
            },
        );

        assert!(toasts[0].1.contains("forge-pre-restore-xyz"), "{toasts:?}");
    }

    #[gpui::test]
    fn a_failed_rollback_shows_the_underlying_error(cx: &mut TestAppContext) {
        let toasts = toasts_after_boot(
            cx,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::Failed {
                    error: "disk full while swapping".to_owned(),
                },
            },
        );

        assert!(
            toasts[0].1.contains("disk full while swapping"),
            "{toasts:?}"
        );
    }

    #[gpui::test]
    fn the_boot_outcome_is_reported_only_once(cx: &mut TestAppContext) {
        toasts_after_boot(
            cx,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::StagingMissing,
            },
        );

        cx.update(raise_boot_restore_toast);

        assert_eq!(cx.update(|cx| cx.global::<Toasts>().items().len()), 1);
    }
}
