#![allow(clippy::unwrap_used)]
mod support;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_net::TokenBucketRateLimiter;
use forge_platform_core::{ChatPlatform, EndpointSurface, PlatformEndpoints, ViewerReport};
use forge_platform_youtube::{
    ActiveBroadcastIdHandle, GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW,
    GoogleOAuthClient, LiveChatIdHandle, SharedQuota, YoutubeAdBreak, YoutubeAuthBundle,
    YoutubeChannelLookup, YoutubeCredentialsManager, YoutubeDataApi, YoutubeModeration,
    YoutubePlatform, YoutubeSendChat, YoutubeStreamMetadata, YoutubeStreamStats, YoutubeThumbnail,
    YoutubeViewerPoll, google_oauth_gateway,
};
use forge_storage::ban_ledger::MockBanLedgerRepo;
use forge_types::OAuthToken;
use serde_json::json;
use support::{
    BroadcasterSource, MemoryRepo, SilentBus, TokenSource, data_api_override, endpoints,
    fixed_broadcaster, fixed_token,
};
use tokio::sync::watch;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const STEP_DEADLINE: Duration = Duration::from_secs(30);
const BROADCAST_ID: &str = "broadcast-override";
const LIVE_CHAT_ID: &str = "live-chat-override";

fn broadcaster_source() -> BroadcasterSource {
    fixed_broadcaster("UCoverride", "Override")
}

fn token_source() -> TokenSource {
    fixed_token("override-token")
}

fn api(endpoints: &PlatformEndpoints) -> YoutubeDataApi {
    YoutubeDataApi::new(endpoints, SharedQuota::default(), Arc::new(SilentBus)).unwrap()
}

fn live_chat() -> LiveChatIdHandle {
    let handle = LiveChatIdHandle::new();
    handle.set(Some(LIVE_CHAT_ID.to_owned()));
    handle
}

fn active_broadcast() -> ActiveBroadcastIdHandle {
    let handle = ActiveBroadcastIdHandle::new();
    handle.set(Some(BROADCAST_ID.to_owned()));
    handle
}

fn bundle(expires_at: SystemTime, channel_handle: Option<&str>) -> YoutubeAuthBundle {
    YoutubeAuthBundle {
        access_token: OAuthToken::new("stale-access"),
        refresh_token: OAuthToken::new("stale-refresh"),
        channel_id: "UCoverride".to_owned(),
        channel_title: "Override Channel".to_owned(),
        channel_handle: channel_handle.map(ToOwned::to_owned),
        client_id: "client".to_owned(),
        expires_at,
    }
}

async fn manager_holding(
    endpoints: &PlatformEndpoints,
    saved: YoutubeAuthBundle,
) -> YoutubeCredentialsManager {
    let oauth = GoogleOAuthClient::new(
        google_oauth_gateway(
            endpoints,
            Arc::new(TokenBucketRateLimiter::new(
                GOOGLE_OAUTH_BUDGET_CAPACITY,
                GOOGLE_OAUTH_BUDGET_WINDOW,
            )),
        )
        .unwrap(),
        "client".to_owned(),
        forge_types::Secret::new("secret"),
    );
    let manager =
        YoutubeCredentialsManager::new(Arc::new(MemoryRepo::default()), oauth, api(endpoints));
    manager.save_from_bundle(saved).await.unwrap();
    manager
}

fn one_hour_from_now() -> SystemTime {
    SystemTime::now() + Duration::from_secs(3600)
}

async fn received_routes(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect()
}

fn routes(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(verb, route)| ((*verb).to_owned(), (*route).to_owned()))
        .collect()
}

#[tokio::test]
async fn data_api_override_carries_every_rest_client_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&server)
        .await;
    let endpoints = data_api_override(&server);

    YoutubeSendChat::new(api(&endpoints), token_source(), live_chat())
        .send("hi")
        .await
        .ok();
    YoutubeModeration::new(
        api(&endpoints),
        token_source(),
        broadcaster_source(),
        live_chat(),
        Arc::new(MockBanLedgerRepo::new()),
    )
    .add_moderator("UCviewer")
    .await
    .ok();
    YoutubeStreamMetadata::new(api(&endpoints), token_source(), active_broadcast())
        .set_title("New title")
        .await
        .ok();
    YoutubeStreamStats::new(api(&endpoints), token_source(), active_broadcast())
        .fetch()
        .await
        .ok();
    YoutubeAdBreak::new(api(&endpoints), token_source(), active_broadcast())
        .insert_cuepoint(30)
        .await
        .ok();
    YoutubeChannelLookup::new(api(&endpoints), token_source())
        .lookup("@viewer")
        .await
        .ok();

    assert_eq!(
        received_routes(&server).await,
        routes(&[
            ("POST", "/youtube/v3/liveChat/messages"),
            ("POST", "/youtube/v3/liveChat/moderators"),
            ("GET", "/youtube/v3/videos"),
            ("GET", "/youtube/v3/videos"),
            ("POST", "/youtube/v3/liveBroadcasts/cuepoint"),
            ("GET", "/youtube/v3/channels"),
        ])
    );
}

