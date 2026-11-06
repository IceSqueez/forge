use std::sync::Arc;
use std::time::Duration;

use forge_events::{RequestFailed, TypedEvent};
use forge_platform_core::{PlatformError, TokenSource};
use prost::Message;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::api::quota_state::SharedQuota;
use crate::chat::chat_poller::fetch_chat_page;
use crate::endpoints_test_support::{
    CapturingPublisher, GrpcStep, GrpcStub, HANG_GUARD, StubReply, StubServer, UNREACHABLE_HTTP,
    data_api_streaming, exhaust_quota, grpc_frame, grpc_head, grpc_ok_trailers, grpc_status,
    grpc_status_with_message, grpc_status_with_reason, grpc_status_with_retry, grpc_trailers_only,
    used_today,
};
use crate::stream_list_proto::live_chat_message_snippet::type_wrapper::Type;
use crate::stream_list_proto::{
    LiveChatMessage, LiveChatMessageListRequest, LiveChatMessageSnippet,
};

const TOKEN: &str = "ya29.stream-secret-7f3a";
const CHAT: &str = "lc-stream-1";
const RESUME: &str = "resume-9";
const UNAUTHENTICATED: &str = "16";
const UNIMPLEMENTED: &str = "12";
const RESOURCE_EXHAUSTED: &str = "8";
const PERMISSION_DENIED: &str = "7";
const INTERNAL: &str = "13";
const MESSAGES_PATH: &str = "/liveChat/messages";
const LONG_RETRY_AFTER: &str = "3600";
const RETRY_HINT: Duration = Duration::from_secs(7);
const TRUNCATED_FIELD: [u8; 3] = [0x0a, 0x05, 0x61];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Quota,
    Rate,
    Unauthorized,
    NotOffered,
    ChatEnded,
    Closed,
    Cancelled,
    Transport,
}

fn ending(outcome: &StreamOutcome) -> Ending {
    match outcome {
        StreamOutcome::QuotaExhausted => Ending::Quota,
        StreamOutcome::RateLimited { .. } => Ending::Rate,
        StreamOutcome::Unauthorized => Ending::Unauthorized,
        StreamOutcome::ProtocolNotOffered => Ending::NotOffered,
        StreamOutcome::ChatEnded => Ending::ChatEnded,
        StreamOutcome::Closed => Ending::Closed,
        StreamOutcome::Cancelled => Ending::Cancelled,
        StreamOutcome::Transport(_) => Ending::Transport,
    }
}

#[derive(Debug, PartialEq)]
enum Observed {
    Page(Option<String>, Vec<String>),
    Ended(Ending),
}

fn observed(event: &StreamEvent) -> Observed {
    match event {
        StreamEvent::Page(page) => Observed::Page(
            page.next_page_token.clone(),
            page.items
                .iter()
                .filter_map(|item| item["id"].as_str().map(ToOwned::to_owned))
                .collect(),
        ),
        StreamEvent::Ended(outcome) => Observed::Ended(ending(outcome)),
    }
}

fn token_source() -> TokenSource {
    Arc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) }))
}

fn refusing_token_source() -> TokenSource {
    Arc::new(|| {
        Box::pin(async {
            Err(PlatformError::ReauthRequired {
                platform: "youtube".to_owned(),
            })
        })
    })
}

