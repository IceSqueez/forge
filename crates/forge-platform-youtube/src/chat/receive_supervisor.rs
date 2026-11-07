use std::time::Duration;

use forge_net::{Backoff, DEFAULT_RETRY_AFTER};
use forge_platform_core::{PlatformError, TokenSource};
use time::OffsetDateTime;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::api::data_api::YouTubeDataApi;
use crate::api::quota_state::{QUOTA_RESET_WAKE_MARGIN, today_pacific, until_next_reset};
use crate::chat::access_tokens::AccessTokens;
use crate::chat::chat_poller::{
    ChatMessagesResponse, ChatSession, FetchFailure, PageEffect, Receive, ReceiveExit,
    YouTubeChatPoller, fetch_chat_page,
};
use crate::chat::stream_transport::{ChatStream, ChatStreamTransport, StreamEvent, StreamOutcome};
use crate::chat::transport_mode::{ChatTransportMode, TransportModeSink};

const RECONNECT_PAUSE_SECS: u64 = 5;
const STREAM_REPROBE_INTERVAL_SECS: u64 = 600;
const STREAM_FAILURES_BEFORE_FALLBACK: u32 = 3;
const RATE_LIMITED_REPEATS_BEFORE_QUOTA_PATH: u32 = 5;
const UNAUTHORIZED_REASON: &str = "chat read rejected the access token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ReceiveTimings {
    pub(crate) reconnect_pause: Duration,
    pub(crate) reprobe_interval: Duration,
}

impl ReceiveTimings {
    pub(crate) const DEFAULT: Self = Self {
        reconnect_pause: Duration::from_secs(RECONNECT_PAUSE_SECS),
        reprobe_interval: Duration::from_secs(STREAM_REPROBE_INTERVAL_SECS),
    };
}

impl Default for ReceiveTimings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

pub(crate) trait PageStream: Send {
    fn next(&mut self) -> impl Future<Output = Option<StreamEvent>> + Send;
    fn resume_token(&self) -> Option<String>;
}

pub(crate) trait ChatTransports: Send + Sync {
    type Stream: PageStream;

    fn open_stream(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Self::Stream, StreamOutcome>> + Send;

    fn poll_page(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<ChatMessagesResponse, StreamOutcome>> + Send;

    fn refresh_token(&self) -> impl Future<Output = Result<(), PlatformError>> + Send;

    fn long_interval_mode(&self) -> impl Future<Output = bool> + Send;

    fn until_quota_reset(&self) -> Duration;
}

#[derive(Clone)]
pub(crate) struct LiveTransports {
    stream: ChatStreamTransport,
    api: YouTubeDataApi,
    access_tokens: AccessTokens,
}

impl LiveTransports {
    pub(crate) fn new(api: YouTubeDataApi, access_tokens: impl Into<AccessTokens>) -> Self {
        let access_tokens = access_tokens.into();
        let stream_tokens: TokenSource = access_tokens.source();
        Self {
            stream: ChatStreamTransport::new(api.clone(), stream_tokens),
            api,
            access_tokens,
        }
    }
}

impl PageStream for ChatStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        ChatStream::next(self).await
    }

    fn resume_token(&self) -> Option<String> {
        ChatStream::resume_token(self).map(ToOwned::to_owned)
    }
}

impl ChatTransports for LiveTransports {
    type Stream = ChatStream;

