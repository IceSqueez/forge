use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::error::TransferError;

const NAMESPACE: &str = "transfer";

const ITEM_ARG: &str = "item";
const OVERLAY_ARG: &str = "overlay";
const ACTION_ARG: &str = "action";
const PATH_ARG: &str = "path";

const EMPTY_PACK: MessageCode = MessageCode::from_static(NAMESPACE, "empty_pack");
const PACK_NAME_REQUIRED: MessageCode = MessageCode::from_static(NAMESPACE, "pack_name_required");
const UNKNOWN_PACK_ITEM: MessageCode = MessageCode::from_static(NAMESPACE, "unknown_pack_item");
const PACK_RESOLUTION_NOT_ALLOWED: MessageCode =
    MessageCode::from_static(NAMESPACE, "pack_resolution_not_allowed");
const PACK_PLAN_STALE: MessageCode = MessageCode::from_static(NAMESPACE, "pack_plan_stale");
const INVALID_OVERLAY_CONFIG: MessageCode =
    MessageCode::from_static(NAMESPACE, "invalid_overlay_config");
const PACK_ACTION_WITHOUT_QUEUE: MessageCode =
    MessageCode::from_static(NAMESPACE, "pack_action_without_queue");
const DATA_CHANGED_DURING_BACKUP: MessageCode =
    MessageCode::from_static(NAMESPACE, "data_changed_during_backup");
const UNSUPPORTED_FILE_NAME: MessageCode =
    MessageCode::from_static(NAMESPACE, "unsupported_file_name");
const ENCRYPTION_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "encryption_failed");
const INTERRUPTED: MessageCode = MessageCode::from_static(NAMESPACE, "interrupted");
const FOREIGN_STAGING: MessageCode = MessageCode::from_static(NAMESPACE, "foreign_staging");
const MALFORMED_RECORD: MessageCode = MessageCode::from_static(NAMESPACE, "malformed_record");
const RESTORE_UNSETTLED: MessageCode = MessageCode::from_static(NAMESPACE, "restore_unsettled");
const RESTORE_BLOCKED: MessageCode = MessageCode::from_static(NAMESPACE, "restore_blocked");
const OPERATION_FAILED: MessageCode = MessageCode::from_static(NAMESPACE, "operation_failed");

pub const TRANSFER_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        EMPTY_PACK,
        PACK_NAME_REQUIRED,
        UNKNOWN_PACK_ITEM,
        PACK_RESOLUTION_NOT_ALLOWED,
        PACK_PLAN_STALE,
        INVALID_OVERLAY_CONFIG,
        PACK_ACTION_WITHOUT_QUEUE,
        DATA_CHANGED_DURING_BACKUP,
        UNSUPPORTED_FILE_NAME,
        ENCRYPTION_FAILED,
        INTERRUPTED,
        FOREIGN_STAGING,
        MALFORMED_RECORD,
        RESTORE_UNSETTLED,
        RESTORE_BLOCKED,
        OPERATION_FAILED,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};

fn named(code: MessageCode, arg: &'static str, value: &str) -> MessageRef {
    MessageRef::new(code).with_arg(arg, VerbatimText::new(value))
}

fn detailed(code: MessageCode, detail: impl ToString) -> MessageRef {
    MessageRef::new(code).with_detail(UpstreamDetail::from_redacted(detail.to_string()))
}

