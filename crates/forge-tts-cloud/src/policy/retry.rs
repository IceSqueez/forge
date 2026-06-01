use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::PcmBuffer;
use forge_platform_core::{RateLimitOutcome, RateLimiter};
use forge_tts_core::{EngineId, TtsError};

const BACKOFF: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(4),
];

#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub timeout: Duration,
    pub max_retries: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 3,
        }
    }
}

fn is_retryable(err: &TtsError) -> bool {
    matches!(err, TtsError::NetworkFailed(_) | TtsError::Io(_))
}

/// Runs `call` with per-attempt timeout, retrying transient failures with
/// exponential back-off.
///
/// Each attempt is gated through `limiter`:
/// - `Exhausted` → immediate `TtsError::RateLimited`.
/// - `Throttled` → sleep without consuming a retry slot.
/// - `Granted` → the call proceeds.
///
/// `TtsError::Timeout` is never retried (the connection is stalled; retrying
/// immediately would also stall). Non-retryable errors (`AuthFailed`,
/// `QuotaExceeded`, `InvalidVoice`, `SsmlUnsupported`, `EngineUnavailable`)
/// propagate directly. `NetworkFailed` and `Io` are retried up to
/// `cfg.max_retries` times. A `RateLimited` response with a non-zero
/// `retry_after_secs` observes the limiter floor and counts as one retry
/// attempt; without a `Retry-After` hint the normal back-off schedule applies.
pub async fn retry_synthesize<F, Fut>(
    engine_id: EngineId,
    limiter: Arc<dyn RateLimiter>,
    cfg: RetryConfig,
    call: F,
) -> Result<PcmBuffer, TtsError>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = Result<PcmBuffer, TtsError>> + Send + 'static,
{
    let mut retry_count: u32 = 0;

    loop {
        loop {
            match limiter
                .acquire(1)
                .await
                .map_err(|e| TtsError::NetworkFailed(e.to_string()))?
            {
                RateLimitOutcome::Granted => break,
                RateLimitOutcome::Throttled { wait_for } => {
                    tokio::time::sleep(wait_for).await;
                }
                RateLimitOutcome::Exhausted => {
                    return Err(TtsError::RateLimited {
                        retry_after_secs: 0,
                    });
                }
            }
        }

        let result = tokio::time::timeout(cfg.timeout, call()).await;

        match result {
            Err(_elapsed) => {
                return Err(TtsError::Timeout {
                    ms: cfg.timeout.as_millis() as u64,
                });
            }
            Ok(Ok(buf)) => return Ok(buf),
            Ok(Err(TtsError::RateLimited { retry_after_secs })) => {
                if retry_after_secs > 0 {
                    limiter
                        .observe_remote_throttle(Duration::from_secs(retry_after_secs))
                        .await;
                }
                retry_count += 1;
                if retry_count > cfg.max_retries {
                    tracing::warn!(
                        engine = %engine_id.0,
                        retry_count,
                        "rate limit retries exhausted"
                    );
                    return Err(TtsError::RateLimited { retry_after_secs });
                }
                if retry_after_secs == 0 {
                    let idx = backoff_index(retry_count);
                    tokio::time::sleep(BACKOFF[idx]).await;
                }
            }
            Ok(Err(e)) if !is_retryable(&e) => return Err(e),
            Ok(Err(e)) => {
                retry_count += 1;
                if retry_count > cfg.max_retries {
                    tracing::warn!(
                        engine = %engine_id.0,
                        retry_count,
                        error = %e,
                        "synthesis retries exhausted"
                    );
                    return Err(e);
                }
                let idx = backoff_index(retry_count);
                tracing::warn!(
                    engine = %engine_id.0,
                    retry_count,
                    error = %e,
                    backoff_ms = BACKOFF[idx].as_millis(),
                    "transient synthesis failure; retrying"
                );
                tokio::time::sleep(BACKOFF[idx]).await;
            }
        }
    }
}

