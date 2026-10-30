use std::sync::Arc;

use forge_net::GatewayRequest;
use forge_platform_core::PlatformError;
use forge_types::Secret;
use futures::future::BoxFuture;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::data_api::{YoutubeDataApi, json_body};

const CUEPOINT_COST: u32 = 50;
const CUEPOINT_PATH: &str = "/liveBroadcasts/cuepoint";
const MISSING_SCOPE: &str = "ad break scope missing";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeAdBreak {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
}

impl YoutubeAdBreak {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
    ) -> Self {
        Self {
            api,
            access_token_source,
            active_broadcast_id,
        }
    }

    pub async fn insert_cuepoint(&self, duration_secs: u32) -> Result<(), PlatformError> {
        let broadcast_id = self.active_broadcast_id.resolve().await?.ok_or_else(|| {
            PlatformError::Unsupported {
                feature: "ad break - no active YouTube broadcast".to_owned(),
            }
        })?;

        let payload = serde_json::json!({
            "cueType": "cueTypeAd",
            "durationSecs": duration_secs,
        });
        let body = json_body(&payload)?;
        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::post(CUEPOINT_PATH)
            .label(CUEPOINT_PATH)
            .bearer(Secret::new(token))
            .query("id", broadcast_id)
            .body(body);

        self.api
            .send(CUEPOINT_COST, request)
            .await
            .map(drop)
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::endpoints_test_support::{data_api_charging, unreachable_data_api, used_today};
    use crate::quota_state::SharedQuota;

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok("yt-adbreak-token".to_owned()) }))
    }

    fn broadcast(id: Option<&str>) -> ActiveBroadcastIdHandle {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(id.map(ToOwned::to_owned));
        handle
    }

    #[tokio::test]
    async fn insert_returns_unsupported_when_no_active_broadcast() {
        let ad_break = YoutubeAdBreak::new(unreachable_data_api(), token_source(), broadcast(None));

        let err = ad_break.insert_cuepoint(30).await.unwrap_err();

        assert!(matches!(err, PlatformError::Unsupported { .. }), "{err}");
    }

    #[tokio::test]
    async fn insert_posts_ad_cuepoint_with_id_query_and_charges_quota() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveBroadcasts/cuepoint"))
            .and(query_param("id", "bc-777"))
            .and(body_json(
                json!({ "cueType": "cueTypeAd", "durationSecs": 45 }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;
        let quota = SharedQuota::default();
        let ad_break = YoutubeAdBreak::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            broadcast(Some("bc-777")),
        );

        ad_break.insert_cuepoint(45).await.unwrap();

        assert_eq!(used_today(&quota).await, CUEPOINT_COST);
    }
}
