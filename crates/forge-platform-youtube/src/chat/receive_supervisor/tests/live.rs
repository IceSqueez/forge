use forge_events::test_support::NullPublisher;
use forge_storage::test_support::MemoryCredentials;
use time::OffsetDateTime;

use super::*;
use crate::account::credentials::{YOUTUBE_CREDENTIAL_ID, YouTubeCredentials};
use crate::account::credentials_manager::YouTubeCredentialsManager;
use crate::api::quota_state::SharedQuota;
use crate::chat::chat_platform::token_source;
use crate::endpoints_test_support::{
    GrpcStep, GrpcStub, StubReply, StubServer, UNREACHABLE_HTTP, data_api_at, data_api_charging,
    data_api_streaming, exhaust_quota, grpc_head, grpc_status, grpc_trailers_only, oauth_client_at,
    wait_until,
};

const STALE_ACCESS: &str = "ya29.stale-access";
const FRESH_ACCESS: &str = "ya29.fresh-access";
const TOKEN_PATH: &str = "/token";
const MESSAGES_PATH: &str = "/liveChat/messages";
const UNAUTHENTICATED: &str = "16";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polled {
    Page,
    Quota,
    Rate(Duration),
    Unauthorized,
    ChatEnded,
    Transport,
    Other,
}

fn polled(outcome: &Result<ChatMessagesResponse, StreamOutcome>) -> Polled {
    match outcome {
        Ok(_) => Polled::Page,
        Err(StreamOutcome::QuotaExhausted) => Polled::Quota,
        Err(StreamOutcome::RateLimited { retry_after }) => Polled::Rate(*retry_after),
        Err(StreamOutcome::Unauthorized) => Polled::Unauthorized,
        Err(StreamOutcome::ChatEnded) => Polled::ChatEnded,
        Err(StreamOutcome::Transport(_)) => Polled::Transport,
        Err(_) => Polled::Other,
    }
}

fn fixed_token() -> TokenSource {
    Arc::new(|| Box::pin(async { Ok(STALE_ACCESS.to_owned()) }))
}

fn google_refusal(status: reqwest::StatusCode, reason: &str) -> StubReply {
    StubReply::json(
        status.as_u16(),
        &json!({ "error": { "code": status.as_u16(), "errors": [{ "reason": reason }] } }),
    )
}

async fn poll_through(api: YouTubeDataApi, tokens: TokenSource) -> Polled {
    let outcome = LiveTransports::new(api, tokens)
        .poll_page(CHAT, None, &CancellationToken::new())
        .await;
    polled(&outcome)
}

#[tokio::test]
async fn every_poll_answer_maps_to_its_receive_outcome() {
    for (label, reply, expected) in [
        (
            "page",
            StubReply::ok(&json!({ "items": [], "nextPageToken": "p-1" })),
            Polled::Page,
        ),
        (
            "unauthorized",
            StubReply::status(reqwest::StatusCode::UNAUTHORIZED.as_u16()),
            Polled::Unauthorized,
        ),
        (
            "throttled",
            StubReply::status(reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16())
                .header("retry-after", "7"),
            Polled::Rate(Duration::from_secs(7)),
        ),
        (
            "chat ended",
            google_refusal(reqwest::StatusCode::FORBIDDEN, "liveChatEnded"),
            Polled::ChatEnded,
        ),
        (
            "server error",
            StubReply::status(reqwest::StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
            Polled::Transport,
        ),
    ] {
        let server = StubServer::start().await;
        server.reply_always(MESSAGES_PATH, reply);

        let outcome = poll_through(data_api_at(&server.uri()), fixed_token()).await;

        assert_eq!(outcome, expected, "{label}");
    }
}

#[tokio::test]
async fn a_poll_google_refuses_for_quota_takes_the_quota_path() {
    let server = StubServer::start().await;
    server.reply_always(
        MESSAGES_PATH,
        google_refusal(reqwest::StatusCode::FORBIDDEN, "quotaExceeded"),
    );

    let outcome = poll_through(data_api_at(&server.uri()), fixed_token()).await;

    assert_eq!(outcome, Polled::Quota);
}

#[tokio::test]
async fn a_poll_without_an_access_token_or_quota_sends_nothing() {
    let refusing: TokenSource = Arc::new(|| {
        Box::pin(async {
            Err(PlatformError::ReauthRequired {
                platform: "youtube".to_owned(),
            })
        })
    });
    let spent = SharedQuota::default();
    exhaust_quota(&spent).await;
    for (label, quota, tokens, expected) in [
        ("token", SharedQuota::default(), refusing, Polled::Transport),
        ("quota", spent, fixed_token(), Polled::Quota),
    ] {
        let server = StubServer::start().await;
        server.reply_always(MESSAGES_PATH, StubReply::ok(&json!({ "items": [] })));

        let outcome = poll_through(data_api_charging(&server.uri(), quota), tokens).await;

        assert_eq!(
            (outcome, server.request_count(MESSAGES_PATH)),
            (expected, 0),
            "{label}"
        );
    }
}

fn stored(access: &str) -> String {
    serde_json::to_string(&YouTubeCredentials {
        access_token: forge_types::OAuthToken::new(access),
        refresh_token: forge_types::OAuthToken::new("1//refresh"),
        client_id: "client".to_owned(),
        channel_id: "UCbroadcaster".to_owned(),
        channel_title: "Broadcaster".to_owned(),
        channel_handle: None,
        expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
    })
    .unwrap()
}

#[tokio::test]
async fn an_unauthorized_stream_reopens_with_a_refreshed_access_token() {
    let oauth = StubServer::start().await;
    oauth.reply_always(
        TOKEN_PATH,
        StubReply::ok(&json!({
            "access_token": FRESH_ACCESS,
            "expires_in": 3599,
            "token_type": "Bearer"
        })),
    );
    let stale_bearer = format!("Bearer {STALE_ACCESS}");
    let grpc = GrpcStub::scripted(move |seen| {
        if seen.header("authorization") == Some(stale_bearer.as_str()) {
            vec![grpc_trailers_only(grpc_status(UNAUTHENTICATED))]
        } else {
            vec![grpc_head(), GrpcStep::Hold]
        }
    })
    .await;
    let manager = Arc::new(YouTubeCredentialsManager::new(
        Arc::new(MemoryCredentials::holding(
            YOUTUBE_CREDENTIAL_ID,
            &stored(STALE_ACCESS),
        )),
        oauth_client_at(&oauth.uri()),
        unreachable_data_api(),
    ));
    let api = data_api_streaming(
        UNREACHABLE_HTTP,
        &grpc.uri(),
        SharedQuota::default(),
        Arc::new(NullPublisher),
    );
    let mut supervisor = ReceiveSupervisor::new(
        LiveTransports::new(api, token_source(manager)),
        ReceiveTimings::DEFAULT,
    );
    let mut rig = rig();
    let cancel = rig.cancel.clone();
    let opens = grpc.seen_counter();

    let (_, reopened) = tokio::join!(rig.receive(&mut supervisor), async {
        let reopened = wait_until(|| opens() >= 2).await;
        cancel.cancel();
        reopened
    });

    let seen = grpc.seen();
    assert!(reopened);
    assert_eq!(
        seen[1].header("authorization"),
        Some(format!("Bearer {FRESH_ACCESS}").as_str())
    );
}
