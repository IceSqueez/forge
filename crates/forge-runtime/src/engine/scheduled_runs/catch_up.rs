use std::pin::Pin;
use std::time::Duration;

pub struct CatchUpSettle(Pin<Box<dyn Future<Output = ()> + Send>>);

impl CatchUpSettle {
    pub fn when(settled: impl Future<Output = ()> + Send + 'static) -> Self {
        Self(Box::pin(settled))
    }

    pub fn immediately() -> Self {
        Self::when(std::future::ready(()))
    }

    pub(super) async fn wait(self, limit: Duration) -> bool {
        tokio::time::timeout(limit, self.0).await.is_ok()
    }
}
