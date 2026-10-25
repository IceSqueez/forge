use std::sync::{Arc, OnceLock};
use std::time::Duration;

use forge_net::{
    BodyProgress, BodySink, CallClass, DEFAULT_RETRY_AFTER_SECS, Gateway, GatewayFailure,
    GatewayProfile, GatewayRequest, GatewayResponse, Method, RateLimiter, StreamFailure,
    ThrottleHintFamily, TokenBucketRateLimiter,
};
use tokio_util::sync::CancellationToken;

use crate::UpdateError;

const GITHUB_API_REQUEST_BUDGET: u32 = 60;
const GITHUB_API_WINDOW: Duration = Duration::from_secs(60 * 60);
const RELEASE_DOWNLOAD_REQUEST_BUDGET: u32 = 120;
const RELEASE_DOWNLOAD_WINDOW: Duration = Duration::from_secs(60 * 60);
const NO_HINT_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

const RELEASE_API_PROFILE: GatewayProfile = GatewayProfile::absolute_https(
    CallClass::Catalog,
    ThrottleHintFamily::ResetEpochSeconds,
    NO_HINT_WAIT,
);
const RELEASE_DOWNLOAD_PROFILE: GatewayProfile = GatewayProfile::absolute_https(
    CallClass::Media,
    ThrottleHintFamily::RetryAfter,
    NO_HINT_WAIT,
);

#[derive(Clone)]
pub struct UpdateClient {
    release_api: Gateway,
    release_downloads: Gateway,
}

pub(crate) enum StreamError {
    TooLarge,
    Update(UpdateError),
}

impl UpdateClient {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Result<Self, UpdateError> {
        Self::with_limiters(Arc::clone(&limiter), limiter)
    }

    pub fn for_github() -> Result<Self, UpdateError> {
        static CLIENT: OnceLock<UpdateClient> = OnceLock::new();
        if let Some(client) = CLIENT.get() {
            return Ok(client.clone());
        }
        let client = Self::with_limiters(
            Arc::new(TokenBucketRateLimiter::new(
                GITHUB_API_REQUEST_BUDGET,
                GITHUB_API_WINDOW,
            )),
            Arc::new(TokenBucketRateLimiter::new(
                RELEASE_DOWNLOAD_REQUEST_BUDGET,
                RELEASE_DOWNLOAD_WINDOW,
            )),
        )?;
        Ok(CLIENT.get_or_init(|| client).clone())
    }

    fn with_limiters(
        api_limiter: Arc<dyn RateLimiter>,
        download_limiter: Arc<dyn RateLimiter>,
    ) -> Result<Self, UpdateError> {
        let build = |limiter, profile| {
            Gateway::without_base(limiter, profile, None)
                .map_err(|error| UpdateError::Client(error.to_string()))
        };
        Ok(Self {
            release_api: build(api_limiter, RELEASE_API_PROFILE)?,
            release_downloads: build(download_limiter, RELEASE_DOWNLOAD_PROFILE)?,
        })
    }

    pub(crate) async fn fetch_release_metadata(
        &self,
        url: &str,
        limit: u64,
    ) -> Result<Vec<u8>, UpdateError> {
        fetch_small(&self.release_api, url, limit).await
    }

    pub(crate) async fn fetch_release_file(
        &self,
        url: &str,
        limit: u64,
    ) -> Result<Vec<u8>, UpdateError> {
        fetch_small(&self.release_downloads, url, limit).await
    }

    pub(crate) async fn stream_release_file<S, P>(
        &self,
        url: &str,
        cap: u64,
        sink: &mut S,
        on_progress: P,
        cancel: &CancellationToken,
    ) -> Result<u64, StreamError>
    where
        S: BodySink<Error = std::io::Error>,
        P: FnMut(BodyProgress) + Send,
    {
        let request = GatewayRequest::absolute(Method::Get, url).class(CallClass::Media);
        self.release_downloads
            .send_streaming(request, cap, sink, on_progress, cancel)
            .await
            .map(|response| response.received())
            .map_err(|failure| match failure {
                StreamFailure::Gateway(GatewayFailure::ResponseTooLarge { .. }) => {
                    StreamError::TooLarge
                }
                StreamFailure::Gateway(failure) => {
                    StreamError::Update(UpdateError::from_gateway(failure))
                }
                StreamFailure::Sink(error) => StreamError::Update(UpdateError::Io(error)),
            })
    }
}

