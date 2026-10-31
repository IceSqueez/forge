#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use std::path::Path;
use std::sync::Arc;

use forge_platform_youtube::{LiveChatIdHandle, SharedQuota, YoutubeDataApi, YoutubeModeration};
use forge_storage::{BanLedgerKey, DataProvider};
use forge_storage_sqlite::SqliteBackend;
use forge_types::PlatformId;
use support::{data_api_override, fixed_broadcaster, fixed_token};
use time::OffsetDateTime;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BANS_ROUTE: &str = "/youtube/v3/liveChat/bans";
const BAN_ID: &str = "ban-resource-restart";
const BROADCASTER: &str = "UCbroadcaster";
const VIEWER: &str = "UCviewer";
const TEST_KEY: [u8; 32] = [0x5a; 32];

async fn open_backend(dir: &Path) -> SqliteBackend {
    let url = format!("sqlite:{}", dir.join("forge.db").display());
    SqliteBackend::open_for_test(&url, TEST_KEY, dir.join("media"), None)
        .await
        .expect("a sqlite backend")
}

fn moderation_over(server: &MockServer, backend: &SqliteBackend) -> YoutubeModeration {
    let live_chat_id = LiveChatIdHandle::new();
    live_chat_id.set(Some("live-chat".to_owned()));
    let api = YoutubeDataApi::new(
        &data_api_override(server),
        SharedQuota::default(),
        Arc::new(forge_events::test_support::NullPublisher),
    )
    .unwrap();
    YoutubeModeration::new(
        api,
        fixed_token("token"),
        fixed_broadcaster(BROADCASTER, "Broadcaster"),
        live_chat_id,
        backend.ban_ledger_repo(),
    )
}

#[tokio::test]
async fn forge_issued_ban_can_be_lifted_after_a_restart() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BANS_ROUTE))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK)
                .set_body_json(serde_json::json!({ "id": BAN_ID })),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(BANS_ROUTE))
        .and(query_param("id", BAN_ID))
        .respond_with(ResponseTemplate::new(reqwest::StatusCode::NO_CONTENT))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("a data dir");

    let first_run = open_backend(dir.path()).await;
    moderation_over(&server, &first_run)
        .ban(VIEWER, None)
        .await
        .unwrap();
    first_run.shutdown().await;
    drop(first_run);

    let second_run = open_backend(dir.path()).await;
    moderation_over(&server, &second_run)
        .unban(VIEWER)
        .await
        .unwrap();

    let key = BanLedgerKey {
        platform: PlatformId::YouTube,
        channel_id: BROADCASTER.to_owned(),
        viewer_id: VIEWER.to_owned(),
    };
    assert_eq!(
        second_run
            .ban_ledger_repo()
            .get(&key, OffsetDateTime::now_utc())
            .await
            .unwrap(),
        None
    );
}
