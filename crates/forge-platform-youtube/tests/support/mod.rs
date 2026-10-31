#![allow(dead_code, clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::Arc;

use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};
use forge_platform_youtube::YoutubeBroadcaster;
use futures::future::BoxFuture;
use wiremock::MockServer;

pub(crate) const DATA_API_PREFIX: &str = "/youtube/v3";

pub(crate) type TokenSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;
pub(crate) type BroadcasterSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<YoutubeBroadcaster, PlatformError>> + Send + Sync>;

pub(crate) fn endpoints(overrides: &[(EndpointSurface, String)]) -> PlatformEndpoints {
    let table: BTreeMap<&'static str, String> = overrides
        .iter()
        .map(|(surface, url)| (surface.env_var(), url.clone()))
        .collect();
    PlatformEndpoints::resolve(|variable| table.get(variable).map(OsString::from)).unwrap()
}

pub(crate) fn data_api_override(server: &MockServer) -> PlatformEndpoints {
    endpoints(&[(
        EndpointSurface::YouTubeDataApi,
        format!("{}{DATA_API_PREFIX}", server.uri()),
    )])
}

pub(crate) fn fixed_token(token: &'static str) -> TokenSource {
    Arc::new(move || Box::pin(async move { Ok(token.to_owned()) }))
}

pub(crate) fn fixed_broadcaster(
    channel_id: &'static str,
    channel_title: &'static str,
) -> BroadcasterSource {
    Arc::new(move || {
        Box::pin(async move {
            Ok(YoutubeBroadcaster {
                channel_id: channel_id.to_owned(),
                channel_title: channel_title.to_owned(),
            })
        })
    })
}