async fn fetch_small(gateway: &Gateway, url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
    let request = GatewayRequest::absolute(Method::Get, url)
        .class(CallClass::Catalog)
        .body_cap(limit);
    gateway
        .send(request)
        .await
        .map(GatewayResponse::into_body)
        .map_err(|failure| match failure {
            GatewayFailure::ResponseTooLarge { .. } => UpdateError::ResponseTooLarge { limit },
            failure => UpdateError::from_gateway(failure),
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use forge_net::test_support::RecordingRateLimiter;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const HEADER_END: &[u8] = b"\r\n\r\n";
    const RETRY_AFTER: Duration = Duration::from_secs(7);
    const LIMIT: u64 = 16;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Call {
        Metadata,
        ReleaseFile,
        Artifact,
    }

    impl Call {
        fn uses_api_budget(self) -> bool {
            self == Self::Metadata
        }
    }

    struct Fixture {
        client: UpdateClient,
        api: Arc<RecordingRateLimiter>,
        downloads: Arc<RecordingRateLimiter>,
    }

    fn fixture(api: RecordingRateLimiter, downloads: RecordingRateLimiter) -> Fixture {
        let api = Arc::new(api);
        let downloads = Arc::new(downloads);
        let client = UpdateClient::with_limiters(api.clone(), downloads.clone()).unwrap();
        Fixture {
            client,
            api,
            downloads,
        }
    }

    fn granting() -> Fixture {
        fixture(
            RecordingRateLimiter::granting(),
            RecordingRateLimiter::granting(),
        )
    }

    struct Served {
        url: String,
        hits: Arc<AtomicUsize>,
    }

    async fn serve(response: Vec<u8>) -> Served {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                let response = response.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buffer = [0_u8; 1024];
                    while !head.windows(HEADER_END.len()).any(|w| w == HEADER_END) {
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => head.extend_from_slice(&buffer[..read]),
                        }
                    }
                    if stream.write_all(&response).await.is_ok() {
                        stream.shutdown().await.ok();
                    }
                });
            }
        });
        Served { url, hits }
    }

    fn answer(status_line: &str, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {status_line}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(body);
        out
    }

    fn close_delimited(body: &[u8]) -> Vec<u8> {
        let mut out = b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\n".to_vec();
        out.extend_from_slice(body);
        out
    }

    struct MemorySink(Vec<u8>);

    impl BodySink for MemorySink {
        type Error = std::io::Error;

        async fn accept(&mut self, chunk: &[u8]) -> Result<(), std::io::Error> {
            self.0.extend_from_slice(chunk);
            Ok(())
        }
    }

    async fn run(client: &UpdateClient, call: Call, url: &str) -> Result<Vec<u8>, UpdateError> {
        match call {
            Call::Metadata => client.fetch_release_metadata(url, LIMIT).await,
            Call::ReleaseFile => client.fetch_release_file(url, LIMIT).await,
            Call::Artifact => {
                let mut sink = MemorySink(Vec::new());
                match client
                    .stream_release_file(url, LIMIT, &mut sink, |_| {}, &CancellationToken::new())
                    .await
                {
                    Ok(_) => Ok(sink.0),
                    Err(StreamError::TooLarge) => {
                        Err(UpdateError::ArtifactOversized { expected: LIMIT })
                    }
                    Err(StreamError::Update(error)) => Err(error),
                }
            }
        }
    }

    const CALLS: [Call; 3] = [Call::Metadata, Call::ReleaseFile, Call::Artifact];

    #[tokio::test]
    async fn only_release_metadata_draws_on_the_api_budget_and_every_release_file_on_the_download_budget()
     {
        let served = serve(answer("200 OK", "", b"ok")).await;
        let fixture = granting();

        for call in CALLS {
            run(&fixture.client, call, &served.url).await.unwrap();
        }

        assert_eq!(fixture.api.acquire_count(), 1);
        assert_eq!(fixture.downloads.acquire_count(), 2);
    }

    #[tokio::test]
    async fn an_exhausted_budget_refuses_only_its_own_gateway_before_any_request() {
        for exhausted_api in [true, false] {
            let served = serve(answer("200 OK", "", b"ok")).await;
            let (api, downloads) = if exhausted_api {
                (
                    RecordingRateLimiter::exhausted(),
                    RecordingRateLimiter::granting(),
                )
            } else {
                (
                    RecordingRateLimiter::granting(),
                    RecordingRateLimiter::exhausted(),
                )
            };
            let fixture = fixture(api, downloads);

            for call in CALLS {
                let result = run(&fixture.client, call, &served.url).await;
                if call.uses_api_budget() == exhausted_api {
                    assert!(
                        matches!(result, Err(UpdateError::RateLimited)),
                        "api exhausted {exhausted_api}, {call:?}: {result:?}"
                    );
                } else {
                    assert_eq!(
                        result.unwrap(),
                        b"ok",
                        "api exhausted {exhausted_api}, {call:?}"
                    );
                }
            }
            let expected_hits = if exhausted_api { 2 } else { 1 };
            assert_eq!(served.hits.load(Ordering::SeqCst), expected_hits);
        }
    }

    #[tokio::test]
    async fn a_throttle_answer_cools_only_the_budget_of_the_gateway_that_received_it() {
        for (status_line, code) in [("429 Too Many Requests", 429), ("503 Unavailable", 503)] {
            for call in CALLS {
                let served = serve(answer(status_line, "retry-after: 7\r\n", b"")).await;
                let fixture = granting();

                let result = run(&fixture.client, call, &served.url).await;

                assert!(
                    matches!(result, Err(UpdateError::Status(seen)) if seen == code),
                    "{code} {call:?}: {result:?}"
                );
                let (cooled, untouched) = if call.uses_api_budget() {
                    (&fixture.api, &fixture.downloads)
                } else {
                    (&fixture.downloads, &fixture.api)
                };
                assert_eq!(
                    cooled.observed_throttles(),
                    [RETRY_AFTER],
                    "{code} {call:?}"
                );
                assert!(untouched.observed_throttles().is_empty(), "{code} {call:?}");
            }
        }
    }

    #[tokio::test]
    async fn a_forbidden_release_answer_is_a_plain_status_that_cools_no_budget() {
        for call in CALLS {
            let served = serve(answer(
                "403 Forbidden",
                "x-ratelimit-remaining: 0\r\nretry-after: 7\r\n",
                b"",
            ))
            .await;
            let fixture = granting();

            let result = run(&fixture.client, call, &served.url).await;

            assert!(
                matches!(result, Err(UpdateError::Status(403))),
                "{call:?}: {result:?}"
            );
            assert!(fixture.api.observed_throttles().is_empty(), "{call:?}");
            assert!(
                fixture.downloads.observed_throttles().is_empty(),
                "{call:?}"
            );
        }
    }

    #[tokio::test]
    async fn each_call_accepts_a_body_at_its_limit_and_refuses_one_byte_over_declared_or_streamed()
    {
        let at_limit = vec![b'x'; LIMIT as usize];
        let over_limit = vec![b'x'; LIMIT as usize + 1];
        for call in CALLS {
            for (label, response, accepted) in [
                ("declared at", answer("200 OK", "", &at_limit), true),
                ("streamed at", close_delimited(&at_limit), true),
                ("declared over", answer("200 OK", "", &over_limit), false),
                ("streamed over", close_delimited(&over_limit), false),
            ] {
                let served = serve(response).await;

                let result = run(&granting().client, call, &served.url).await;

                match (call, accepted) {
                    (_, true) => assert_eq!(result.unwrap(), at_limit, "{call:?} {label}"),
                    (Call::Artifact, false) => assert!(
                        matches!(
                            result,
                            Err(UpdateError::ArtifactOversized { expected: LIMIT })
                        ),
                        "{call:?} {label}: {result:?}"
                    ),
                    (_, false) => assert!(
                        matches!(result, Err(UpdateError::ResponseTooLarge { limit: LIMIT })),
                        "{call:?} {label}: {result:?}"
                    ),
                }
            }
        }
    }

    #[tokio::test]
    async fn every_github_client_draws_on_one_process_wide_budget_per_gateway() {
        let served = serve(answer("200 OK", "", b"ok")).await;
        let first = UpdateClient::for_github().unwrap();
        first
            .fetch_release_metadata(&served.url, LIMIT)
            .await
            .unwrap();
        for _ in 0..2 {
            first.fetch_release_file(&served.url, LIMIT).await.unwrap();
        }

        let second = UpdateClient::for_github().unwrap();

        let api = second.release_api.limiter_usage();
        let downloads = second.release_downloads.limiter_usage();
        assert_eq!((api.used, api.capacity), (Some(1), Some(60)));
        assert_eq!((downloads.used, downloads.capacity), (Some(2), Some(120)));
    }
}
