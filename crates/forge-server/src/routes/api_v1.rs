use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::bus_adapter::ClientId;
use crate::protocol::{
    DispatchContext, WsResponse, handle_do_action, handle_get_actions, handle_get_active_viewers,
    handle_get_events, handle_get_global, handle_get_globals, handle_get_info,
    handle_get_overlay_files, handle_get_user_globals, handle_replay_event, handle_set_global,
    handle_trigger_code_event,
};
use crate::server::AppState;
use crate::ws_client::WsClient;

fn ephemeral_ctx(state: &AppState) -> DispatchContext {
    let drop_counter = Arc::new(AtomicU64::new(0));
    let client = Arc::new(WsClient::new(
        ClientId::next(),
        std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        drop_counter,
    ));
    client
        .authenticated
        .store(true, std::sync::atomic::Ordering::Relaxed);
    DispatchContext {
        bus: Arc::clone(&state.bus),
        bus_adapter: Arc::clone(&state.bus_adapter),
        actions: Arc::clone(&state.actions),
        globals: Arc::clone(&state.globals),
        user_globals: Arc::clone(&state.user_globals),
        auth_state: Arc::clone(&state.auth),
        client,
        auth_required_for_reads: state.auth.auth_required_for_reads,
        credentials: Arc::clone(&state.credentials),
        server_info: Arc::clone(&state.server_info),
        action_engine: Arc::clone(&state.action_engine),
        overlay_root: Arc::clone(&state.overlay_root),
    }
}

fn ws_response_to_http(resp: WsResponse) -> Response {
    match resp {
        WsResponse::Ok(data) => {
            let mut body = serde_json::Map::new();
            body.insert("status".into(), "ok".into());
            if let serde_json::Value::Object(fields) = data {
                for (k, v) in fields {
                    body.insert(k, v);
                }
            }
            (StatusCode::OK, Json(serde_json::Value::Object(body))).into_response()
        }
        WsResponse::Error { code, message } => {
            let status = match code.as_deref() {
                Some("UNAUTHENTICATED") | Some("AUTH_FAILED") => StatusCode::UNAUTHORIZED,
                Some("NOT_FOUND") | Some("UNKNOWN_METHOD") => StatusCode::NOT_FOUND,
                Some("INVALID_PAYLOAD") => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let body = serde_json::json!({
                "error": {
                    "code": code.unwrap_or_else(|| "RUNTIME_ERROR".to_owned()),
                    "message": message,
                }
            });
            (status, Json(body)).into_response()
        }
    }
}

async fn get_info(State(state): State<AppState>) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_info(&ctx).await)
}

async fn get_actions(State(state): State<AppState>) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_actions(&ctx).await)
}

#[derive(Deserialize)]
struct DoActionBody {
    #[serde(default)]
    args: serde_json::Value,
}

async fn do_action_wildcard(
    State(state): State<AppState>,
    Path(id_do): Path<String>,
    body: Option<Json<DoActionBody>>,
) -> Response {
    let id = id_do.strip_suffix(":do").unwrap_or(&id_do).to_owned();
    let ctx = ephemeral_ctx(&state);
    let args = body.map(|b| b.0.args).unwrap_or(serde_json::Value::Null);
    ws_response_to_http(handle_do_action(id, args, &ctx).await)
}

async fn get_globals(State(state): State<AppState>) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_globals(&ctx).await)
}

async fn get_global(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_global(name, &ctx).await)
}

#[derive(Deserialize)]
struct SetGlobalBody {
    value: serde_json::Value,
    #[serde(default)]
    persisted: bool,
}

async fn set_global(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SetGlobalBody>,
) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_set_global(name, body.value, body.persisted, &ctx).await)
}

#[derive(Deserialize)]
struct UserGlobalsQuery {
    #[serde(rename = "broadcasterId")]
    broadcaster_id: String,
    #[serde(rename = "userId")]
    user_id: Option<String>,
}

async fn get_user_globals(
    State(state): State<AppState>,
    Query(q): Query<UserGlobalsQuery>,
) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_user_globals(q.broadcaster_id, q.user_id, &ctx).await)
}

#[derive(Deserialize)]
struct TriggerCodeEventBody {
    #[serde(default)]
    args: serde_json::Value,
}

async fn trigger_code_event(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<TriggerCodeEventBody>>,
) -> Response {
    let ctx = ephemeral_ctx(&state);
    let args = body.map(|b| b.0.args).unwrap_or(serde_json::Value::Null);
    ws_response_to_http(handle_trigger_code_event(name, args, &ctx).await)
}

#[derive(Deserialize)]
struct GetEventsQuery {
    limit: Option<u32>,
    since: Option<String>,
}

async fn get_events(State(state): State<AppState>, Query(q): Query<GetEventsQuery>) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_events(q.limit, q.since, &ctx).await)
}

async fn replay_event_wildcard(
    State(state): State<AppState>,
    Path(id_replay): Path<String>,
) -> Response {
    let id = id_replay
        .strip_suffix(":replay")
        .unwrap_or(&id_replay)
        .to_owned();
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_replay_event(id, &ctx).await)
}

async fn get_viewers(State(state): State<AppState>) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_active_viewers(&ctx).await)
}

#[derive(Deserialize)]
struct OverlayFilesQuery {
    recursive: Option<bool>,
}

