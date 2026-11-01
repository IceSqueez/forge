use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use forge_runtime::{ActionEngineHandle, EventHistory};
use forge_storage::{ActionRepo, CredentialsRepo, GlobalsRepo, OverlayRepo, UserGlobalsRepo};
use forge_types::{OverlayCredential, Secret};
use tokio::sync::{Mutex, broadcast};

use crate::auth::AuthState;
use crate::bus_adapter::{BusAdapter, OverlayClientClass, WsFrame};
use crate::server_info::ServerInfo;
use crate::ws_client::WsClient;

use super::envelope::{WsErrorCode, WsResponse};

#[derive(Clone)]
pub(crate) struct Services {
    pub bus: Arc<dyn EventHistory>,
    pub bus_adapter: Arc<BusAdapter>,
    pub actions: Arc<dyn ActionRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub overlays: Arc<dyn OverlayRepo>,
    pub auth_state: Arc<AuthState>,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub server_info: Arc<ServerInfo>,
    pub action_engine: Arc<ActionEngineHandle>,
    pub overlay_root: Arc<std::path::PathBuf>,
}

pub(crate) struct Session {
    pub client: Arc<WsClient>,
    pub overlay_channel_swap: Mutex<Option<broadcast::Receiver<WsFrame>>>,
    pub close_after_auth_failure: AtomicBool,
}

impl Session {
    pub(crate) fn new(client: Arc<WsClient>) -> Self {
        Self {
            client,
            overlay_channel_swap: Mutex::new(None),
            close_after_auth_failure: AtomicBool::new(false),
        }
    }
}

pub(crate) struct DispatchContext {
    pub services: Services,
    pub session: Session,
}

pub(super) fn is_authenticated(ctx: &DispatchContext) -> bool {
    ctx.session
        .client
        .bearer_generation()
        .is_some_and(|generation| generation == ctx.services.auth_state.token_generation())
}

pub(super) fn read_refused(ctx: &DispatchContext) -> bool {
    ctx.services.auth_state.reads_required() && !is_authenticated(ctx)
}

pub(super) fn unauthenticated() -> WsResponse {
    WsResponse::error(WsErrorCode::Unauthenticated, "authentication required")
}

pub(super) async fn handle_authenticate(
    token: Option<Secret>,
    overlay_credential: Option<OverlayCredential>,
    preview_connection: bool,
    ctx: &DispatchContext,
) -> WsResponse {
    match (token, overlay_credential) {
        (Some(token), None) => authenticate_bearer(token, ctx).await,
        (None, Some(credential)) => authenticate_overlay(credential, preview_connection, ctx).await,
        (Some(_), Some(_)) => auth_failed("token and overlayCredential cannot both be presented"),
        (None, None) => auth_failed("token or overlayCredential is required"),
    }
}

async fn authenticate_bearer(token: Secret, ctx: &DispatchContext) -> WsResponse {
    if let Some(generation) = ctx
        .services
        .auth_state
        .verify_generation(token.expose())
        .await
    {
        ctx.session.client.mark_bearer_authenticated(generation);
        WsResponse::Ok(serde_json::json!({ "authenticated": true }))
    } else {
        auth_failed("invalid token")
    }
}

async fn authenticate_overlay(
    credential: OverlayCredential,
    preview_connection: bool,
    ctx: &DispatchContext,
) -> WsResponse {
    let lookup = ctx
        .services
        .overlays
        .get_by_credential(&credential)
        .await
        .inspect_err(|error| tracing::warn!(%error, "overlay credential lookup failed"));
    let Ok(Some(definition)) = lookup else {
        ctx.session
            .close_after_auth_failure
            .store(true, Ordering::SeqCst);
        return auth_failed("invalid credential");
    };
    if !definition.enabled {
        ctx.session
            .close_after_auth_failure
            .store(true, Ordering::SeqCst);
        return auth_failed("invalid credential");
    }
    let identity = definition.id;
    if let Some(receiver) = ctx
        .services
        .bus_adapter
        .promote_to_overlay(
            ctx.session.client.id,
            identity.clone(),
            OverlayClientClass::from_frame_flag(preview_connection),
        )
        .await
    {
        *ctx.session.overlay_channel_swap.lock().await = Some(receiver);
        if let Some(listener) = ctx.services.bus_adapter.overlay_connect_listener() {
            listener.overlay_connected(&identity).await;
        }
    }
    WsResponse::Ok(serde_json::json!({ "authenticated": true }))
}

fn auth_failed(message: &str) -> WsResponse {
    WsResponse::error(WsErrorCode::AuthFailed, message)
}
