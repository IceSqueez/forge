use std::sync::Arc;

use forge_platform_core::{PlatformFuture, TokenSource};

pub(crate) type RejectedTokenReplacer = Arc<dyn Fn(String) -> PlatformFuture<String> + Send + Sync>;

#[derive(Clone)]
pub(crate) struct AccessTokens {
    current: TokenSource,
    replace_rejected: RejectedTokenReplacer,
}

impl AccessTokens {
    pub(crate) fn new(current: TokenSource, replace_rejected: RejectedTokenReplacer) -> Self {
        Self {
            current,
            replace_rejected,
        }
    }

    pub(crate) fn source(&self) -> TokenSource {
        Arc::clone(&self.current)
    }

    pub(crate) async fn current(&self) -> Result<String, forge_platform_core::PlatformError> {
        (self.current)().await
    }

    pub(crate) async fn replace_current(
        &self,
    ) -> Result<String, forge_platform_core::PlatformError> {
        let rejected = self.current().await?;
        (self.replace_rejected)(rejected).await
    }
}

impl From<TokenSource> for AccessTokens {
    fn from(current: TokenSource) -> Self {
        let again = Arc::clone(&current);
        Self {
            current,
            replace_rejected: Arc::new(move |_| again()),
        }
    }
}
