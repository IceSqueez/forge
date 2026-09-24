use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::error::{ObsError, map_request_error};

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Switching a scene collection or profile answers only once OBS has finished loading it.
pub(crate) const CONFIG_SWITCH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct LiveSession {
    client: Arc<obws::Client>,
    suspect: Arc<Notify>,
}

impl LiveSession {
    pub(crate) fn new(client: obws::Client) -> Self {
        Self {
            client: Arc::new(client),
            suspect: Arc::new(Notify::new()),
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
        self.request_within(REQUEST_TIMEOUT, request_type, request)
            .await
    }

    /// A request that outlives `limit` flags this session as suspect, which makes the supervisor
    /// drop it and redial.
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
    use super::*;

    const WELL_PAST_ANY_DEADLINE: Duration = Duration::from_secs(600);

    fn unanswered() -> impl Future<Output = Result<(), obws::error::Error>> {
        std::future::pending()
    }

    #[tokio::test(start_paused = true)]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn an_unanswered_request_times_out_exactly_when_its_deadline_elapses() {
        let request = tokio::spawn(with_deadline(REQUEST_TIMEOUT, "GetStats", unanswered()));

        tokio::time::advance(REQUEST_TIMEOUT - Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert!(
            !request.is_finished(),
            "the request gave up before its deadline"
        );

        tokio::time::advance(Duration::from_millis(1)).await;
        let outcome = tokio::time::timeout(WELL_PAST_ANY_DEADLINE, request)
            .await
            .expect("an unanswered request never timed out")
            .unwrap();
        assert!(matches!(outcome, Err(ObsError::Timeout)), "got {outcome:?}");
    }
}
