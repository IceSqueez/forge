use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::error::{ObsError, map_request_error};

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) const CONFIG_SWITCH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct LiveSession {
    client: Arc<obws::Client>,
    suspect: Arc<Notify>,
    request_timeout: Duration,
}

impl LiveSession {
    pub(crate) fn new(client: obws::Client, request_timeout: Duration) -> Self {
        Self {
            client: Arc::new(client),
            suspect: Arc::new(Notify::new()),
            request_timeout,
        }
    }

    pub(crate) fn obs(&self) -> &obws::Client {
        &self.client
    }

    pub(crate) async fn request<T>(
        &self,
        request_type: &'static str,
        request: impl Future<Output = Result<T, obws::error::Error>>,
    ) -> Result<T, ObsError> {
        self.request_within(self.request_timeout, request_type, request)
            .await
    }

    pub(crate) async fn request_within<T>(
        &self,
        limit: Duration,
        request_type: &'static str,
        request: impl Future<Output = Result<T, obws::error::Error>>,
    ) -> Result<T, ObsError> {
        let outcome = with_deadline(limit, request_type, request).await;
        if matches!(outcome, Err(ObsError::Timeout)) {
            self.mark_suspect();
        }
        outcome
    }

    pub(crate) fn mark_suspect(&self) {
        self.suspect.notify_one();
    }

    pub(crate) async fn suspected(&self) {
        self.suspect.notified().await;
    }
}

pub(crate) async fn with_deadline<T>(
    limit: Duration,
    request_type: &'static str,
    request: impl Future<Output = Result<T, obws::error::Error>>,
) -> Result<T, ObsError> {
    match tokio::time::timeout(limit, request).await {
        Ok(outcome) => outcome.map_err(|e| map_request_error(request_type, e)),
        Err(_) => {
            tracing::warn!(
                request_type,
                timeout_ms = limit.as_millis(),
                "OBS request timed out"
            );
            Err(ObsError::Timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    fn unanswered() -> impl Future<Output = Result<(), obws::error::Error>> {
        std::future::pending()
    }

    #[tokio::test(start_paused = true)]
    async fn an_unanswered_request_times_out_exactly_when_its_deadline_elapses() {
        let request = with_deadline(REQUEST_TIMEOUT, "GetStats", unanswered());
        tokio::pin!(request);
        assert!((&mut request).now_or_never().is_none(), "already timed out");

        tokio::time::advance(REQUEST_TIMEOUT - Duration::from_millis(1)).await;
        assert!(
            (&mut request).now_or_never().is_none(),
            "the request gave up before its deadline"
        );

        tokio::time::advance(Duration::from_millis(1)).await;
        let outcome = (&mut request)
            .now_or_never()
            .expect("an unanswered request outlived its deadline");
        assert!(matches!(outcome, Err(ObsError::Timeout)), "got {outcome:?}");
    }
}