fn text_message(id: &str) -> LiveChatMessage {
    LiveChatMessage {
        id: Some(id.to_owned()),
        snippet: Some(LiveChatMessageSnippet {
            r#type: Some(Type::TextMessageEvent as i32),
            display_message: Some("hi".to_owned()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn response(token: Option<&str>, items: Vec<LiveChatMessage>) -> LiveChatMessageListResponse {
    LiveChatMessageListResponse {
        next_page_token: token.map(ToOwned::to_owned),
        items,
        ..Default::default()
    }
}

fn page_frame(token: &str, ids: &[&str]) -> GrpcStep {
    grpc_frame(
        &response(Some(token), ids.iter().map(|id| text_message(id)).collect()).encode_to_vec(),
    )
}

struct Rig {
    grpc: GrpcStub,
    quota: SharedQuota,
    bus: Arc<CapturingPublisher>,
    api: YouTubeDataApi,
    transport: ChatStreamTransport,
}

async fn rig_with(grpc: GrpcStub, rest_base: &str, tokens: TokenSource) -> Rig {
    let quota = SharedQuota::default();
    let bus = Arc::new(CapturingPublisher::default());
    let api = data_api_streaming(rest_base, &grpc.uri(), quota.clone(), bus.clone());
    let transport = ChatStreamTransport::new(api.clone(), tokens);
    Rig {
        grpc,
        quota,
        bus,
        api,
        transport,
    }
}

async fn rig(steps: Vec<GrpcStep>) -> Rig {
    rig_with(
        GrpcStub::serving(steps).await,
        UNREACHABLE_HTTP,
        token_source(),
    )
    .await
}

impl Rig {
    async fn open(&self, page_token: Option<&str>) -> Result<ChatStream, StreamOutcome> {
        tokio::time::timeout(
            HANG_GUARD,
            self.transport
                .open(CHAT, page_token, &CancellationToken::new()),
        )
        .await
        .unwrap()
    }

    async fn opened(&self) -> ChatStream {
        match self.open(None).await {
            Ok(stream) => stream,
            Err(outcome) => panic!("expected the stream to open, got {outcome:?}"),
        }
    }
}

async fn next_of(stream: &mut ChatStream) -> Option<StreamEvent> {
    tokio::time::timeout(HANG_GUARD, stream.next())
        .await
        .unwrap()
}

async fn drain(stream: &mut ChatStream) -> Vec<Observed> {
    let mut seen = Vec::new();
    while let Some(event) = next_of(stream).await {
        seen.push(observed(&event));
    }
    seen
}

async fn first_ending(steps: Vec<GrpcStep>) -> Ending {
    let rig = rig(steps).await;
    match rig.open(None).await {
        Err(outcome) => ending(&outcome),
        Ok(mut stream) => match next_of(&mut stream).await {
            Some(StreamEvent::Ended(outcome)) => ending(&outcome),
            Some(StreamEvent::Page(_)) => panic!("expected an ending, got a page"),
            None => panic!("expected an ending, got nothing"),
        },
    }
}

async fn ended_outcome(steps: Vec<GrpcStep>) -> StreamOutcome {
    let rig = rig(steps).await;
    let mut stream = rig.opened().await;
    match next_of(&mut stream).await {
        Some(StreamEvent::Ended(outcome)) => outcome,
        _ => panic!("expected the stream to end"),
    }
}

#[tokio::test]
async fn the_request_names_the_chat_the_parts_and_the_resume_token_on_the_stream_list_path() {
    for page_token in [Some(RESUME), None] {
        let rig = rig(vec![grpc_head(), GrpcStep::Hold]).await;

        let _stream = rig.open(page_token).await;

        let [seen] = rig.grpc.seen().try_into().unwrap();
        let request = LiveChatMessageListRequest::decode(seen.message()).unwrap();
        assert_eq!(
            (
                seen.path.as_str(),
                seen.header("authorization"),
                request.live_chat_id.as_deref(),
                request.part,
                request.page_token.as_deref(),
            ),
            (
                STREAM_LIST_PATH,
                Some(format!("Bearer {TOKEN}").as_str()),
                Some(CHAT),
                vec![
                    "id".to_owned(),
                    "snippet".to_owned(),
                    "authorDetails".to_owned()
                ],
                page_token,
            ),
            "{page_token:?}"
        );
    }
}

#[tokio::test]
async fn a_failed_stream_reports_request_fail_under_the_stream_list_label() {
    let rig = rig(vec![grpc_trailers_only(grpc_status(INTERNAL))]).await;
    let mut stream = rig.opened().await;

    drain(&mut stream).await;

    let events = rig.bus.of_kind(RequestFailed::KIND);
    let [event] = events.as_slice() else {
        panic!("expected one request.fail, got {events:?}");
    };
    assert_eq!(event.payload["endpoint"], json!(STREAM_LABEL));
}

#[tokio::test]
async fn an_open_that_reached_google_stays_charged_whatever_the_answer() {
    for (label, steps) in [
        ("stream", vec![grpc_head(), GrpcStep::Hold]),
        (
            "call failure",
            vec![grpc_trailers_only(grpc_status(INTERNAL))],
        ),
        (
            "server error",
            vec![GrpcStep::Head {
                status: reqwest::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                headers: vec![("content-type", "text/plain".to_owned())],
                end: true,
            }],
        ),
    ] {
        let rig = rig(steps).await;

        let _outcome = rig.open(None).await;

        assert_eq!(
            (used_today(&rig.quota).await, rig.grpc.seen().len()),
            (STREAM_OPEN_COST, 1),
            "{label}"
        );
    }
}

#[tokio::test]
async fn an_open_cancelled_before_sending_is_refunded_and_sends_nothing() {
    let rig = rig(vec![grpc_head(), GrpcStep::Hold]).await;
    let cancel = CancellationToken::new();
    cancel.cancel();

    let outcome = rig.transport.open(CHAT, None, &cancel).await;

    assert!(
        matches!(outcome, Err(StreamOutcome::Cancelled)),
        "{:?}",
        outcome.err()
    );
    assert_eq!(
        (used_today(&rig.quota).await, rig.grpc.seen().len()),
        (0, 0)
    );
}

async fn throttled_rig() -> Rig {
    let rest = StubServer::start().await;
    rest.reply_always(
        MESSAGES_PATH,
        StubReply::status(reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16())
            .header("retry-after", LONG_RETRY_AFTER),
    );
    let rig = rig_with(
        GrpcStub::serving(vec![grpc_head(), GrpcStep::Hold]).await,
        &rest.uri(),
        token_source(),
    )
    .await;
    let throttled = fetch_chat_page(&rig.api, TOKEN, CHAT, None, &CancellationToken::new()).await;
    assert!(throttled.is_err());
    rig
}

#[tokio::test]
async fn an_open_refused_by_the_local_budget_is_refunded_and_sends_nothing() {
    let rig = throttled_rig().await;
    let charged_before = used_today(&rig.quota).await;

    let outcome = rig.open(None).await;

    assert!(
        matches!(outcome, Err(StreamOutcome::RateLimited { .. })),
        "{:?}",
        outcome.err()
    );
    assert_eq!(
        (used_today(&rig.quota).await, rig.grpc.seen().len()),
        (charged_before, 0)
    );
}

#[tokio::test]
async fn an_open_refused_by_the_local_budget_waits_out_the_remaining_throttle() {
    let rig = throttled_rig().await;

    let outcome = rig.open(None).await;

    assert!(
        matches!(
            outcome,
            Err(StreamOutcome::RateLimited { retry_after }) if retry_after > DEFAULT_RETRY_AFTER
        ),
        "{:?}",
        outcome.err()
    );
}

#[tokio::test]
async fn a_spent_daily_quota_opens_nothing() {
    let rig = rig(vec![grpc_head(), GrpcStep::Hold]).await;
    exhaust_quota(&rig.quota).await;

    let outcome = rig.open(None).await;

    assert!(
        matches!(outcome, Err(StreamOutcome::QuotaExhausted)),
        "{:?}",
        outcome.err()
    );
    assert!(rig.grpc.seen().is_empty());
}

#[tokio::test]
async fn a_token_source_failure_opens_nothing_and_charges_nothing() {
    let rig = rig_with(
        GrpcStub::serving(vec![grpc_head(), GrpcStep::Hold]).await,
        UNREACHABLE_HTTP,
        refusing_token_source(),
    )
    .await;

    let outcome = rig.open(None).await;

    assert!(
        matches!(
            outcome,
            Err(StreamOutcome::Transport(
                PlatformError::ReauthRequired { .. }
            ))
        ),
        "{:?}",
        outcome.err()
    );
    assert_eq!(
        (used_today(&rig.quota).await, rig.grpc.seen().len()),
        (0, 0)
    );
}

#[tokio::test]
async fn every_call_outcome_classifies_to_its_stream_outcome() {
    let ended_with = |pairs| vec![grpc_head(), GrpcStep::Trailers(pairs)];
    let table = [
        (
            "clean end",
            vec![grpc_head(), grpc_ok_trailers()],
            Ending::Closed,
        ),
        (
            "unauthenticated",
            ended_with(grpc_status(UNAUTHENTICATED)),
            Ending::Unauthorized,
        ),
        (
            "unimplemented",
            ended_with(grpc_status(UNIMPLEMENTED)),
            Ending::NotOffered,
        ),
        (
            "not a grpc answer",
            vec![GrpcStep::Head {
                status: reqwest::StatusCode::NOT_FOUND.as_u16(),
                headers: vec![("content-type", "text/html".to_owned())],
                end: true,
            }],
            Ending::NotOffered,
        ),
        (
            "exhausted quotaExceeded",
            ended_with(grpc_status_with_reason(RESOURCE_EXHAUSTED, "quotaExceeded")),
            Ending::Quota,
        ),
        (
            "exhausted QUOTA_EXCEEDED",
            ended_with(grpc_status_with_reason(
                RESOURCE_EXHAUSTED,
                "QUOTA_EXCEEDED",
            )),
            Ending::Quota,
        ),
        (
            "exhausted rateLimitExceeded",
            ended_with(grpc_status_with_reason(
                RESOURCE_EXHAUSTED,
                "rateLimitExceeded",
            )),
            Ending::Rate,
        ),
        (
            "exhausted RATE_LIMIT_EXCEEDED",
            ended_with(grpc_status_with_reason(
                RESOURCE_EXHAUSTED,
                "RATE_LIMIT_EXCEEDED",
            )),
            Ending::Rate,
        ),
        (
            "exhausted without a reason",
            ended_with(grpc_status(RESOURCE_EXHAUSTED)),
            Ending::Rate,
        ),
        (
            "denied liveChatEnded",
            ended_with(grpc_status_with_reason(PERMISSION_DENIED, "liveChatEnded")),
            Ending::ChatEnded,
        ),
        (
            "denied LIVE_CHAT_ENDED",
            ended_with(grpc_status_with_reason(
                PERMISSION_DENIED,
                "LIVE_CHAT_ENDED",
            )),
            Ending::ChatEnded,
        ),
        (
            "denied with liveChatEnded in the message",
            ended_with(grpc_status_with_message(PERMISSION_DENIED, "liveChatEnded")),
            Ending::ChatEnded,
        ),
        (
            "denied quotaExceeded",
            ended_with(grpc_status_with_reason(PERMISSION_DENIED, "quotaExceeded")),
            Ending::Quota,
        ),
        (
            "denied QUOTA_EXCEEDED",
            ended_with(grpc_status_with_reason(PERMISSION_DENIED, "QUOTA_EXCEEDED")),
            Ending::Quota,
        ),
        (
            "denied without a reason",
            ended_with(grpc_status(PERMISSION_DENIED)),
            Ending::Transport,
        ),
        (
            "internal",
            ended_with(grpc_status(INTERNAL)),
            Ending::Transport,
        ),
    ];
    for (label, steps, expected) in table {
        assert_eq!(first_ending(steps).await, expected, "{label}");
    }
}

#[tokio::test]
async fn a_retry_hint_on_a_throttled_stream_is_carried_as_the_wait() {
    let outcome = ended_outcome(vec![
        grpc_head(),
        GrpcStep::Trailers(grpc_status_with_retry(RESOURCE_EXHAUSTED, RETRY_HINT)),
    ])
    .await;

    assert!(
        matches!(outcome, StreamOutcome::RateLimited { retry_after } if retry_after == RETRY_HINT),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn cancelling_an_open_stream_ends_it_as_cancelled() {
    let rig = rig(vec![grpc_head(), GrpcStep::Hold]).await;
    let cancel = CancellationToken::new();
    let mut stream = tokio::time::timeout(HANG_GUARD, rig.transport.open(CHAT, None, &cancel))
        .await
        .unwrap()
        .ok()
        .unwrap();

    cancel.cancel();

    assert_eq!(
        drain(&mut stream).await,
        [Observed::Ended(Ending::Cancelled)]
    );
}

#[tokio::test]
async fn pages_arrive_in_order_then_one_closed_then_nothing() {
    let rig = rig(vec![
        grpc_head(),
        page_frame("tok-a", &["m-1"]),
        page_frame("tok-b", &["m-2", "m-3"]),
        grpc_ok_trailers(),
    ])
    .await;
    let mut stream = rig.opened().await;

    let seen = drain(&mut stream).await;

    assert_eq!(
        seen,
        [
            Observed::Page(Some("tok-a".to_owned()), vec!["m-1".to_owned()]),
            Observed::Page(
                Some("tok-b".to_owned()),
                vec!["m-2".to_owned(), "m-3".to_owned()]
            ),
            Observed::Ended(Ending::Closed),
        ]
    );
    assert!(next_of(&mut stream).await.is_none());
}

#[tokio::test]
async fn a_page_that_ends_the_chat_is_delivered_then_chat_ended_then_nothing() {
    let ended_item = LiveChatMessage {
        id: Some("end-1".to_owned()),
        snippet: Some(LiveChatMessageSnippet {
            r#type: Some(Type::ChatEndedEvent as i32),
            ..Default::default()
        }),
        ..Default::default()
    };
    let offline = LiveChatMessageListResponse {
        offline_at: Some("2026-11-06T10:00:00Z".to_owned()),
        ..response(Some("tok-x"), vec![text_message("m-1")])
    };
    for (label, frame, expected_page) in [
        (
            "offline",
            offline,
            Observed::Page(Some("tok-x".to_owned()), vec!["m-1".to_owned()]),
        ),
        (
            "chat ended event",
            response(Some("tok-x"), vec![ended_item]),
            Observed::Page(Some("tok-x".to_owned()), vec!["end-1".to_owned()]),
        ),
        (
            "empty next token",
            response(Some(""), vec![text_message("m-1")]),
            Observed::Page(Some(String::new()), vec!["m-1".to_owned()]),
        ),
        (
            "no next token",
            response(None, vec![text_message("m-1")]),
            Observed::Page(None, vec!["m-1".to_owned()]),
        ),
    ] {
        let rig = rig(vec![
            grpc_head(),
            grpc_frame(&frame.encode_to_vec()),
            GrpcStep::Hold,
        ])
        .await;
        let mut stream = rig.opened().await;

        let seen = drain(&mut stream).await;

        assert_eq!(
            seen,
            [expected_page, Observed::Ended(Ending::ChatEnded)],
            "{label}"
        );
    }
}

#[tokio::test]
async fn a_live_page_does_not_end_the_chat() {
    let rig = rig(vec![
        grpc_head(),
        page_frame("tok-a", &["m-1"]),
        GrpcStep::Hold,
    ])
    .await;
    let mut stream = rig.opened().await;

    let first = next_of(&mut stream).await.map(|event| observed(&event));
    let second = tokio::time::timeout(Duration::from_millis(20), stream.next()).await;

    assert_eq!(
        first,
        Some(Observed::Page(
            Some("tok-a".to_owned()),
            vec!["m-1".to_owned()]
        ))
    );
    assert!(second.is_err());
}

#[tokio::test]
async fn the_resume_token_follows_the_latest_page_token() {
    let rig = rig(vec![
        grpc_head(),
        page_frame("tok-a", &["m-1"]),
        page_frame("tok-b", &["m-2"]),
        GrpcStep::Hold,
    ])
    .await;
    let mut stream = rig.open(Some(RESUME)).await.ok().unwrap();
    let before = stream.resume_token().map(ToOwned::to_owned);

    next_of(&mut stream).await;
    let after_first = stream.resume_token().map(ToOwned::to_owned);
    next_of(&mut stream).await;

    assert_eq!(
        (
            before.as_deref(),
            after_first.as_deref(),
            stream.resume_token()
        ),
        (Some(RESUME), Some("tok-a"), Some("tok-b"))
    );
}

#[tokio::test]
async fn an_empty_page_token_keeps_the_last_real_resume_token() {
    let rig = rig(vec![
        grpc_head(),
        page_frame("tok-a", &["m-1"]),
        page_frame("", &["m-2"]),
        GrpcStep::Hold,
    ])
    .await;
    let mut stream = rig.opened().await;

    next_of(&mut stream).await;
    next_of(&mut stream).await;

    assert_eq!(stream.resume_token(), Some("tok-a"));
}

#[tokio::test]
async fn undecodable_message_bytes_end_the_stream_as_malformed() {
    let rig = rig(vec![
        grpc_head(),
        grpc_frame(&TRUNCATED_FIELD),
        GrpcStep::Hold,
    ])
    .await;
    let mut stream = rig.opened().await;

    let first = next_of(&mut stream).await;
    let after = next_of(&mut stream).await;

    assert!(
        matches!(
            first,
            Some(StreamEvent::Ended(StreamOutcome::Transport(
                PlatformError::MalformedResponse { .. }
            )))
        ),
        "{:?}",
        first.map(|event| observed(&event))
    );
    assert!(after.is_none());
}

#[tokio::test]
async fn an_echoed_bearer_never_reaches_the_outcome() {
    let outcome = ended_outcome(vec![
        grpc_head(),
        GrpcStep::Trailers(grpc_status_with_message(
            INTERNAL,
            &format!("token%20{TOKEN}%20rejected"),
        )),
    ])
    .await;

    let StreamOutcome::Transport(error) = &outcome else {
        panic!("expected a transport failure, got {outcome:?}");
    };
    assert!(
        !format!("{outcome:?}").contains(TOKEN) && !error.to_string().contains(TOKEN),
        "{error}"
    );
}