async fn get_overlay_files(
    State(state): State<AppState>,
    Query(q): Query<OverlayFilesQuery>,
) -> Response {
    let ctx = ephemeral_ctx(&state);
    ws_response_to_http(handle_get_overlay_files(q.recursive.unwrap_or(false), &ctx).await)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/info", get(get_info))
        .route("/actions", get(get_actions))
        .route("/actions/{*id_do}", post(do_action_wildcard))
        .route("/globals", get(get_globals))
        .route("/globals/{name}", get(get_global).post(set_global))
        .route("/user-globals", get(get_user_globals))
        .route("/code-events/{name}", post(trigger_code_event))
        .route("/events", get(get_events))
        .route("/events/{*id_replay}", post(replay_event_wildcard))
        .route("/viewers", get(get_viewers))
        .route("/overlay-files", get(get_overlay_files))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_registry::SubActionRegistry;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry, spawn_action_engine};
    use forge_storage::{
        CredentialId, CredentialsRepo, DataProvider, GlobalsRepo, StorageError, UserGlobalsRepo,
    };
    use time::OffsetDateTime;
    use tokio::net::TcpListener;

    use crate::ServerHandle;
    use crate::auth::AuthState;
    use crate::bus_adapter::BusAdapter;
    use crate::server::AppState;
    use crate::server_info::ServerInfo;
    use crate::test_helpers::TestDataProvider;

    struct MemCreds(Mutex<HashMap<String, String>>);

    impl MemCreds {
        fn with_token(token: &str) -> Arc<Self> {
            let me = Arc::new(Self(Mutex::new(HashMap::new())));
            me.0.lock()
                .expect("mutex")
                .insert("server:bearer".to_owned(), token.to_owned());
            me
        }
    }

    #[async_trait]
    impl CredentialsRepo for MemCreds {
        async fn store(
            &self,
            id: &CredentialId,
            plaintext_bundle: &str,
        ) -> Result<(), StorageError> {
            self.0
                .lock()
                .expect("mutex")
                .insert(id.as_str().to_owned(), plaintext_bundle.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().expect("mutex").get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().expect("mutex").remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .0
                .lock()
                .expect("mutex")
                .keys()
                .map(|k| CredentialId::new(k.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _id: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    async fn make_test_server(
        auth_required_for_reads: bool,
        token: &str,
    ) -> (ServerHandle, std::net::SocketAddr) {
        let creds = MemCreds::with_token(token);
        let auth = AuthState::load(auth_required_for_reads, &*creds)
            .await
            .expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        bus_adapter.spawn();
        let mut tdp = TestDataProvider::new();
        tdp.globals().expect_list().returning(|| Ok(vec![]));
        tdp.globals().expect_get().returning(|_| Ok(None));
        tdp.globals().expect_set().returning(|_, _, _| Ok(()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let _registry = Arc::new(ScriptRegistry::new());
        let action_engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(forge_runtime::ActionCancelRegistry::new()),
        ));
        let actions = dp.action_repo();
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&dp) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>;
        let overlays = dp.overlay_repo();
        let state = AppState {
            auth,
            bus,
            bus_adapter,
            actions,
            globals,
            user_globals,
            overlays,
            credentials: creds_dyn,
            settings: Arc::clone(&dp) as Arc<dyn forge_storage::SettingsRepo>,
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(std::path::PathBuf::from("/tmp/forge-test-overlays")),
            http_overlay_require_token: false,
            overlay_cors_any_origin: true,
            bind_addr: "127.0.0.1:9515".parse().expect("addr"),
            allowed_origins: Arc::new(crate::origin::build_allowed_origins(
                "127.0.0.1:9515".parse().expect("addr"),
                &[],
            )),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let bind_addr = addr;
        let (run_state_tx, _run_state_rx) = tokio::sync::watch::channel(true);
        let (join, shutdown_tx) =
            crate::server::serve_on_with_shutdown(listener, state.clone(), run_state_tx.clone());
        (
            crate::ServerHandle::new(join, shutdown_tx, state, bind_addr, run_state_tx),
            addr,
        )
    }

    #[tokio::test]
    async fn get_info_with_bearer_returns_200_with_version() {
        let (handle, addr) = make_test_server(false, "tok").await;
        let resp = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/info"))
            .header("Authorization", "Bearer tok")
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert!(body.get("version").is_some(), "missing version field");
        handle.abort();
    }

    #[tokio::test]
    async fn get_info_without_bearer_returns_401_when_reads_required() {
        let (handle, addr) = make_test_server(true, "tok").await;
        let resp = reqwest::get(format!("http://{addr}/api/v1/info"))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 401);
        handle.abort();
    }

    #[tokio::test]
    async fn get_info_without_bearer_returns_200_when_reads_not_required() {
        let (handle, addr) = make_test_server(false, "tok").await;
        let resp = reqwest::get(format!("http://{addr}/api/v1/info"))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert!(body.get("version").is_some(), "missing version field");
        handle.abort();
    }

    #[tokio::test]
    async fn get_globals_with_bearer_returns_200() {
        let (handle, addr) = make_test_server(false, "tok").await;
        let resp = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/globals"))
            .header("Authorization", "Bearer tok")
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert!(body.get("globals").is_some(), "missing globals field");
        handle.abort();
    }

    #[tokio::test]
    async fn post_set_global_with_bearer_returns_200() {
        let (handle, addr) = make_test_server(false, "tok").await;
        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/api/v1/globals/foo"))
            .header("Authorization", "Bearer tok")
            .json(&serde_json::json!({ "value": 42 }))
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        handle.abort();
    }

    #[tokio::test]
    async fn post_set_global_without_bearer_returns_401() {
        let (handle, addr) = make_test_server(false, "tok").await;
        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/api/v1/globals/foo"))
            .json(&serde_json::json!({ "value": 42 }))
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 401);
        handle.abort();
    }
}
