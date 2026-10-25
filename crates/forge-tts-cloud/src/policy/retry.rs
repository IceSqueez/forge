use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_audio::PcmBuffer;
use forge_tts_core::{EngineId, TtsError};

use crate::network::ceil_secs;
use crate::policy::rate_limit::{EXHAUSTED_THRESHOLD, SynthesisRateLimiter};

const BACKOFF: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(4),
];

#[derive(Clone)]
pub struct RetryConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub cooldown: Option<Arc<SynthesisRateLimiter>>,
}

impl RetryConfig {
    pub fn paced_by(limiter: Arc<SynthesisRateLimiter>) -> Self {
        Self {
            cooldown: Some(limiter),
            ..Self::default()
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 3,
            cooldown: None,
        }
    }
}

impl fmt::Debug for RetryConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetryConfig")
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field("paced", &self.cooldown.is_some())
            .finish()
    }
}

fn is_retryable(err: &TtsError) -> bool {
    matches!(err, TtsError::NetworkFailed(_) | TtsError::Io(_))
}

async fn sit_out_cooldown(cfg: &RetryConfig) -> Result<(), TtsError> {
    let Some(limiter) = &cfg.cooldown else {
        return Ok(());
    };
    let remaining = limiter.cooldown_remaining();
    if remaining > EXHAUSTED_THRESHOLD {
        return Err(TtsError::RateLimited {
            retry_after_secs: ceil_secs(remaining),
        });
    }
    if !remaining.is_zero() {
        tokio::time::sleep(remaining).await;
    }
    Ok(())
}

