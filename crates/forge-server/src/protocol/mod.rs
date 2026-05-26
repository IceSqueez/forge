mod context;
mod envelope;
mod handlers;
mod helpers;
mod introspect;

use std::collections::HashSet;

use crate::bus_adapter::{ClientFilterSet, EventFilter};

pub use context::DispatchContext;
pub use envelope::{
    EventMetadata, EventPushFrame, WireEventFilter, WsEnvelope, WsRequest, WsResponse,
    serialize_response_frame,
};
pub(crate) use handlers::{
    handle_do_action, handle_get_actions, handle_get_active_viewers, handle_get_commands,
    handle_get_events, handle_get_global, handle_get_globals, handle_get_info,
    handle_get_overlay_files, handle_get_user_globals, handle_replay_event, handle_set_global,
    handle_trigger_code_event, mime_for_extension,
};

use context::{handle_authenticate, is_authenticated, unauthenticated};
use helpers::parse_wire_filter;

pub async fn dispatch(req: WsEnvelope<WsRequest>, ctx: &DispatchContext) -> WsEnvelope<WsResponse> {
    let id = req.id.clone();
    let inner = route(req.inner, ctx).await;
    WsEnvelope { id, inner }
}

async fn route(req: WsRequest, ctx: &DispatchContext) -> WsResponse {
    match req {
        WsRequest::Auth { token } => handle_authenticate(token, ctx).await,

        WsRequest::Subscribe { events } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            let new_filters: HashSet<EventFilter> = events.iter().map(parse_wire_filter).collect();
            let mut current = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
            current.extend(new_filters);
            ctx.bus_adapter
                .update_subscriptions(ctx.client.id, ClientFilterSet::new(current))
                .await;
            WsResponse::Ok(serde_json::json!({ "subscribed": events }))
        }

        WsRequest::Unsubscribe { events } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            let to_remove: HashSet<EventFilter> = events.iter().map(parse_wire_filter).collect();
            let mut current = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
            current.retain(|f| !to_remove.contains(f));
            ctx.bus_adapter
                .update_subscriptions(ctx.client.id, ClientFilterSet::new(current))
                .await;
            WsResponse::Ok(serde_json::json!({ "unsubscribed": events }))
        }

        WsRequest::GetInfo => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_info(ctx).await
        }

        WsRequest::GetActions => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_actions(ctx).await
        }

        WsRequest::DoAction { action_id, args } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_do_action(action_id, args, ctx).await
        }

        WsRequest::GetCommands => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_commands(ctx).await
        }

        WsRequest::GetGlobals => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_globals(ctx).await
        }

        WsRequest::GetGlobal { name } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_global(name, ctx).await
        }

        WsRequest::SetGlobal {
            name,
            value,
            persisted,
        } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_set_global(name, value, persisted, ctx).await
        }

        WsRequest::GetUserGlobals {
            broadcaster_id,
            user_id,
        } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_user_globals(broadcaster_id, user_id, ctx).await
        }

        WsRequest::TriggerCodeEvent { name, args } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_trigger_code_event(name, args, ctx).await
        }

        WsRequest::GetEvents { limit, since } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_events(limit, since, ctx).await
        }

        WsRequest::ReplayEvent { event_id } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_replay_event(event_id, ctx).await
        }

        WsRequest::GetActiveViewers => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_active_viewers(ctx).await
        }

        WsRequest::GetOverlayFiles { recursive } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_overlay_files(recursive, ctx).await
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::auth::AuthState;
    use forge_events::{Event, EventSource};
    use forge_runtime::{
        ActionEngineHandle, EventBus, NullEventLogRepo, ScriptRegistry, spawn_action_engine,
    };
    use forge_storage::{DataProvider, GlobalsRepo, UserGlobalsRepo};
    use forge_types::{Action, ActionId, LogLevel, QueueId, SubActionSpec};

    use super::*;
    use crate::bus_adapter::{BusAdapter, ClientFilterSet, ClientId, EventFilter};
    use crate::server_info::ServerInfo;
    use forge_storage::GlobalEntry;
    use time::OffsetDateTime;

    use crate::test_helpers::{TestDataProvider, test_creds, test_dp};
    use crate::ws_client::WsClient;

    fn make_engine(bus: &Arc<EventBus>, dp: &Arc<dyn DataProvider>) -> Arc<ActionEngineHandle> {
        let registry = Arc::new(ScriptRegistry::new());
        Arc::new(spawn_action_engine(
            Arc::clone(bus),
            Arc::clone(dp),
            registry,
            None,
            None,
            None,
        ))
    }

    fn make_ctx(authenticated: bool, auth_required_for_reads: bool) -> DispatchContext {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        let mut tdp = TestDataProvider::new();
        tdp.action().expect_list().returning(|| Ok(vec![]));
        tdp.action().expect_get().returning(|_| Ok(None));
        tdp.globals().expect_get().returning(|_| Ok(None));
        tdp.globals().expect_set().returning(|_, _, _| Ok(()));
        tdp.user_globals()
            .expect_list_for_broadcaster()
            .returning(|_| Ok(vec![]));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let actions = dp.action_repo();
        let commands = dp.command_repo();
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&dp) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>;
        let auth_state = AuthState::for_test(auth_required_for_reads, "test-token");
        let drop_counter = Arc::new(AtomicU64::new(0));
        let client = Arc::new(WsClient::new(
            ClientId::next(),
            "127.0.0.1:0".parse().unwrap(),
            Arc::clone(&drop_counter),
        ));
        client.authenticated.store(authenticated, Ordering::Relaxed);
        let action_engine = make_engine(&bus, &dp);
        let mut creds_tdp = TestDataProvider::new();
        creds_tdp
            .credentials()
            .expect_list_ids()
            .returning(|| Ok(vec![]));
        let credentials: Arc<dyn forge_storage::CredentialsRepo> = Arc::new(creds_tdp);
        DispatchContext {
            bus,
            bus_adapter,
            actions,
            commands,
            globals,
            user_globals,
            auth_state,
            client,
            auth_required_for_reads,
            credentials,
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(std::path::PathBuf::from("/tmp/forge-test-overlays")),
        }
    }

    fn make_ctx_with_dp(authenticated: bool, dp: Arc<dyn DataProvider>) -> DispatchContext {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        let auth_state = AuthState::for_test(false, "test-token");
        let drop_counter = Arc::new(AtomicU64::new(0));
        let client = Arc::new(WsClient::new(
            ClientId::next(),
            "127.0.0.1:0".parse().unwrap(),
            Arc::clone(&drop_counter),
        ));
        client.authenticated.store(authenticated, Ordering::Relaxed);
        let action_engine = make_engine(&bus, &dp);
        let actions = dp.action_repo();
        let commands = dp.command_repo();
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&dp) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>;
        DispatchContext {
            bus,
            bus_adapter,
            actions,
            commands,
            globals,
            user_globals,
            auth_state,
            client,
            auth_required_for_reads: false,
            credentials: test_creds(),
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(std::path::PathBuf::from("/tmp/forge-test-overlays")),
        }
    }

    async fn make_registered_ctx(
        authenticated: bool,
        auth_required_for_reads: bool,
    ) -> DispatchContext {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        let dp: Arc<dyn DataProvider> = test_dp();
        let actions = dp.action_repo();
        let commands = dp.command_repo();
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&dp) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>;
        let auth_state = AuthState::for_test(auth_required_for_reads, "test-token");
        let (handle, _rx) = bus_adapter
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        let client = Arc::new(WsClient::new(
            handle.id,
            "127.0.0.1:0".parse().unwrap(),
            Arc::clone(&handle.drop_counter),
        ));
        client.authenticated.store(authenticated, Ordering::Relaxed);
        let action_engine = make_engine(&bus, &dp);
        let mut creds_tdp = TestDataProvider::new();
        creds_tdp
            .credentials()
            .expect_list_ids()
            .returning(|| Ok(vec![]));
        let credentials: Arc<dyn forge_storage::CredentialsRepo> = Arc::new(creds_tdp);
        DispatchContext {
            bus,
            bus_adapter,
            actions,
            commands,
            globals,
            user_globals,
            auth_state,
            client,
            auth_required_for_reads,
            credentials,
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(std::path::PathBuf::from("/tmp/forge-test-overlays")),
        }
    }

    fn sample_action() -> Action {
        Action {
            id: ActionId::new(),
            name: "Test Action".to_string(),
            group: Some("Chat".to_string()),
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: Some("A test action".to_string()),
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "hello".to_string(),
            }],
        }
    }

    #[test]
    fn parse_request_envelope_get_info() {
        let json = r#"{"id":"1","request":"getInfo"}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, Some("1".to_owned()));
        assert!(matches!(env.inner, WsRequest::GetInfo));
    }

    #[test]
    fn parse_request_envelope_do_action_camel_case_fields() {
        let json = r#"{"id":"5","request":"doAction","actionId":"abc","args":{"user":"Alice"}}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, Some("5".to_owned()));
        match env.inner {
            WsRequest::DoAction { action_id, args } => {
                assert_eq!(action_id, "abc");
                assert_eq!(args["user"], "Alice");
            }
            other => panic!("expected DoAction, got {other:?}"),
        }
    }

    #[test]
    fn parse_request_envelope_no_id_field() {
        let json = r#"{"request":"getActions"}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, None);
        assert!(matches!(env.inner, WsRequest::GetActions));
    }

    #[test]
    fn serialize_response_ok_data_flattened() {
        let env = WsEnvelope {
            id: Some("1".to_owned()),
            inner: WsResponse::Ok(serde_json::json!({"version": "0.1.0-alpha.9"})),
        };
        let json = serialize_response_frame(&env);
        assert_eq!(json["id"], "1");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["version"], "0.1.0-alpha.9");
        assert!(json.get("inner").is_none());
    }

    #[test]
    fn serialize_response_error_shape() {
        let env = WsEnvelope {
            id: Some("2".to_owned()),
            inner: WsResponse::Error {
                code: Some("NOT_FOUND".to_owned()),
                message: "action not found".to_owned(),
            },
        };
        let json = serialize_response_frame(&env);
        assert_eq!(json["id"], "2");
        assert_eq!(json["status"], "error");
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert_eq!(json["error"]["message"], "action not found");
    }

    #[test]
    fn serialize_response_null_id_when_absent() {
        let env = WsEnvelope {
            id: None,
            inner: WsResponse::Ok(serde_json::json!({})),
        };
        let json = serialize_response_frame(&env);
        assert!(json["id"].is_null());
    }

    #[tokio::test]
    async fn unauthenticated_do_action_returns_unauthenticated_error() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("5".to_owned()),
            inner: WsRequest::DoAction {
                action_id: "fake-id".to_owned(),
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("5".to_owned()));
        match resp.inner {
            WsResponse::Error {
                code: Some(code),
                message,
            } => {
                assert_eq!(code, "UNAUTHENTICATED");
                assert!(!message.is_empty());
            }
            other => panic!("expected UNAUTHENTICATED error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unauthenticated_set_global_returns_unauthenticated_error() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("6".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(42),
                persisted: true,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_actions_returns_empty_list_for_null_dp() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("5".to_owned()),
            inner: WsRequest::GetActions,
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("5".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["actions"].is_array());
        assert_eq!(json["actions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_actions_returns_wire_shape_for_seeded_action() {
        let action = sample_action();
        let action_id_str = action.id.to_string();
        let actions = vec![action];
        let mut tdp = TestDataProvider::new();
        tdp.action()
            .expect_list()
            .returning(move || Ok(actions.clone()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("5".to_owned()),
            inner: WsRequest::GetActions,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let actions = json["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["id"].as_str().unwrap(), action_id_str);
        assert_eq!(actions[0]["name"], "Test Action");
        assert_eq!(actions[0]["group"], "Chat");
        assert_eq!(actions[0]["enabled"], true);
        assert_eq!(actions[0]["concurrent"], false);
        assert_eq!(actions[0]["bypass_queue_pause"], false);
        assert_eq!(actions[0]["sub_action_count"], 1u64);
        assert!(actions[0]["queue_id"].is_string());
    }

    #[tokio::test]
    async fn do_action_valid_id_returns_ok_with_execution_id() {
        let action = sample_action();
        let action_id_str = action.id.to_string();
        let action_clone = action.clone();
        let mut tdp = TestDataProvider::new();
        tdp.action()
            .expect_get()
            .returning(move |_| Ok(Some(action_clone.clone())));
        tdp.history().expect_save().returning(|_| Ok(()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(true, dp);
        let req = WsEnvelope {
            id: Some("6".to_owned()),
            inner: WsRequest::DoAction {
                action_id: action_id_str,
                args: serde_json::json!({ "user": "Alice", "count": 3 }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("6".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);
        assert!(json["execution_id"].is_string());
        assert!(!json["execution_id"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn do_action_nonexistent_id_returns_not_found() {
        let ctx = make_ctx(true, false);
        let nonexistent = ActionId::new().to_string();
        let req = WsEnvelope {
            id: Some("7".to_owned()),
            inner: WsRequest::DoAction {
                action_id: nonexistent,
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("7".to_owned()));
        match &resp.inner {
            WsResponse::Error {
                code: Some(code),
                message,
            } => {
                assert_eq!(code, "NOT_FOUND");
                assert!(!message.is_empty());
            }
            other => panic!("expected NOT_FOUND error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_info_succeeds_without_auth_by_default() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("8".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["version"].is_string());
        assert!(json["uptime_seconds"].is_number());
        assert!(json["connected_clients"].is_array());
        assert!(json["bandwidth"].is_object());
    }

    #[tokio::test]
    async fn read_only_blocked_when_auth_required_for_reads() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("9".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_events_empty_bus_returns_empty_list() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("13".to_owned()),
            inner: WsRequest::GetEvents {
                limit: None,
                since: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["events"].is_array());
        assert_eq!(json["events"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_events_with_three_events_returns_all_in_shape() {
        let ctx = make_ctx(false, false);
        for i in 0u32..3 {
            ctx.bus.publish(Event::new(
                EventSource::Core,
                "test.event",
                serde_json::json!({ "i": i }),
            ));
        }
        let req = WsEnvelope {
            id: Some("13".to_owned()),
            inner: WsRequest::GetEvents {
                limit: None,
                since: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let events = json["events"].as_array().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events[0]["id"].is_string());
        assert!(events[0]["source"].is_string());
        assert!(events[0]["kind"].is_string());
        assert!(events[0]["timestamp"].is_string());
        assert!(events[0]["payload"].is_object());
        assert_eq!(events[0]["replay"], false);
    }

    #[tokio::test]
    async fn get_events_since_with_limit_returns_events_after_anchor() {
        let ctx = make_ctx(false, false);
        let ev1 = Event::new(EventSource::Core, "test.a", serde_json::json!({}));
        let anchor_id = ev1.id.to_string();
        ctx.bus.publish(ev1);
        ctx.bus.publish(Event::new(
            EventSource::Core,
            "test.b",
            serde_json::json!({}),
        ));
        ctx.bus.publish(Event::new(
            EventSource::Core,
            "test.c",
            serde_json::json!({}),
        ));

        let req = WsEnvelope {
            id: Some("13".to_owned()),
            inner: WsRequest::GetEvents {
                limit: Some(1),
                since: Some(anchor_id),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let events = json["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "test.c");
    }

    #[tokio::test]
    async fn replay_event_without_auth_returns_unauthenticated() {
        let ctx = make_ctx(false, false);
        let ev = Event::new(EventSource::Core, "test.event", serde_json::json!({}));
        let ev_id = ev.id.to_string();
        ctx.bus.publish(ev);
        let req = WsEnvelope {
            id: Some("14".to_owned()),
            inner: WsRequest::ReplayEvent { event_id: ev_id },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn replay_event_nonexistent_id_returns_not_found() {
        let ctx = make_ctx(true, false);
        let ghost_id = forge_types::EventId::new().to_string();
        let req = WsEnvelope {
            id: Some("14".to_owned()),
            inner: WsRequest::ReplayEvent { event_id: ghost_id },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "NOT_FOUND"),
            other => panic!("expected NOT_FOUND, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn replay_event_valid_id_returns_ok_and_emits_replayed_event() {
        let ctx = make_ctx(true, false);
        let original = Event::new(EventSource::Core, "test.original", serde_json::json!({}));
        let original_id = original.id.to_string();
        ctx.bus.publish(original);

        let mut bus_sub = ctx.bus.subscribe();

        let req = WsEnvelope {
            id: Some("14".to_owned()),
            inner: WsRequest::ReplayEvent {
                event_id: original_id,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let replayed = bus_sub.recv().await.unwrap();
        assert_eq!(replayed.kind, "test.original");
        assert!(replayed.replay);
    }

    #[tokio::test]
    async fn subscribe_registers_filter_for_client() {
        let ctx = make_registered_ctx(false, false).await;
        let req = WsEnvelope {
            id: Some("2".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("chat.message".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("2".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["subscribed"].is_array());

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert!(subs.contains(&EventFilter::new(
            Some(EventSource::Twitch),
            Some("chat.message".to_owned()),
        )));
    }

    #[tokio::test]
    async fn unsubscribe_removes_filter_from_client() {
        let ctx = make_registered_ctx(false, false).await;
        let filter = EventFilter::new(Some(EventSource::Twitch), Some("chat.message".to_owned()));
        ctx.bus_adapter
            .update_subscriptions(
                ctx.client.id,
                ClientFilterSet::new(HashSet::from([filter.clone()])),
            )
            .await;

        let req = WsEnvelope {
            id: Some("3".to_owned()),
            inner: WsRequest::Unsubscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("chat.message".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("3".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["unsubscribed"].is_array());

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert!(!subs.contains(&filter));
    }

    #[tokio::test]
    async fn subscribe_is_additive_across_calls() {
        let ctx = make_registered_ctx(false, false).await;
        let req1 = WsEnvelope {
            id: Some("2".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: None,
                }],
            },
        };
        dispatch(req1, &ctx).await;

        let req2 = WsEnvelope {
            id: Some("3".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("obs".to_owned()),
                    kind: None,
                }],
            },
        };
        dispatch(req2, &ctx).await;

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert_eq!(subs.len(), 2);
    }

    #[tokio::test]
    async fn subscribe_wildcard_type_registers_kind_wildcard() {
        let ctx = make_registered_ctx(false, false).await;
        let req = WsEnvelope {
            id: Some("4".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("*".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert!(subs.contains(&EventFilter::new(Some(EventSource::Twitch), None)));
    }

    #[tokio::test]
    async fn get_info_no_clients_returns_empty_connected_clients() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("1".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert!(json["uptime_seconds"].is_number());
        let clients = json["connected_clients"].as_array().unwrap();
        assert!(clients.is_empty());
        assert_eq!(json["bandwidth"]["outbound_bytes_total"], 0u64);
        assert_eq!(json["bandwidth"]["peak_outbound_bytes_per_second"], 0u64);
        assert!(json["available_platforms"].is_array());
    }

    #[tokio::test]
    async fn get_info_with_connected_client_returns_telemetry() {
        let ctx = make_registered_ctx(false, false).await;
        ctx.server_info
            .register(ctx.client.id, Arc::clone(&ctx.client))
            .await;

        let req = WsEnvelope {
            id: Some("1".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let clients = json["connected_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert!(clients[0]["client_id"].as_str().unwrap().starts_with("ws_"));
        assert!(clients[0]["bytes_sent"].is_number());
        assert!(clients[0]["uptime_seconds"].is_number());
        assert!(clients[0]["events_per_second"].is_number());
        assert!(clients[0]["subscriptions"].is_array());
    }

    fn sample_command() -> forge_types::Command {
        forge_types::Command {
            id: forge_types::CommandId::new(),
            action_id: ActionId::new(),
            name: "!quote".to_string(),
            cooldown_secs: 30,
            permission: forge_types::CommandPermission::Everyone,
        }
    }

    fn sample_global_entry() -> GlobalEntry {
        GlobalEntry {
            name: "counter".to_string(),
            value: forge_types::Variant::Int(42),
            persisted: true,
            reads: 5,
            writes: 2,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_001_000).unwrap(),
        }
    }

    #[tokio::test]
    async fn get_commands_returns_list_with_sample_command() {
        let cmd = sample_command();
        let cmd_id = cmd.id.to_string();
        let action_id = cmd.action_id.to_string();
        let commands = vec![cmd];
        let mut tdp = TestDataProvider::new();
        tdp.command()
            .expect_list()
            .returning(move || Ok(commands.clone()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("7".to_owned()),
            inner: WsRequest::GetCommands,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let cmds = json["commands"].as_array().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0]["id"].as_str().unwrap(), cmd_id);
        assert_eq!(cmds[0]["command"], "!quote");
        assert_eq!(cmds[0]["action_id"].as_str().unwrap(), action_id);
        assert_eq!(cmds[0]["cooldown_seconds"], 30u64);
        assert_eq!(cmds[0]["enabled"], true);
        assert_eq!(cmds[0]["permission_level"], "everyone");
    }

    #[tokio::test]
    async fn get_globals_returns_list_with_sample_global() {
        let entry = sample_global_entry();
        let entries = vec![entry];
        let mut tdp = TestDataProvider::new();
        tdp.globals()
            .expect_list()
            .returning(move || Ok(entries.clone()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("8".to_owned()),
            inner: WsRequest::GetGlobals,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let globals = json["globals"].as_array().unwrap();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0]["key"], "counter");
        assert_eq!(globals[0]["value"], 42i64);
        assert_eq!(globals[0]["type"], "int");
        assert_eq!(globals[0]["persisted"], true);
        assert_eq!(globals[0]["reads"], 5u64);
        assert_eq!(globals[0]["writes"], 2u64);
        assert!(globals[0]["last_modified"].is_string());
    }

    #[tokio::test]
    async fn get_global_existing_returns_value() {
        let entry = sample_global_entry();
        let entry_clone = entry.clone();
        let mut tdp = TestDataProvider::new();
        tdp.globals()
            .expect_get()
            .returning(move |_| Ok(Some(entry_clone.value.clone())));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("9".to_owned()),
            inner: WsRequest::GetGlobal {
                name: "counter".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["name"], "counter");
        assert_eq!(json["value"], 42i64);
        assert_eq!(json["type"], "int");
    }

    #[tokio::test]
    async fn get_global_nonexistent_returns_not_found() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("9".to_owned()),
            inner: WsRequest::GetGlobal {
                name: "missing".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "NOT_FOUND"),
            other => panic!("expected NOT_FOUND error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_global_without_auth_returns_unauthenticated() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(99),
                persisted: true,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_global_with_auth_returns_ok_and_emits_event() {
        let ctx = make_ctx(true, false);
        let mut bus_sub = ctx.bus.subscribe();
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(99),
                persisted: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let event = bus_sub.recv().await.unwrap();
        assert_eq!(event.source, EventSource::Server);
        assert_eq!(event.kind, "global.set");
        assert_eq!(event.payload["name"], "counter");
        assert_eq!(event.payload["via"], "ws_api");
    }

    #[tokio::test]
    async fn set_global_null_value_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::Value::Null,
                persisted: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    fn sample_user_global_entry() -> forge_storage::UserGlobalEntry {
        forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "87654321".to_string(),
            name: "points".to_string(),
            value: forge_types::Variant::Int(500),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        }
    }

    #[tokio::test]
    async fn get_user_globals_empty_for_null_dp() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("11".to_owned()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["globals"].is_array());
        assert_eq!(json["globals"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_user_globals_with_user_id_returns_filtered_globals() {
        let entry = sample_user_global_entry();
        let _other_entry = forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "11111111".to_string(),
            name: "other".to_string(),
            value: forge_types::Variant::Int(0),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        };
        let filtered = vec![entry];
        let mut tdp = TestDataProvider::new();
        tdp.user_globals()
            .expect_list_for_user()
            .returning(move |_, _| Ok(filtered.clone()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("11".to_owned()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: Some("87654321".to_owned()),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let globals = json["globals"].as_array().unwrap();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0]["user_id"], "87654321");
        assert_eq!(globals[0]["key"], "points");
        assert_eq!(globals[0]["value"], 500i64);
        assert_eq!(globals[0]["type"], "int");
        assert!(globals[0]["last_modified"].is_string());
    }

    #[tokio::test]
    async fn get_user_globals_without_user_id_returns_all_broadcaster_globals() {
        let entry1 = sample_user_global_entry();
        let entry2 = forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "11111111".to_string(),
            name: "level".to_string(),
            value: forge_types::Variant::Int(5),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        };
        let all_entries = vec![entry1, entry2];
        let mut tdp = TestDataProvider::new();
        tdp.user_globals()
            .expect_list_for_broadcaster()
            .returning(move |_| Ok(all_entries.clone()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("11".to_owned()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let globals = json["globals"].as_array().unwrap();
        assert_eq!(globals.len(), 2);
    }

    #[tokio::test]
    async fn trigger_code_event_without_auth_returns_unauthenticated() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: "my_event".to_owned(),
                args: serde_json::json!({ "scene": "Gameplay" }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn trigger_code_event_with_auth_publishes_bus_event() {
        let ctx = make_ctx(true, false);
        let mut bus_sub = ctx.bus.subscribe();
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: "my_event".to_owned(),
                args: serde_json::json!({ "scene": "Gameplay" }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let event = bus_sub.recv().await.unwrap();
        assert_eq!(event.source, EventSource::Server);
        assert_eq!(event.kind, "custom.my_event");
        assert_eq!(event.payload["scene"], "Gameplay");
    }

    #[tokio::test]
    async fn trigger_code_event_invalid_name_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: "Invalid-Name!".to_owned(),
                args: serde_json::json!({}),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn trigger_code_event_empty_name_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: String::new(),
                args: serde_json::json!({}),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn authenticate_correct_token_marks_session_authenticated() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a1".to_owned()),
            inner: WsRequest::Auth {
                token: "test-token".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Ok(data) => {
                assert_eq!(data.get("authenticated"), Some(&serde_json::json!(true)));
            }
            other => panic!("expected ok response, got {other:?}"),
        }
        assert!(ctx.client.authenticated.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn authenticate_wrong_token_returns_auth_failed() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a2".to_owned()),
            inner: WsRequest::Auth {
                token: "wrong-token".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED error, got {other:?}"),
        }
        assert!(!ctx.client.authenticated.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn get_active_viewers_empty_bus_returns_empty_list() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("av1".to_owned()),
            inner: WsRequest::GetActiveViewers,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["viewers"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_active_viewers_dedupes_by_platform_and_user_id() {
        let ctx = make_ctx(false, false);
        let chat_payload = |login: &str, id: &str| {
            serde_json::json!({
                "channel": "ch",
                "user": { "login": login, "id": id, "roles": [] },
                "message": "hi",
            })
        };
        ctx.bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            chat_payload("alice", "111"),
        ));
        ctx.bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            chat_payload("alice", "111"),
        ));
        ctx.bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            chat_payload("bob", "222"),
        ));
        let req = WsEnvelope {
            id: Some("av2".to_owned()),
            inner: WsRequest::GetActiveViewers,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        let viewers = json["viewers"].as_array().unwrap();
        assert_eq!(viewers.len(), 2);
        let ids: HashSet<&str> = viewers.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert!(ids.contains("111"));
        assert!(ids.contains("222"));
    }

    #[tokio::test]
    async fn get_active_viewers_ignores_non_chat_events() {
        let ctx = make_ctx(false, false);
        ctx.bus.publish(Event::new(
            EventSource::Twitch,
            "platform.connection.changed",
            serde_json::json!({ "state": "connected" }),
        ));
        ctx.bus.publish(Event::new(
            EventSource::Core,
            "action.start",
            serde_json::json!({}),
        ));
        let req = WsEnvelope {
            id: Some("av3".to_owned()),
            inner: WsRequest::GetActiveViewers,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["viewers"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn authenticate_empty_token_returns_auth_failed() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a3".to_owned()),
            inner: WsRequest::Auth {
                token: String::new(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED error, got {other:?}"),
        }
        assert!(!ctx.client.authenticated.load(Ordering::Acquire));
    }

    fn make_ctx_with_overlay_root(root: std::path::PathBuf) -> DispatchContext {
        let mut ctx = make_ctx(false, false);
        ctx.overlay_root = Arc::new(root);
        ctx
    }

    #[tokio::test]
    async fn get_overlay_files_empty_dir_returns_empty_list() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx = make_ctx_with_overlay_root(tmp.path().to_path_buf());
        let req = WsEnvelope {
            id: Some("of1".to_owned()),
            inner: WsRequest::GetOverlayFiles { recursive: false },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["root_path"].is_string());
        assert_eq!(json["files"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_overlay_files_non_recursive_lists_only_depth_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("alerts.html"), b"<html/>").expect("write");
        std::fs::create_dir_all(root.join("sub")).expect("mkdir");
        std::fs::write(root.join("sub").join("nested.js"), b"//").expect("write nested");

        let ctx = make_ctx_with_overlay_root(root.to_path_buf());
        let req = WsEnvelope {
            id: Some("of2".to_owned()),
            inner: WsRequest::GetOverlayFiles { recursive: false },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        let files = json["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        let names: HashSet<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert!(names.contains("alerts.html"));
        assert!(names.contains("sub"));
        assert!(!names.iter().any(|n| n.contains("nested.js")));
        let alerts = files.iter().find(|f| f["name"] == "alerts.html").unwrap();
        assert_eq!(alerts["mime"], "text/html");
        assert_eq!(alerts["kind"], "file");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn get_overlay_files_symlink_escape_is_filtered() {
        let outside = tempfile::tempdir().expect("outside tempdir");
        std::fs::write(outside.path().join("secret.txt"), b"shh").expect("write secret");
        let root = tempfile::tempdir().expect("root tempdir");
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).expect("symlink");

        let ctx = make_ctx_with_overlay_root(root.path().to_path_buf());
        let req = WsEnvelope {
            id: Some("of3".to_owned()),
            inner: WsRequest::GetOverlayFiles { recursive: true },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        let files = json["files"].as_array().unwrap();
        assert!(
            !files
                .iter()
                .any(|f| f["name"].as_str().unwrap().contains("secret"))
        );
    }
}
