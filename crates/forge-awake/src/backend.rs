use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{Aspect, AwakeError};

/// Each message means the OS may have dropped, or can now grant, the hold for that aspect.
pub(crate) type DisruptionSender = mpsc::UnboundedSender<Aspect>;

#[async_trait]
pub(crate) trait AwakeBackend: Send {
    /// Acquiring an aspect that is already held replaces the previous hold.
    async fn acquire(&mut self, aspect: Aspect) -> Result<(), AwakeError>;
    async fn release(&mut self, aspect: Aspect);
    fn is_held(&self, aspect: Aspect) -> bool;
}

pub(crate) async fn open_platform_backend(
    reason: String,
    disruptions: DisruptionSender,
) -> Box<dyn AwakeBackend> {
    #[cfg(target_os = "linux")]
    {
        Box::new(crate::backend_linux::LinuxBackend::open(reason, disruptions).await)
    }
    #[cfg(target_os = "macos")]
    {
        drop(disruptions);
        Box::new(crate::backend_macos::MacBackend::new(reason))
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(crate::backend_windows::WindowsBackend::new(
            reason,
            disruptions,
        ))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        drop((reason, disruptions));
        Box::new(crate::backend_unsupported::UnsupportedBackend)
    }
}