pub async fn retry_synthesize<F, Fut>(
    engine_id: EngineId,
    cfg: RetryConfig,
    call: F,
) -> Result<PcmBuffer, TtsError>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = Result<PcmBuffer, TtsError>> + Send + 'static,
{
    let mut retry_count: u32 = 0;

    loop {
        sit_out_cooldown(&cfg).await?;
        let result = tokio::time::timeout(cfg.timeout, call()).await;

        match result {
            Err(_elapsed) => {
                return Err(TtsError::Timeout {
                    ms: cfg.timeout.as_millis() as u64,
                });
            }
            Ok(Ok(buf)) => return Ok(buf),
            Ok(Err(TtsError::RateLimited { retry_after_secs })) => {
                retry_count += 1;
                if retry_count > cfg.max_retries {
                    tracing::warn!(
                        engine = %engine_id.0,
                        retry_count,
                        "rate limit retries exhausted"
                    );
                    return Err(TtsError::RateLimited { retry_after_secs });
                }
                let wait =
                    Duration::from_secs(retry_after_secs).max(BACKOFF[backoff_index(retry_count)]);
                if wait > EXHAUSTED_THRESHOLD {
                    tracing::warn!(
                        engine = %engine_id.0,
                        retry_after_secs,
                        "rate limit wait is longer than the synthesis path waits"
                    );
                    return Err(TtsError::RateLimited { retry_after_secs });
                }
                tokio::time::sleep(wait).await;
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
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use forge_tts_core::VoiceId;
    use tokio::time::Instant;

    fn pcm() -> PcmBuffer {
        PcmBuffer::new(vec![], 16_000, 1)
    }

    fn engine() -> EngineId {
        EngineId("t".into())
    }

    struct Script {
        answers: Mutex<std::collections::VecDeque<Result<PcmBuffer, TtsError>>>,
        calls: Mutex<Vec<Instant>>,
    }

    impl Script {
        fn new(answers: Vec<Result<PcmBuffer, TtsError>>) -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(answers.into()),
                calls: Mutex::new(Vec::new()),
            })
        }

        fn gaps(&self) -> Vec<Duration> {
            self.calls
                .lock()
                .unwrap()
                .windows(2)
                .map(|pair| pair[1] - pair[0])
                .collect()
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    async fn run(script: &Arc<Script>, cfg: RetryConfig) -> Result<PcmBuffer, TtsError> {
        let script = Arc::clone(script);
        retry_synthesize(engine(), cfg, move || {
            let script = Arc::clone(&script);
            async move {
                script.calls.lock().unwrap().push(Instant::now());
                script
                    .answers
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| Ok(pcm()))
            }
        })
        .await
    }

    fn rate_limited(retry_after_secs: u64) -> Result<PcmBuffer, TtsError> {
        Err(TtsError::RateLimited { retry_after_secs })
    }

    fn network() -> Result<PcmBuffer, TtsError> {
        Err(TtsError::NetworkFailed("reset".into()))
    }

    #[tokio::test(start_paused = true)]
    async fn first_success_returns_without_waiting() {
        let script = Script::new(vec![]);
        let started = Instant::now();
        assert!(run(&script, RetryConfig::default()).await.is_ok());
        assert_eq!(script.call_count(), 1);
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn an_attempt_is_capped_at_ten_seconds_by_default() {
        for (attempt, timed_out) in [
            (Duration::from_secs(10) - Duration::from_millis(1), false),
            (Duration::from_secs(10) + Duration::from_millis(1), true),
        ] {
            let started = Instant::now();
            let result = retry_synthesize(engine(), RetryConfig::default(), move || async move {
                tokio::time::sleep(attempt).await;
                Ok::<_, TtsError>(pcm())
            })
            .await;
            if timed_out {
                assert!(
                    matches!(result, Err(TtsError::Timeout { ms: 10_000 })),
                    "{result:?}"
                );
                assert_eq!(started.elapsed(), Duration::from_secs(10));
            } else {
                assert!(result.is_ok(), "{attempt:?}: {result:?}");
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_timed_out_attempt_is_not_retried() {
        let calls = Arc::new(Mutex::new(0_usize));
        let counted = Arc::clone(&calls);
        let result = retry_synthesize(engine(), RetryConfig::default(), move || {
            let counted = Arc::clone(&counted);
            async move {
                *counted.lock().unwrap() += 1;
                std::future::pending::<Result<PcmBuffer, TtsError>>().await
            }
        })
        .await;
        assert!(matches!(result, Err(TtsError::Timeout { .. })));
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn transient_failures_retry_on_the_backoff_schedule_then_surface() {
        let script = Script::new(vec![network(), network(), network(), network()]);
        let result = run(&script, RetryConfig::default()).await;
        assert!(matches!(result, Err(TtsError::NetworkFailed(_))));
        assert_eq!(script.gaps(), BACKOFF.to_vec());
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_attempts_wait_the_longer_of_hint_and_backoff() {
        let ms = Duration::from_millis;
        let secs = Duration::from_secs;
        for (hints, gaps) in [
            (vec![0, 0], vec![ms(250), secs(1)]),
            (vec![1, 1, 1], vec![secs(1), secs(1), secs(4)]),
            (vec![2, 3], vec![secs(2), secs(3)]),
            (vec![60], vec![secs(60)]),
        ] {
            let script = Script::new(hints.iter().map(|&h| rate_limited(h)).collect());
            let result = run(&script, RetryConfig::default()).await;
            assert!(result.is_ok(), "hints {hints:?}: {result:?}");
            assert_eq!(script.gaps(), gaps, "hints {hints:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_hint_over_the_wait_threshold_returns_rate_limited_at_once() {
        let script = Script::new(vec![rate_limited(61)]);
        let started = Instant::now();
        let result = run(&script, RetryConfig::default()).await;
        assert!(matches!(
            result,
            Err(TtsError::RateLimited {
                retry_after_secs: 61
            })
        ));
        assert_eq!(script.call_count(), 1);
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_retries_stop_after_max_retries_with_the_last_hint() {
        let script = Script::new(vec![
            rate_limited(1),
            rate_limited(1),
            rate_limited(1),
            rate_limited(5),
        ]);
        let result = run(&script, RetryConfig::default()).await;
        assert!(matches!(
            result,
            Err(TtsError::RateLimited {
                retry_after_secs: 5
            })
        ));
        assert_eq!(script.call_count(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_and_transient_failures_share_one_retry_budget() {
        let script = Script::new(vec![rate_limited(1), network(), rate_limited(1), network()]);
        let result = run(&script, RetryConfig::default()).await;
        assert!(matches!(result, Err(TtsError::NetworkFailed(_))));
        assert_eq!(script.call_count(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn permanent_failures_are_returned_without_a_retry() {
        for failure in [
            TtsError::AuthFailed {
                reason: "bad key".into(),
            },
            TtsError::QuotaExceeded {
                id: engine(),
                detail: "monthly limit reached".into(),
            },
            TtsError::InvalidVoice {
                id: VoiceId("nope".into()),
            },
            TtsError::SsmlUnsupported { id: engine() },
        ] {
            let label = failure.to_string();
            let script = Script::new(vec![Err(failure)]);
            let result = run(&script, RetryConfig::default()).await;
            assert!(result.is_err(), "{label}");
            assert_eq!(script.call_count(), 1, "{label}");
        }
    }

    fn paced(floor: Duration) -> (RetryConfig, Arc<SynthesisRateLimiter>) {
        let limiter = Arc::new(SynthesisRateLimiter::new());
        limiter.observe_remote_throttle_at(Instant::now().into_std(), floor);
        (RetryConfig::paced_by(Arc::clone(&limiter)), limiter)
    }

    #[tokio::test(start_paused = true)]
    async fn a_provider_cool_down_is_sat_out_before_the_attempt_and_outside_its_window() {
        let ms = Duration::from_millis;
        let secs = Duration::from_secs;
        for (floor, first_call) in [
            (Duration::ZERO, Duration::ZERO),
            (ms(1), ms(1)),
            (secs(30), secs(30)),
            (EXHAUSTED_THRESHOLD, EXHAUSTED_THRESHOLD),
        ] {
            let (cfg, _limiter) = paced(floor);
            let script = Script::new(vec![]);
            let started = Instant::now();
            let result = run(&script, cfg).await;
            assert!(result.is_ok(), "{floor:?}: {result:?}");
            let calls = script.calls.lock().unwrap().clone();
            assert_eq!(calls.len(), 1, "{floor:?}");
            assert_eq!(calls[0] - started, first_call, "{floor:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_cool_down_past_the_threshold_returns_its_ceiling_at_once() {
        for (floor, expected) in [
            (EXHAUSTED_THRESHOLD + Duration::from_millis(1), 61),
            (Duration::from_millis(120_500), 121),
        ] {
            let (cfg, _limiter) = paced(floor);
            let script = Script::new(vec![]);
            let started = Instant::now();
            let result = run(&script, cfg).await;
            assert!(
                matches!(result, Err(TtsError::RateLimited { retry_after_secs }) if retry_after_secs == expected),
                "{floor:?}: {result:?}"
            );
            assert_eq!(script.call_count(), 0);
            assert_eq!(started.elapsed(), Duration::ZERO);
        }
    }

    async fn run_raising(
        script: &Arc<Script>,
        limiter: &Arc<SynthesisRateLimiter>,
        raise: Duration,
    ) -> Result<PcmBuffer, TtsError> {
        let script = Arc::clone(script);
        let limiter_in_call = Arc::clone(limiter);
        retry_synthesize(
            engine(),
            RetryConfig::paced_by(Arc::clone(limiter)),
            move || {
                let script = Arc::clone(&script);
                let limiter = Arc::clone(&limiter_in_call);
                async move {
                    let first = {
                        let mut calls = script.calls.lock().unwrap();
                        calls.push(Instant::now());
                        calls.len() == 1
                    };
                    if first {
                        limiter.observe_remote_throttle_at(Instant::now().into_std(), raise);
                    }
                    script
                        .answers
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or_else(|| Ok(pcm()))
                }
            },
        )
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_cool_down_raised_by_a_throttled_attempt_is_waited_once() {
        let (_, limiter) = paced(Duration::ZERO);
        let script = Script::new(vec![rate_limited(5)]);
        let result = run_raising(&script, &limiter, Duration::from_secs(5)).await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(script.gaps(), vec![Duration::from_secs(5)]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_long_cool_down_raised_during_a_retry_ends_the_retries_at_once() {
        let (_, limiter) = paced(Duration::ZERO);
        let script = Script::new(vec![network()]);
        let result = run_raising(&script, &limiter, Duration::from_secs(90)).await;
        assert!(
            matches!(
                result,
                Err(TtsError::RateLimited {
                    retry_after_secs: 90
                })
            ),
            "{result:?}"
        );
        assert_eq!(script.call_count(), 1);
    }
}
