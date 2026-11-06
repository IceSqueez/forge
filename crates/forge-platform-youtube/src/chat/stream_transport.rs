use std::time::Duration;

use forge_net::{
    DEFAULT_RETRY_AFTER, ExpectedRefusal, GatewayFailure, GrpcItem, GrpcRequest, GrpcStream,
};
use forge_platform_core::{PlatformError, TokenSource};
use forge_types::Secret;
use prost::Message;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::api::data_api::{DataApiError, YouTubeDataApi, malformed};
use crate::api::quota_state::STREAM_OPEN_COST;
use crate::chat::chat_poller::{
    ChatMessagesResponse, MISSING_SCOPE, page_from_body, transcode_response,
};
use crate::stream_list_proto::{LiveChatMessageListRequest, LiveChatMessageListResponse};

const STREAM_LIST_PATH: &str = "/youtube.api.v3.V3DataLiveChatMessageService/StreamList";
const STREAM_LABEL: &str = "StreamList";
const MESSAGE_CAP_BYTES: u64 = 4 * 1024 * 1024;
const REQUESTED_PARTS: [&str; 3] = ["id", "snippet", "authorDetails"];
const CHAT_ENDED_EVENT_TYPE: &str = "chatEndedEvent";
const LIVE_CHAT_ENDED_REASON: &str = "liveChatEnded";
const LIVE_CHAT_ENDED_STATUS: u16 = forge_net::status::FORBIDDEN;
const QUOTA_REASON: &str = "quotaexceeded";
const RATE_REASON: &str = "ratelimitexceeded";
const LIVE_CHAT_ENDED_NORMALIZED: &str = "livechatended";

#[derive(Debug)]
pub(crate) enum StreamOutcome {
    QuotaExhausted,
    RateLimited { retry_after: Duration },
    Unauthorized,
    ProtocolNotOffered,
    ChatEnded,
    Closed,
    Cancelled,
    Transport(PlatformError),
}

pub(crate) enum StreamEvent {
    Page(ChatMessagesResponse),
    Ended(StreamOutcome),
}

#[derive(Clone)]
pub(crate) struct ChatStreamTransport {
    api: YouTubeDataApi,
    access_token_source: TokenSource,
}

impl ChatStreamTransport {
    pub(crate) fn new(api: YouTubeDataApi, access_token_source: TokenSource) -> Self {
        Self {
            api,
            access_token_source,
        }
    }

    pub(crate) async fn open(
        &self,
        live_chat_id: &str,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<ChatStream, StreamOutcome> {
        let token = (self.access_token_source)()
            .await
            .map_err(StreamOutcome::Transport)?;
        let message = encode_request(live_chat_id, page_token);
        let request = GrpcRequest::new(STREAM_LIST_PATH, message, MESSAGE_CAP_BYTES)
            .label(STREAM_LABEL)
            .without_limiter_wait()
            .bearer(Secret::new(token))
            .expect_refusal(ExpectedRefusal::status_with_excerpt(
                LIVE_CHAT_ENDED_STATUS,
                LIVE_CHAT_ENDED_REASON,
            ));
        match self
            .api
            .open_stream(STREAM_OPEN_COST, request, cancel.clone())
            .await
        {
            Ok(stream) => Ok(ChatStream {
                stream,
                resume_token: page_token.map(ToOwned::to_owned),
                pending_end: None,
                finished: false,
            }),
            Err(DataApiError::QuotaExhausted) => Err(StreamOutcome::QuotaExhausted),
            Err(DataApiError::Gateway(failure)) => Err(classify(failure)),
        }
    }
}

pub(crate) struct ChatStream {
    stream: GrpcStream,
    resume_token: Option<String>,
    pending_end: Option<StreamOutcome>,
    finished: bool,
}

impl ChatStream {
    pub(crate) fn resume_token(&self) -> Option<&str> {
        self.resume_token.as_deref()
    }

