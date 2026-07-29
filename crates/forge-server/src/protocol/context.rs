use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use forge_runtime::{ActionEngineHandle, EventBus};
use forge_storage::{
    ActionRepo, CredentialsRepo, GlobalsRepo, OverlayCredential, OverlayRepo, UserGlobalsRepo,
};
use tokio::sync::{Mutex, broadcast};

use crate::auth::AuthState;
use crate::bus_adapter::{BusAdapter, WsFrame};
use crate::server_info::ServerInfo;
use crate::ws_client::WsClient;

use super::envelope::WsResponse;

pub struct DispatchContext {
    pub bus: Arc<EventBus>,
    pub bus_adapter: Arc<BusAdapter>,
    pub actions: Arc<dyn ActionRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub overlays: Arc<dyn OverlayRepo>,
    pub auth_state: Arc<AuthState>,
    pub client: Arc<WsClient>,
    pub auth_required_for_reads: bool,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub server_info: Arc<ServerInfo>,
    pub action_engine: Arc<ActionEngineHandle>,
    pub overlay_root: Arc<std::path::PathBuf>,
    /// Populated once an overlay credential validates; the WS handler takes it to swap its
    /// receiver onto the tighter overlay channel bound.
    pub overlay_channel_swap: Mutex<Option<broadcast::Receiver<WsFrame>>>,
    /// Set when an overlay credential is refused; the WS handler closes the socket right after
    /// sending the response so the page's own reconnect loop is what recovers it later.
    pub close_after_auth_failure: AtomicBool,
}

pub(super) fn is_authenticated(ctx: &DispatchContext) -> bool {
    ctx.client.authenticated.load(Ordering::Acquire)
}

pub(super) fn unauthenticated() -> WsResponse {
    WsResponse::Error {
        code: Some("UNAUTHENTICATED".to_owned()),
        message: "authentication required".to_owned(),
    }
}

pub(super) async fn handle_authenticate(
    token: Option<String>,
    overlay_credential: Option<String>,
    ctx: &DispatchContext,
) -> WsResponse {
    match (token, overlay_credential) {
        (Some(token), None) => authenticate_bearer(token, ctx).await,
        (None, Some(credential)) => authenticate_overlay(credential, ctx).await,
        (Some(_), Some(_)) => auth_failed("token and overlayCredential cannot both be presented"),
        (None, None) => auth_failed("token or overlayCredential is required"),
    }
}

async fn authenticate_bearer(token: String, ctx: &DispatchContext) -> WsResponse {
    if ctx.auth_state.verify(&token).await {
        ctx.client.authenticated.store(true, Ordering::SeqCst);
        WsResponse::Ok(serde_json::json!({ "authenticated": true }))
    } else {
        auth_failed("invalid token")
    }
}

/// Never sets `client.authenticated`: the overlay credential grants directed delivery only, and
/// every mutating and read-gated method stays refused exactly as for an unauthenticated client.
async fn authenticate_overlay(credential: String, ctx: &DispatchContext) -> WsResponse {
    let lookup = ctx
        .overlays
        .get_by_credential(&OverlayCredential::new(credential))
        .await;
    let Ok(Some(definition)) = lookup else {
        ctx.close_after_auth_failure.store(true, Ordering::SeqCst);
        return auth_failed("invalid credential");
    };
    if !definition.enabled {
        ctx.close_after_auth_failure.store(true, Ordering::SeqCst);
        return auth_failed("invalid credential");
    }
    let identity = definition.id;
    if let Some(receiver) = ctx
        .bus_adapter
        .promote_to_overlay(ctx.client.id, identity.clone())
        .await
    {
        *ctx.overlay_channel_swap.lock().await = Some(receiver);
        if let Some(listener) = ctx.bus_adapter.overlay_connect_listener() {
            listener.overlay_connected(&identity).await;
        }
    }
    WsResponse::Ok(serde_json::json!({ "authenticated": true }))
}

fn auth_failed(message: &str) -> WsResponse {
    WsResponse::Error {
        code: Some("AUTH_FAILED".to_owned()),
        message: message.to_owned(),
    }
}
