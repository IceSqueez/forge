use std::sync::{Arc, OnceLock};

use forge_platform_core::PlatformError;
use tokio_util::sync::CancellationToken;

use crate::broadcast_lookup::BroadcastLookup;

#[derive(Debug, Clone, Default)]
pub struct ActiveBroadcastIdHandle {
    inner: Arc<std::sync::Mutex<Option<String>>>,
    fallback: Arc<OnceLock<BroadcastLookup>>,
}

impl ActiveBroadcastIdHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, id: Option<String>) {
        *self.inner.lock().unwrap_or_else(|p| p.into_inner()) = id;
    }

    pub fn set_while_active(&self, id: Option<String>, cancel: &CancellationToken) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if !cancel.is_cancelled() {
            *guard = id;
        }
    }

    pub fn get(&self) -> Option<String> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub(crate) fn fall_back_to(&self, lookup: BroadcastLookup) {
        self.fallback.get_or_init(|| lookup);
    }

    pub(crate) async fn resolve(&self) -> Result<Option<String>, PlatformError> {
        if let Some(id) = self.get() {
            return Ok(Some(id));
        }
        let Some(lookup) = self.fallback.get() else {
            return Ok(None);
        };
        Ok(lookup
            .active()
            .await?
            .map(|broadcast| broadcast.broadcast_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_while_active_publishes_until_the_token_is_cancelled_then_changes_nothing() {
        let handle = ActiveBroadcastIdHandle::new();
        let cancel = CancellationToken::new();

        handle.set_while_active(Some("live".to_owned()), &cancel);
        let published = handle.get();
        cancel.cancel();
        handle.set_while_active(None, &cancel);
        handle.set_while_active(Some("stale".to_owned()), &cancel);

        assert_eq!(
            (published.as_deref(), handle.get().as_deref()),
            (Some("live"), Some("live"))
        );
    }
}
