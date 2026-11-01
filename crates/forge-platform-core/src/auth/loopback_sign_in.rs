use std::time::Duration;

use async_trait::async_trait;

use super::pkce_flow::PkceAuthorizeUrl;
use crate::error::PlatformError;

#[async_trait]
pub trait LoopbackAttempt: Send {
    async fn start(&mut self) -> Result<PkceAuthorizeUrl, PlatformError>;
    async fn complete(self: Box<Self>, timeout: Duration) -> Result<(), PlatformError>;
}

pub trait LoopbackSignIn: Send + Sync {
    fn attempt(&self) -> Box<dyn LoopbackAttempt>;
}
