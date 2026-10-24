use std::time::Duration;

use tracing::error;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn bounded_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_else(|e| {
            error!(
                error = %e.without_url(),
                "failed to build a timeout-bounded reqwest client; falling back to an unbounded default client"
            );
            reqwest::Client::new()
        })
}