    async fn open_stream(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ChatStream, StreamOutcome> {
        self.stream.open(live_chat_id, page_token, cancel).await
    }

    async fn poll_page(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ChatMessagesResponse, StreamOutcome> {
        let token = self
            .access_tokens
            .current()
            .await
            .map_err(StreamOutcome::Transport)?;
        fetch_chat_page(&self.api, &token, live_chat_id, page_token, cancel)
            .await
            .map_err(|failure| match failure {
                FetchFailure::Cancelled => StreamOutcome::Cancelled,
                FetchFailure::QuotaExhausted => StreamOutcome::QuotaExhausted,
                FetchFailure::ChatEnded => StreamOutcome::ChatEnded,
                FetchFailure::Failed(error) => outcome_of(error),
            })
    }

    async fn refresh_token(&self) -> Result<(), PlatformError> {
        self.access_tokens.replace_current().await.map(|_| ())
    }

    async fn long_interval_mode(&self) -> bool {
        self.api
            .quota()
            .lock()
            .await
            .long_interval_mode(today_pacific())
    }

    fn until_quota_reset(&self) -> Duration {
        until_next_reset(OffsetDateTime::now_utc())
    }
}

fn outcome_of(error: PlatformError) -> StreamOutcome {
    match error {
        PlatformError::RateLimited { retry_after } => StreamOutcome::RateLimited { retry_after },
        PlatformError::QuotaExhausted => StreamOutcome::QuotaExhausted,
        PlatformError::RateLimitExhausted => StreamOutcome::RateLimited {
            retry_after: DEFAULT_RETRY_AFTER,
        },
        PlatformError::Http { status, .. } if status == forge_net::status::UNAUTHORIZED => {
            StreamOutcome::Unauthorized
        }
        other => StreamOutcome::Transport(other),
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Streaming,
    Polling { probe_at: Instant },
}

enum Step {
    Continue,
    Exit(ReceiveExit),
}

pub(crate) struct ReceiveSupervisor<T: ChatTransports> {
    transports: T,
    timings: ReceiveTimings,
    backoff: Backoff,
    mode: Mode,
    stream_failures: u32,
    rate_limited_repeats: u32,
    token_refreshed: bool,
    resume_token: Option<String>,
    bound_chat_id: Option<String>,
    mode_sink: Option<TransportModeSink>,
}

impl<T: ChatTransports> ReceiveSupervisor<T> {
    pub(crate) fn new(transports: T, timings: ReceiveTimings) -> Self {
        Self {
            transports,
            timings,
            backoff: Backoff::default(),
            mode: Mode::Streaming,
            stream_failures: 0,
            rate_limited_repeats: 0,
            token_refreshed: false,
            resume_token: None,
            bound_chat_id: None,
            mode_sink: None,
        }
    }

    pub(crate) fn with_mode_sink(mut self, sink: TransportModeSink) -> Self {
        sink.publish(ChatTransportMode::Streaming);
        self.mode_sink = Some(sink);
        self
    }

    fn publish_mode(&self, mode: ChatTransportMode) {
        if let Some(sink) = &self.mode_sink {
            sink.publish(mode);
        }
    }

    fn bind(&mut self, live_chat_id: &str) {
        if self.bound_chat_id.as_deref() == Some(live_chat_id) {
            return;
        }
        self.bound_chat_id = Some(live_chat_id.to_owned());
        self.mode = Mode::Streaming;
        self.stream_failures = 0;
        self.resume_token = None;
    }

    fn note_success(&mut self, session: &mut ChatSession<'_>) {
        session.health.succeeded();
        self.stream_failures = 0;
        self.rate_limited_repeats = 0;
        self.token_refreshed = false;
        self.backoff.reset();
    }

    fn enter_polling(&mut self) {
        tracing::info!("youtube chat stream unavailable; receiving chat by polling");
        self.mode = Mode::Polling {
            probe_at: Instant::now() + self.timings.reprobe_interval,
        };
        self.publish_mode(ChatTransportMode::Polling);
        self.stream_failures = 0;
        self.resume_token = None;
    }

    fn begin_probe(&mut self) {
        self.mode = Mode::Streaming;
        self.stream_failures = STREAM_FAILURES_BEFORE_FALLBACK - 1;
        self.resume_token = None;
    }

    async fn reconnect_pause(&self, poller: &YouTubeChatPoller) -> Duration {
        if self.transports.long_interval_mode().await {
            poller
                .timings()
                .long_interval
                .max(self.timings.reconnect_pause)
        } else {
            self.timings.reconnect_pause
        }
    }

    async fn poll_floor(&self, poller: &YouTubeChatPoller) -> Duration {
        if self.transports.long_interval_mode().await {
            poller.timings().long_interval
        } else {
            poller.timings().poll_floor
        }
    }

    async fn stream_step(
        &mut self,
        poller: &YouTubeChatPoller,
        session: &mut ChatSession<'_>,
    ) -> Step {
        let cancel = session.cancel;
        let opened = tokio::select! {
            biased;
            () = cancel.cancelled() => return Step::Exit(ReceiveExit::Stop),
            opened = self.transports.open_stream(
                session.live_chat_id,
                self.resume_token.as_deref(),
                cancel,
            ) => opened,
        };
        let mut stream = match opened {
            Ok(stream) => stream,
            Err(outcome) => return self.route(outcome, poller, session).await,
        };
        loop {
            let event = tokio::select! {
                biased;
                () = cancel.cancelled() => return Step::Exit(ReceiveExit::Stop),
                event = stream.next() => event,
            };
            match event {
                Some(StreamEvent::Page(page)) => {
                    self.note_success(session);
                    self.publish_mode(ChatTransportMode::Streaming);
                    match poller.handle_page(&page, session).await {
                        PageEffect::BusClosed => return Step::Exit(ReceiveExit::Stop),
                        PageEffect::ChatEnded => return Step::Exit(ReceiveExit::BroadcastEnded),
                        PageEffect::Continue => {}
                    }
                    self.resume_token = stream.resume_token();
                }
                Some(StreamEvent::Ended(outcome)) => {
                    self.resume_token = stream.resume_token();
                    return self.route(outcome, poller, session).await;
                }
                None => {
                    self.resume_token = stream.resume_token();
                    return self.route(StreamOutcome::Closed, poller, session).await;
                }
            }
        }
    }

    async fn poll_step(
        &mut self,
        poller: &YouTubeChatPoller,
        session: &mut ChatSession<'_>,
        poll_token: &mut Option<String>,
    ) -> Step {
        let cancel = session.cancel;
        let polled = tokio::select! {
            biased;
            () = cancel.cancelled() => return Step::Exit(ReceiveExit::Stop),
            polled = self.transports.poll_page(
                session.live_chat_id,
                poll_token.as_deref(),
                cancel,
            ) => polled,
        };
        let page = match polled {
            Ok(page) => page,
            Err(outcome) => return self.route(outcome, poller, session).await,
        };
        self.note_success(session);
        match poller.handle_page(&page, session).await {
            PageEffect::BusClosed => return Step::Exit(ReceiveExit::Stop),
            PageEffect::ChatEnded => return Step::Exit(ReceiveExit::BroadcastEnded),
            PageEffect::Continue => {}
        }
        let wait =
            Duration::from_millis(page.polling_interval_millis).max(self.poll_floor(poller).await);
        *poll_token = page.next_page_token;
        pause(wait, cancel).await
    }

    async fn route(
        &mut self,
        outcome: StreamOutcome,
        poller: &YouTubeChatPoller,
        session: &mut ChatSession<'_>,
    ) -> Step {
        let cancel = session.cancel;
        match outcome {
            StreamOutcome::Closed => {
                self.note_success(session);
                pause(self.reconnect_pause(poller).await, cancel).await
            }
            StreamOutcome::Cancelled => Step::Exit(ReceiveExit::Stop),
            StreamOutcome::ChatEnded => {
                session.health.succeeded();
                self.resume_token = None;
                Step::Exit(ReceiveExit::BroadcastEnded)
            }
            StreamOutcome::QuotaExhausted => self.wait_for_quota_reset(cancel).await,
            StreamOutcome::RateLimited { retry_after } => {
                session.health.failed();
                self.rate_limited_repeats = self.rate_limited_repeats.saturating_add(1);
                if self.rate_limited_repeats > RATE_LIMITED_REPEATS_BEFORE_QUOTA_PATH {
                    return self.wait_for_quota_reset(cancel).await;
                }
                tracing::warn!(
                    retry_after_secs = retry_after.as_secs(),
                    "youtube chat receive throttled"
                );
                pause(retry_after, cancel).await
            }
            StreamOutcome::Unauthorized => {
                if self.token_refreshed {
                    let error = PlatformError::Auth {
                        reason: UNAUTHORIZED_REASON.to_owned(),
                    };
                    return self.transport_failure(error, session).await;
                }
                self.token_refreshed = true;
                let refreshed = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Step::Exit(ReceiveExit::Stop),
                    refreshed = self.transports.refresh_token() => refreshed,
                };
                match refreshed {
                    Ok(()) => Step::Continue,
                    Err(error) => self.transport_failure(error, session).await,
                }
            }
            StreamOutcome::ProtocolNotOffered => {
                self.enter_polling();
                Step::Continue
            }
            StreamOutcome::Transport(error) => self.transport_failure(error, session).await,
        }
    }

    async fn transport_failure(
        &mut self,
        error: PlatformError,
        session: &mut ChatSession<'_>,
    ) -> Step {
        session.health.failed();
        tracing::warn!(
            "youtube chat receive failed: {}",
            crate::api::error_shape::redact_platform_error(&error)
        );
        if matches!(self.mode, Mode::Streaming) {
            self.stream_failures = self.stream_failures.saturating_add(1);
            if self.stream_failures >= STREAM_FAILURES_BEFORE_FALLBACK {
                self.enter_polling();
            }
        }
        pause(self.backoff.next_delay(), session.cancel).await
    }

    async fn wait_for_quota_reset(&mut self, cancel: &CancellationToken) -> Step {
        tracing::warn!(
            "youtube chat receive paused until the daily quota resets: {}",
            PlatformError::QuotaExhausted
        );
        self.mode = Mode::Streaming;
        self.stream_failures = 0;
        self.rate_limited_repeats = 0;
        self.resume_token = None;
        let wait = self.transports.until_quota_reset() + QUOTA_RESET_WAKE_MARGIN;
        match pause(wait, cancel).await {
            Step::Continue => Step::Exit(ReceiveExit::Resolve),
            exit => exit,
        }
    }
}

async fn pause(duration: Duration, cancel: &CancellationToken) -> Step {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Step::Exit(ReceiveExit::Stop),
        () = tokio::time::sleep(duration) => Step::Continue,
    }
}

impl<T: ChatTransports> Receive for ReceiveSupervisor<T> {
    async fn receive(
        &mut self,
        poller: &YouTubeChatPoller,
        mut session: ChatSession<'_>,
    ) -> ReceiveExit {
        self.bind(session.live_chat_id);
        let mut poll_token: Option<String> = None;
        loop {
            if session.cancel.is_cancelled() {
                return ReceiveExit::Stop;
            }
            if session.broadcast_moved_on() {
                return ReceiveExit::Resolve;
            }
            let step = match self.mode {
                Mode::Streaming => self.stream_step(poller, &mut session).await,
                Mode::Polling { probe_at } if Instant::now() >= probe_at => {
                    self.begin_probe();
                    continue;
                }
                Mode::Polling { .. } => self.poll_step(poller, &mut session, &mut poll_token).await,
            };
            if let Step::Exit(exit) = step {
                return exit;
            }
        }
    }
}

#[cfg(test)]
mod tests;
