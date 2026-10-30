use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{Aspect, AwakeError};

pub(crate) type DisruptionSender = mpsc::UnboundedSender<Aspect>;

#[async_trait]
pub(crate) trait AwakeBackend: Send {
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

type OpenFuture = Pin<Box<dyn Future<Output = Box<dyn AwakeBackend>> + Send>>;
type Opener = Box<dyn FnOnce() -> OpenFuture + Send>;

enum LazyState {
    Closed(Opener),
    Open(Box<dyn AwakeBackend>),
}

pub(crate) struct LazyBackend(Option<LazyState>);

impl LazyBackend {
    pub(crate) fn new<F, Fut>(opener: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Box<dyn AwakeBackend>> + Send + 'static,
    {
        Self(Some(LazyState::Closed(Box::new(
            move || Box::pin(opener()),
        ))))
    }

    pub(crate) fn platform(reason: String, disruptions: DisruptionSender) -> Self {
        Self::new(move || open_platform_backend(reason, disruptions))
    }
}

#[async_trait]
impl AwakeBackend for LazyBackend {
    async fn acquire(&mut self, aspect: Aspect) -> Result<(), AwakeError> {
        if matches!(self.0, Some(LazyState::Closed(_)))
            && let Some(LazyState::Closed(opener)) = self.0.take()
        {
            self.0 = Some(LazyState::Open(opener().await));
        }
        match self.0.as_mut() {
            Some(LazyState::Open(backend)) => backend.acquire(aspect).await,
            _ => Err(AwakeError::Stopped),
        }
    }

    async fn release(&mut self, aspect: Aspect) {
        if let Some(LazyState::Open(backend)) = self.0.as_mut() {
            backend.release(aspect).await;
        }
    }

    fn is_held(&self, aspect: Aspect) -> bool {
        matches!(&self.0, Some(LazyState::Open(backend)) if backend.is_held(aspect))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{AwakeBackend, LazyBackend};
    use crate::Aspect;
    use crate::fake_backend::FakeBackend;

    fn counting(opens: &Arc<AtomicUsize>, fake: &FakeBackend) -> LazyBackend {
        let opens = Arc::clone(opens);
        let fake = fake.clone();
        LazyBackend::new(move || async move {
            opens.fetch_add(1, Ordering::SeqCst);
            Box::new(fake) as Box<dyn AwakeBackend>
        })
    }

    #[tokio::test]
    async fn never_acquired_never_opens() {
        let opens = Arc::new(AtomicUsize::new(0));
        let mut lazy = counting(&opens, &FakeBackend::default());

        lazy.release(Aspect::Display).await;

        assert_eq!(opens.load(Ordering::SeqCst), 0);
        assert!(!lazy.is_held(Aspect::Display));
    }

    #[tokio::test]
    async fn opens_once_on_first_acquire_and_reuses() {
        let opens = Arc::new(AtomicUsize::new(0));
        let fake = FakeBackend::default();
        let mut lazy = counting(&opens, &fake);

        lazy.acquire(Aspect::Display).await.unwrap();
        lazy.acquire(Aspect::System).await.unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert!(lazy.is_held(Aspect::Display) && lazy.is_held(Aspect::System));
    }

    #[tokio::test]
    async fn release_after_acquire_reaches_the_opened_backend() {
        let opens = Arc::new(AtomicUsize::new(0));
        let fake = FakeBackend::default();
        let mut lazy = counting(&opens, &fake);
        lazy.acquire(Aspect::Display).await.unwrap();

        lazy.release(Aspect::Display).await;

        assert!(!fake.holds(Aspect::Display));
    }
}
