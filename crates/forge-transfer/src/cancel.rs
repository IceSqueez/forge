use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::TransferError;

#[derive(Debug, Clone, Default)]
pub struct CancelFlag {
    raised: Arc<AtomicBool>,
}

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn raise(&self) {
        self.raised.store(true, Ordering::Release);
    }

    pub(crate) fn checkpoint(&self) -> Result<(), TransferError> {
        if self.raised.load(Ordering::Acquire) {
            Err(TransferError::Interrupted {
                reason: "the backup was cancelled".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}