    pub(crate) async fn next(&mut self) -> Option<StreamEvent> {
        if self.finished {
            return None;
        }
        if let Some(outcome) = self.pending_end.take() {
            return Some(self.finish(outcome));
        }
        match self.stream.next().await {
            Some(GrpcItem::Message(bytes)) => Some(self.page_from(&bytes)),
            Some(GrpcItem::End(Ok(()))) => Some(self.finish(StreamOutcome::Closed)),
            Some(GrpcItem::End(Err(failure))) => Some(self.finish(classify(failure))),
            None => {
                self.finished = true;
                None
            }
        }
    }

    fn finish(&mut self, outcome: StreamOutcome) -> StreamEvent {
        self.finished = true;
        StreamEvent::Ended(outcome)
    }

    fn page_from(&mut self, bytes: &[u8]) -> StreamEvent {
        let response = match LiveChatMessageListResponse::decode(bytes) {
            Ok(response) => response,
            Err(error) => {
                return self.finish(StreamOutcome::Transport(malformed(error)));
            }
        };
        let body = transcode_response(&response);
        let page = page_from_body(&body);
        if let Some(token) = page
            .next_page_token
            .as_ref()
            .filter(|token| !token.is_empty())
        {
            self.resume_token = Some(token.clone());
        }
        if chat_has_ended(&body, &page) {
            self.pending_end = Some(StreamOutcome::ChatEnded);
        }
        StreamEvent::Page(page)
    }
}

fn encode_request(live_chat_id: &str, page_token: Option<&str>) -> Vec<u8> {
    LiveChatMessageListRequest {
        live_chat_id: Some(live_chat_id.to_owned()),
        part: REQUESTED_PARTS.map(str::to_owned).to_vec(),
        page_token: page_token.map(ToOwned::to_owned),
        ..Default::default()
    }
    .encode_to_vec()
}

fn chat_has_ended(body: &Value, page: &ChatMessagesResponse) -> bool {
    let offline = body
        .get("offlineAt")
        .and_then(Value::as_str)
        .is_some_and(|at| !at.is_empty());
    let ended_item = page.items.iter().any(|item| {
        item.get("snippet")
            .and_then(|snippet| snippet.get("type"))
            .and_then(Value::as_str)
            == Some(CHAT_ENDED_EVENT_TYPE)
    });
    let empty_token = page.next_page_token.as_deref().is_none_or(str::is_empty);
    offline || ended_item || empty_token
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| *c != '_' && *c != ' ' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn names(excerpt: &forge_net::BodyExcerpt, marker: &str) -> bool {
    excerpt
        .reason()
        .is_some_and(|reason| normalized(reason) == marker)
        || normalized(excerpt.as_str()).contains(marker)
}

fn classify(failure: GatewayFailure) -> StreamOutcome {
    match failure {
        GatewayFailure::Cancelled => StreamOutcome::Cancelled,
        GatewayFailure::Unauthorized { .. } => StreamOutcome::Unauthorized,
        GatewayFailure::ProtocolNotOffered { .. } => StreamOutcome::ProtocolNotOffered,
        GatewayFailure::RateLimited {
            retry_after,
            excerpt,
            ..
        } => {
            if names(&excerpt, QUOTA_REASON) && !names(&excerpt, RATE_REASON) {
                StreamOutcome::QuotaExhausted
            } else {
                StreamOutcome::RateLimited { retry_after }
            }
        }
        GatewayFailure::BudgetExhausted { retry_after } => StreamOutcome::RateLimited {
            retry_after: retry_after.unwrap_or(DEFAULT_RETRY_AFTER),
        },
        GatewayFailure::Forbidden { excerpt } if names(&excerpt, LIVE_CHAT_ENDED_NORMALIZED) => {
            StreamOutcome::ChatEnded
        }
        GatewayFailure::Forbidden { excerpt } if names(&excerpt, QUOTA_REASON) => {
            StreamOutcome::QuotaExhausted
        }
        other => StreamOutcome::Transport(
            DataApiError::Gateway(other).into_platform_error(MISSING_SCOPE),
        ),
    }
}
