use crate::bundle::CancelFlag;
use crate::error::TransferError;
use crate::full::{ScheduledBackupRequest, ScheduledWrite};

pub const INCLUDED_DIRS: [&str; 4] = crate::bundle::layout::INCLUDED_DIRS;
pub const MAX_DECRYPT_SCRYPT_WORK_FACTOR: u8 =
    crate::bundle::limits::MAX_DECRYPT_SCRYPT_WORK_FACTOR;

pub async fn write_scheduled_backup_cancellable(
    request: ScheduledBackupRequest,
    unchanged_since: Option<String>,
    cancel: CancelFlag,
) -> Result<ScheduledWrite, TransferError> {
    crate::full::write_scheduled_backup_cancellable(request, unchanged_since, cancel).await
}
