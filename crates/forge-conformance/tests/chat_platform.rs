#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::ffi::OsString;
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_events::test_support::NullPublisher;
use forge_net::RateLimiter;
use forge_net::test_support::RecordingRateLimiter;
use forge_platform_core::{
    ChatPlatform, ConnectionState, EndpointSurface, PlatformEndpoints, PlatformError,
    SenderPreference,
};
use forge_platform_kick::{
    ChannelInfoFetcher, KickCredentialsManager, KickOAuthClient, KickPlatform, KickSendChat,
    kick_api_gateway, kick_channel_info_gateway, kick_oauth_gateway,
};
use forge_platform_twitch::TwitchAssembly;
use forge_platform_youtube::{
    ActiveBroadcastIdHandle, GoogleOAuthClient, LiveChatIdHandle, SharedQuota,
    YoutubeCredentialsManager, YoutubeDataApi, YoutubePlatform, google_oauth_gateway,
};
use forge_registry::SubActionRegistry;
use forge_storage::test_support::MemoryCredentials;
use forge_storage::{BanLedgerRepo, CredentialsRepo, MockBanLedgerRepo};
use forge_types::Secret;

const UNREACHABLE_HTTP: &str = "http://127.0.0.1:0";
const UNREACHABLE_SOCKET: &str = "ws://127.0.0.1:0";
const CHANNEL: &str = "channel";
const TEXT: &str = "hello";
const PARENT_MESSAGE: &str = "parent";
const CLIENT_ID: &str = "client";

struct Subject {
    id: &'static str,
    platform: Arc<dyn ChatPlatform>,
}

fn chat_endpoint_surfaces() -> Vec<EndpointSurface> {
    [
        forge_platform_twitch::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_kick::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_youtube::endpoint_surfaces::ENDPOINT_SURFACES,
    ]
    .concat()
}

fn unreachable_endpoints() -> PlatformEndpoints {
    PlatformEndpoints::resolve(&chat_endpoint_surfaces(), |variable| {
        let url = if variable.contains("_WS_") {
            UNREACHABLE_SOCKET
        } else {
            UNREACHABLE_HTTP
        };
        Some(OsString::from(url))
    })
    .unwrap()
}

fn limiter() -> Arc<dyn RateLimiter> {
    Arc::new(RecordingRateLimiter::granting())
}

fn publisher() -> Arc<dyn EventPublisher> {
    Arc::new(NullPublisher)
}

fn ban_ledger() -> Arc<dyn BanLedgerRepo> {
    Arc::new(MockBanLedgerRepo::new())
}

async fn signed_out_twitch(endpoints: &PlatformEndpoints) -> Arc<dyn ChatPlatform> {
    let creds: Arc<dyn CredentialsRepo> = Arc::new(MemoryCredentials::default());
    let assembly = TwitchAssembly::new(
        &mut SubActionRegistry::new(),
        creds,
        &publisher(),
        endpoints,
        CLIENT_ID.to_owned(),
    )
    .unwrap();
    assembly.open_session().await.unwrap().chat_platform()
}

fn signed_out_youtube(endpoints: &PlatformEndpoints) -> Arc<dyn ChatPlatform> {
    let api = YoutubeDataApi::new(endpoints, SharedQuota::default(), publisher()).unwrap();
    let oauth = GoogleOAuthClient::new(
        google_oauth_gateway(endpoints, limiter()).unwrap(),
        CLIENT_ID.to_owned(),
        Secret::new("secret"),
    );
    let manager =
        YoutubeCredentialsManager::new(Arc::new(MemoryCredentials::default()), oauth, api.clone());
    Arc::new(YoutubePlatform::new(
        api,
        "UCconformance".to_owned(),
        Arc::new(manager),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        ban_ledger(),
    ))
}

fn signed_out_kick(endpoints: &PlatformEndpoints) -> Arc<dyn ChatPlatform> {
    let api = kick_api_gateway(endpoints, limiter(), publisher()).unwrap();
    let oauth = KickOAuthClient::new(
        endpoints,
        kick_oauth_gateway(endpoints, limiter()).unwrap(),
        CLIENT_ID.to_owned(),
        Secret::new("secret"),
    );
    let manager =
        KickCredentialsManager::new(Arc::new(MemoryCredentials::default()), oauth, api.clone());
    Arc::new(KickPlatform::new(
        endpoints,
        Arc::new(manager),
        Arc::new(KickSendChat::new(api)),
        Arc::new(ChannelInfoFetcher::new(
            kick_channel_info_gateway(endpoints, limiter(), publisher()).unwrap(),
        )),
        ban_ledger(),
    ))
}

async fn every_signed_out_platform() -> [Subject; 3] {
    let endpoints = unreachable_endpoints();
    [
        Subject {
            id: "twitch",
            platform: signed_out_twitch(&endpoints).await,
        },
        Subject {
            id: "youtube",
            platform: signed_out_youtube(&endpoints),
        },
        Subject {
            id: "kick",
            platform: signed_out_kick(&endpoints),
        },
    ]
}

fn assert_asks_for_sign_in<T: std::fmt::Debug>(
    subject: &Subject,
    call: &str,
    answer: Result<T, PlatformError>,
) {
    assert!(
        matches!(&answer, Err(PlatformError::ReauthRequired { platform }) if platform == subject.id),
        "{} {call}: {answer:?}",
        subject.id
    );
}

#[tokio::test]
async fn a_platform_starts_disconnected() {
    for subject in every_signed_out_platform().await {
        assert_eq!(
            subject.platform.connection_state(),
            ConnectionState::Disconnected,
            "{}",
            subject.id
        );
    }
}

#[tokio::test]
async fn disconnecting_a_platform_that_never_connected_succeeds_and_changes_nothing() {
    for subject in every_signed_out_platform().await {
        let first = subject.platform.disconnect().await;
        let second = subject.platform.disconnect().await;

        assert!(first.is_ok() && second.is_ok(), "{}", subject.id);
        assert_eq!(
            subject.platform.connection_state(),
            ConnectionState::Disconnected,
            "{}",
            subject.id
        );
    }
}

#[tokio::test]
async fn connecting_without_credentials_asks_for_sign_in_and_stays_disconnected() {
    for subject in every_signed_out_platform().await {
        let answer = subject.platform.connect().await;

        assert_asks_for_sign_in(&subject, "connect", answer);
        assert_eq!(
            subject.platform.connection_state(),
            ConnectionState::Disconnected,
            "{}",
            subject.id
        );
    }
}

#[tokio::test]
async fn sending_without_credentials_asks_for_sign_in() {
    for subject in every_signed_out_platform().await {
        let platform = &subject.platform;

        assert_asks_for_sign_in(
            &subject,
            "send_message",
            platform.send_message(CHANNEL, TEXT).await,
        );
        assert_asks_for_sign_in(
            &subject,
            "send_reply",
            platform.send_reply(CHANNEL, PARENT_MESSAGE, TEXT).await,
        );
        assert_asks_for_sign_in(
            &subject,
            "send_message_as",
            platform
                .send_message_as(CHANNEL, TEXT, SenderPreference::Default)
                .await,
        );
        assert_asks_for_sign_in(
            &subject,
            "send_reply_as",
            platform
                .send_reply_as(CHANNEL, PARENT_MESSAGE, TEXT, SenderPreference::Default)
                .await,
        );
    }
}