impl TransferError {
    pub fn user_message(&self) -> MessageRef {
        match self {
            Self::EmptyPack => MessageRef::new(EMPTY_PACK),
            Self::PackNameRequired => MessageRef::new(PACK_NAME_REQUIRED),
            Self::UnknownPackItem { item } => named(UNKNOWN_PACK_ITEM, ITEM_ARG, item),
            Self::PackResolutionNotAllowed { item } => {
                named(PACK_RESOLUTION_NOT_ALLOWED, ITEM_ARG, item)
            }
            Self::PackPlanStale => MessageRef::new(PACK_PLAN_STALE),
            Self::InvalidOverlayConfig { overlay, source } => {
                named(INVALID_OVERLAY_CONFIG, OVERLAY_ARG, overlay)
                    .with_detail(UpstreamDetail::from_redacted(source.to_string()))
            }
            Self::PackActionWithoutQueue { action } => {
                named(PACK_ACTION_WITHOUT_QUEUE, ACTION_ARG, action)
            }
            Self::DataChangedDuringBackup { path } => {
                named(DATA_CHANGED_DURING_BACKUP, PATH_ARG, path)
            }
            Self::UnsupportedFileName { path } => named(UNSUPPORTED_FILE_NAME, PATH_ARG, path),
            Self::Encryption { reason } => detailed(ENCRYPTION_FAILED, reason),
            Self::Interrupted { reason } => detailed(INTERRUPTED, reason),
            Self::ForeignStaging => MessageRef::new(FOREIGN_STAGING),
            Self::MalformedRecord { reason } => detailed(MALFORMED_RECORD, reason),
            Self::RestoreUnsettled => MessageRef::new(RESTORE_UNSETTLED),
            Self::RestoreBlocked { path } => named(RESTORE_BLOCKED, PATH_ARG, path),
            other => detailed(OPERATION_FAILED, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use forge_overlay::OverlayError;
    use forge_types::MessageArg;

    use super::*;

    const UPSTREAM: &str = "upstream-cause-text";

    type Case = (
        TransferError,
        &'static str,
        Option<(&'static str, &'static str)>,
        Option<&'static str>,
    );

    fn cases() -> Vec<Case> {
        vec![
            (
                TransferError::EmptyPack,
                "msg-transfer-empty_pack",
                None,
                None,
            ),
            (
                TransferError::PackNameRequired,
                "msg-transfer-pack_name_required",
                None,
                None,
            ),
            (
                TransferError::UnknownPackItem { item: "a".into() },
                "msg-transfer-unknown_pack_item",
                Some(("item", "a")),
                None,
            ),
            (
                TransferError::PackResolutionNotAllowed { item: "b".into() },
                "msg-transfer-pack_resolution_not_allowed",
                Some(("item", "b")),
                None,
            ),
            (
                TransferError::PackPlanStale,
                "msg-transfer-pack_plan_stale",
                None,
                None,
            ),
            (
                TransferError::InvalidOverlayConfig {
                    overlay: "alerts".into(),
                    source: OverlayError::UnknownKind(UPSTREAM.into()),
                },
                "msg-transfer-invalid_overlay_config",
                Some(("overlay", "alerts")),
                Some("unknown overlay type: upstream-cause-text"),
            ),
            (
                TransferError::PackActionWithoutQueue {
                    action: "act".into(),
                },
                "msg-transfer-pack_action_without_queue",
                Some(("action", "act")),
                None,
            ),
            (
                TransferError::DataChangedDuringBackup { path: "p1".into() },
                "msg-transfer-data_changed_during_backup",
                Some(("path", "p1")),
                None,
            ),
            (
                TransferError::UnsupportedFileName { path: "p2".into() },
                "msg-transfer-unsupported_file_name",
                Some(("path", "p2")),
                None,
            ),
            (
                TransferError::Encryption {
                    reason: UPSTREAM.into(),
                },
                "msg-transfer-encryption_failed",
                None,
                Some(UPSTREAM),
            ),
            (
                TransferError::Interrupted {
                    reason: UPSTREAM.into(),
                },
                "msg-transfer-interrupted",
                None,
                Some(UPSTREAM),
            ),
            (
                TransferError::ForeignStaging,
                "msg-transfer-foreign_staging",
                None,
                None,
            ),
            (
                TransferError::MalformedRecord {
                    reason: UPSTREAM.into(),
                },
                "msg-transfer-malformed_record",
                None,
                Some(UPSTREAM),
            ),
            (
                TransferError::RestoreUnsettled,
                "msg-transfer-restore_unsettled",
                None,
                None,
            ),
            (
                TransferError::RestoreBlocked { path: "p3".into() },
                "msg-transfer-restore_blocked",
                Some(("path", "p3")),
                None,
            ),
            (
                TransferError::Io(std::io::Error::other(UPSTREAM)),
                "msg-transfer-operation_failed",
                None,
                Some("io error: upstream-cause-text"),
            ),
        ]
    }

    #[test]
    fn each_transfer_error_maps_to_its_own_code_argument_and_detail() {
        for (error, key, arg, detail) in cases() {
            let message = error.user_message();

            assert_eq!(message.code().ftl_key(), key, "{error:?}");
            match arg {
                Some((name, value)) => {
                    assert_eq!(
                        message.arg(name),
                        Some(&MessageArg::Verbatim(VerbatimText::new(value))),
                        "{error:?}"
                    );
                    assert_eq!(message.args().count(), 1, "{error:?}");
                }
                None => assert_eq!(message.args().count(), 0, "{error:?}"),
            }
            assert_eq!(message.detail().map(|d| d.as_str()), detail, "{error:?}");
        }
    }

    #[test]
    fn an_unclassified_error_falls_back_to_operation_failed_carrying_its_description() {
        let error = TransferError::PackRollbackIncomplete {
            cause: "cause".into(),
            failure: "failure".into(),
            snapshot: Some(PathBuf::from("/data/backups/safety.tar.gz")),
        };

        let message = error.user_message();

        assert_eq!(message.code().ftl_key(), "msg-transfer-operation_failed");
        assert_eq!(
            message.detail().map(|d| d.as_str().to_owned()),
            Some(error.to_string())
        );
    }

    #[test]
    fn every_catalogued_code_is_produced_by_some_transfer_error() {
        let produced: BTreeSet<String> = cases()
            .iter()
            .map(|(error, ..)| error.user_message().code().ftl_key())
            .collect();
        let catalogued: BTreeSet<String> = TRANSFER_MESSAGES.ftl_keys().collect();

        assert_eq!(produced, catalogued);
    }
}