fn backoff_index(retry_count: u32) -> usize {
    (retry_count as usize)
        .saturating_sub(1)
        .min(BACKOFF.len() - 1)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_tts_core::EngineId;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::policy::rate_limit::SynthesisRateLimiter;

    fn pcm() -> PcmBuffer {
        PcmBuffer::new(vec![], 16_000, 1)
    }

    fn unthrottled() -> Arc<dyn RateLimiter> {
        Arc::new(SynthesisRateLimiter::new())
    }

    #[tokio::test]
    async fn ok_on_first_call() {
        let result = retry_synthesize(
            EngineId("t".into()),
            unthrottled(),
            RetryConfig::default(),
            || async { Ok(pcm()) },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn timeout_fires() {
        tokio::time::pause();
        let cfg = RetryConfig {
            timeout: Duration::from_millis(100),
            max_retries: 0,
        };
        let task = tokio::spawn(retry_synthesize(
            EngineId("t".into()),
            unthrottled(),
            cfg,
            || async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok::<_, TtsError>(pcm())
            },
        ));
        tokio::time::advance(Duration::from_millis(200)).await;
        let res = task.await.expect("task panicked");
        assert!(matches!(res, Err(TtsError::Timeout { .. })));
    }

    #[tokio::test]
    async fn network_failure_retries_up_to_max() {
        tokio::time::pause();
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let cfg = RetryConfig {
            timeout: Duration::from_secs(60),
            max_retries: 3,
        };
        let task = tokio::spawn(retry_synthesize(
            EngineId("t".into()),
            unthrottled(),
            cfg,
            move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err::<PcmBuffer, TtsError>(TtsError::NetworkFailed("server error".into()))
                }
            },
        ));
        tokio::time::advance(Duration::from_secs(30)).await;
        let res = task.await.expect("task panicked");
        assert!(matches!(res, Err(TtsError::NetworkFailed(_))));
        assert_eq!(count.load(Ordering::Relaxed), 4);
    }

    #[tokio::test]
    async fn auth_failure_does_not_retry() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let result = retry_synthesize(
            EngineId("t".into()),
            unthrottled(),
            RetryConfig::default(),
            move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err::<PcmBuffer, TtsError>(TtsError::AuthFailed {
                        reason: "bad key".into(),
                    })
                }
            },
        )
        .await;
        assert!(matches!(result, Err(TtsError::AuthFailed { .. })));
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn quota_exceeded_does_not_retry() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let result = retry_synthesize(
            EngineId("t".into()),
            unthrottled(),
            RetryConfig::default(),
            move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err::<PcmBuffer, TtsError>(TtsError::QuotaExceeded {
                        id: EngineId("t".into()),
                        detail: "monthly limit reached".into(),
                    })
                }
            },
        )
        .await;
        assert!(matches!(result, Err(TtsError::QuotaExceeded { .. })));
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn rate_limited_with_retry_after_sleeps_then_succeeds() {
        tokio::time::pause();
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let limiter = Arc::new(SynthesisRateLimiter::new());
        let cfg = RetryConfig {
            timeout: Duration::from_secs(60),
            max_retries: 3,
        };
        let task = tokio::spawn(retry_synthesize(
            EngineId("t".into()),
            limiter,
            cfg,
            move || {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::Relaxed);
                    if n == 0 {
                        Err::<PcmBuffer, TtsError>(TtsError::RateLimited {
                            retry_after_secs: 2,
                        })
                    } else {
                        Ok(pcm())
                    }
                }
            },
        ));
        tokio::time::advance(Duration::from_secs(5)).await;
        let res = task.await.expect("task panicked");
        assert!(res.is_ok());
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn exhausted_limiter_returns_rate_limited() {
        tokio::time::pause();
        let limiter = Arc::new(SynthesisRateLimiter::new());
        limiter
            .observe_remote_throttle(Duration::from_secs(120))
            .await;
        let result = retry_synthesize(
            EngineId("t".into()),
            limiter,
            RetryConfig::default(),
            || async { Ok::<_, TtsError>(pcm()) },
        )
        .await;
        assert!(matches!(result, Err(TtsError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn wiremock_429_no_retry_after_then_200() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/tts"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/tts"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8, 0u8, 1u8, 0u8]))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/tts", server.uri());
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result = retry_synthesize(
            EngineId("mock".into()),
            unthrottled(),
            RetryConfig {
                timeout: Duration::from_secs(10),
                max_retries: 3,
            },
            move || {
                let client = client.clone();
                let url = url.clone();
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::Relaxed);
                    let resp = client
                        .post(&url)
                        .send()
                        .await
                        .map_err(|e| TtsError::NetworkFailed(e.to_string()))?;
                    match resp.status().as_u16() {
                        429 => Err(TtsError::RateLimited {
                            retry_after_secs: 0,
                        }),
                        200..=299 => {
                            let bytes = resp
                                .bytes()
                                .await
                                .map_err(|e| TtsError::NetworkFailed(e.to_string()))?;
                            let samples = bytes
                                .chunks_exact(2)
                                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                .collect();
                            Ok(PcmBuffer::new(samples, 16_000, 1))
                        }
                        s => Err(TtsError::NetworkFailed(format!("HTTP {s}"))),
                    }
                }
            },
        )
        .await;

        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        assert_eq!(call_count.load(Ordering::Relaxed), 2);
    }
}
