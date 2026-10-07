#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;

use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};
use forge_platform_youtube::{
    LiveChatIdHandle, SharedQuota, YoutubeBroadcaster, YoutubeModeration,
};
use forge_storage::{BanLedgerKey, DataProvider, ViewerPlatform};
use forge_storage_sqlite::SqliteBackend;
use futures::future::BoxFuture;
use time::OffsetDateTime;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DATA_API_PREFIX: &str = "/youtube/v3";
const BANS_ROUTE: &str = "/youtube/v3/liveChat/bans";
const BAN_ID: &str = "ban-resource-restart";
const BROADCASTER: &str = "UCbroadcaster";
const VIEWER: &str = "UCviewer";
const TEST_KEY: [u8; 32] = [0x5a; 32];

fn data_api_override(server: &MockServer) -> PlatformEndpoints {
    let table = BTreeMap::from([(
        EndpointSurface::YouTubeDataApi.env_var(),
        format!("{}{DATA_API_PREFIX}", server.uri()),
    )]);
    PlatformEndpoints::resolve(|variable| table.get(variable).map(OsString::from)).unwrap()
}

async fn open_backend(dir: &Path) -> SqliteBackend {
    let url = format!("sqlite:{}", dir.join("forge.db").display());
    SqliteBackend::open_for_test(&url, TEST_KEY, dir.join("media"), None)
        .await
        .expect("a sqlite backend")
}

fn moderation_over(server: &MockServer, backend: &SqliteBackend) -> YoutubeModeration {
    let live_chat_id = LiveChatIdHandle::new();
    live_chat_id.set(Some("live-chat".to_owned()));
    let token_source: Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
    > = Arc::new(|| Box::pin(async { Ok("token".to_owned()) }));
    let broadcaster_source: Arc<
        dyn Fn() -> BoxFuture<'static, Result<YoutubeBroadcaster, PlatformError>> + Send + Sync,
    > = Arc::new(|| {
        Box::pin(async {
            Ok(YoutubeBroadcaster {
                channel_id: BROADCASTER.to_owned(),
                channel_title: "Broadcaster".to_owned(),
            })
        })
    });
    YoutubeModeration::new(
        &data_api_override(server),
        token_source,
        broadcaster_source,
        live_chat_id,
        SharedQuota::default(),
        backend.ban_ledger_repo(),
    )
}

#[tokio::test]
async fn forge_issued_ban_can_be_lifted_after_a_restart() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BANS_ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": BAN_ID })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(BANS_ROUTE))
        .and(query_param("id", BAN_ID))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("a data dir");

    let first_run = open_backend(dir.path()).await;
    moderation_over(&server, &first_run)
        .ban(VIEWER)
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
        platform: ViewerPlatform::YouTube,
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