#[tokio::test]
async fn upload_api_override_carries_the_thumbnail_upload_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&server)
        .await;
    let endpoints = endpoints(&[(
        EndpointSurface::YouTubeUploadApi,
        format!("{}/upload/youtube/v3", server.uri()),
    )]);
    let image = std::env::temp_dir().join(format!(
        "forge-youtube-endpoint-thumbnail-{}.png",
        std::process::id()
    ));
    std::fs::write(&image, b"\x89PNG\r\n\x1a\n").unwrap();

    let result = YoutubeThumbnail::new(api(&endpoints), token_source(), active_broadcast())
        .set(image.to_str().unwrap())
        .await;
    std::fs::remove_file(&image).unwrap();

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        received_routes(&server).await,
        routes(&[("POST", "/upload/youtube/v3/thumbnails/set")])
    );
}

#[tokio::test]
async fn oauth_override_carries_the_token_refresh_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/google-oauth/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=stale-refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fresh-access",
            "expires_in": 3599,
            "token_type": "Bearer"
        })))
        .mount(&server)
        .await;
    let endpoints = endpoints(&[(
        EndpointSurface::YouTubeOAuth,
        format!("{}/google-oauth", server.uri()),
    )]);
    let manager = manager_holding(
        &endpoints,
        bundle(UNIX_EPOCH + Duration::from_secs(60), Some("@override")),
    )
    .await;

    let token = manager.get_valid_access_token().await.unwrap();

    assert_eq!(token, "fresh-access");
}

#[tokio::test]
async fn data_api_override_carries_the_channel_handle_backfill_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/youtube/v3/channels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "id": "UCoverride",
                "snippet": { "title": "Override Channel", "customUrl": "@backfilled" }
            }]
        })))
        .mount(&server)
        .await;
    let manager = manager_holding(
        &data_api_override(&server),
        bundle(one_hour_from_now(), None),
    )
    .await;

    let creds = manager.ensure_channel_handle().await.unwrap();

    assert_eq!(creds.channel_handle.as_deref(), Some("@backfilled"));
}

#[tokio::test]
async fn data_api_override_carries_the_viewer_poll_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/youtube/v3/videos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "id": BROADCAST_ID,
                "liveStreamingDetails": { "concurrentViewers": "42" }
            }]
        })))
        .mount(&server)
        .await;
    let (reports_tx, mut reports_rx) = watch::channel(ViewerReport::Absent);
    let poll = YoutubeViewerPoll::new(
        api(&data_api_override(&server)),
        token_source(),
        active_broadcast(),
        reports_tx,
    );

    let task = tokio::spawn(poll.run());
    tokio::time::timeout(STEP_DEADLINE, reports_rx.changed())
        .await
        .unwrap()
        .unwrap();
    task.abort();

    assert_eq!(*reports_rx.borrow(), ViewerReport::Live { count: 42 });
}

#[tokio::test]
async fn platform_carries_its_override_into_the_chat_poller_and_the_sender() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/youtube/v3/liveBroadcasts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "id": BROADCAST_ID,
                "snippet": { "liveChatId": LIVE_CHAT_ID, "title": "Override Stream" }
            }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/youtube/v3/liveChat/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "pollingIntervalMillis": 3000,
            "items": [{
                "id": "msg-override",
                "snippet": { "type": "textMessageEvent", "displayMessage": "over here" },
                "authorDetails": { "displayName": "Viewer", "channelId": "UCviewer" }
            }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/youtube/v3/liveChat/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let endpoints = data_api_override(&server);
    let manager = manager_holding(&endpoints, bundle(one_hour_from_now(), Some("@override"))).await;
    let platform = YoutubePlatform::new(
        api(&endpoints),
        "UCoverride".to_owned(),
        Arc::new(manager),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        Arc::new(MockBanLedgerRepo::new()),
    );
    let mut events = platform.events();

    platform.connect().await.unwrap();
    tokio::time::timeout(STEP_DEADLINE, async {
        while events.recv().await.unwrap().kind != "youtube.chat.message" {}
    })
    .await
    .unwrap();
    platform.send_message("UCoverride", "reply").await.unwrap();
    platform.disconnect().await.unwrap();

    let posts: Vec<(String, String)> = received_routes(&server)
        .await
        .into_iter()
        .filter(|(verb, _)| verb == "POST")
        .collect();
    assert_eq!(posts, routes(&[("POST", "/youtube/v3/liveChat/messages")]));
}
