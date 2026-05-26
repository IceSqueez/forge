use std::sync::Arc;
use std::sync::atomic::Ordering;

use forge_runtime::{ActionEngineHandle, EventBus};
use forge_storage::{ActionRepo, CommandRepo, CredentialsRepo, GlobalsRepo, UserGlobalsRepo};

use crate::auth::AuthState;
use crate::bus_adapter::BusAdapter;
use crate::server_info::ServerInfo;
use crate::ws_client::WsClient;

use super::envelope::WsResponse;

pub struct DispatchContext {
    pub bus: Arc<EventBus>,
    pub bus_adapter: Arc<BusAdapter>,
    pub actions: Arc<dyn ActionRepo>,
    pub commands: Arc<dyn CommandRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub auth_state: Arc<AuthState>,
    pub client: Arc<WsClient>,
    pub auth_required_for_reads: bool,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub server_info: Arc<ServerInfo>,
    pub action_engine: Arc<ActionEngineHandle>,
    pub overlay_root: Arc<std::path::PathBuf>,
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

pub(super) async fn handle_authenticate(token: String, ctx: &DispatchContext) -> WsResponse {
    if ctx.auth_state.verify(&token).await {
        ctx.client.authenticated.store(true, Ordering::SeqCst);
        WsResponse::Ok(serde_json::json!({ "authenticated": true }))
    } else {
        WsResponse::Error {
            code: Some("AUTH_FAILED".to_owned()),
            message: "invalid token".to_owned(),
        }
    }
}
