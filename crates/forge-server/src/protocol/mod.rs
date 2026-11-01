mod context;
mod envelope;
mod handlers;
mod helpers;
mod introspect;

use crate::delivery::bus_adapter::ClientFilterSet;

use crate::access::policy::RequestAccess;
pub(crate) use context::{DispatchContext, Services, Session};
pub(crate) use envelope::merge_payload;
pub use envelope::{WireEventFilter, WsEnvelope, WsRequest};
pub(crate) use envelope::{WsErrorCode, WsResponse, serialize_response_frame};
pub(crate) use handlers::{
    handle_do_action, handle_get_actions, handle_get_active_viewers, handle_get_events,
    handle_get_global, handle_get_globals, handle_get_info, handle_get_overlay_files,
    handle_get_user_globals, handle_replay_event, handle_set_global, handle_trigger_code_event,
};

use context::{handle_authenticate, is_authenticated, unauthenticated};
use helpers::parse_wire_filters;

const MAX_SUBSCRIPTIONS_PER_CLIENT: usize = 256;

fn too_many_subscriptions() -> WsResponse {
    WsResponse::error(
        WsErrorCode::TooManySubscriptions,
        format!("a connection holds at most {MAX_SUBSCRIPTIONS_PER_CLIENT} event filters"),
    )
}

fn invalid_filter(refusal: helpers::UnknownEventSource) -> WsResponse {
    WsResponse::error(WsErrorCode::InvalidPayload, refusal.to_string())
}

pub(crate) async fn dispatch(
    req: WsEnvelope<WsRequest>,
    ctx: &DispatchContext,
) -> WsEnvelope<WsResponse> {
    let id = req.id.clone();
    let inner = route(req.inner, ctx).await;
    WsEnvelope { id, inner }
}

async fn route(req: WsRequest, ctx: &DispatchContext) -> WsResponse {
    if access_refused(req.access(), ctx) {
        return unauthenticated();
    }
    let services = &ctx.services;
    match req {
        WsRequest::Auth {
            token,
            overlay_credential,
            preview_connection,
        } => handle_authenticate(token, overlay_credential, preview_connection, ctx).await,
        WsRequest::Subscribe { events } => handle_subscribe(events, ctx).await,
        WsRequest::Unsubscribe { events } => handle_unsubscribe(events, ctx).await,
        WsRequest::GetInfo => handle_get_info(services).await,
        WsRequest::GetActions => handle_get_actions(services).await,
        WsRequest::DoAction { action_id, args } => {
            handle_do_action(action_id, args, services).await
        }
        WsRequest::GetGlobals => handle_get_globals(services).await,
        WsRequest::GetGlobal { name } => handle_get_global(name, services).await,
        WsRequest::SetGlobal {
            name,
            value,
            persisted,
        } => handle_set_global(name, value, persisted, services).await,
        WsRequest::GetUserGlobals {
            broadcaster_id,
            user_id,
        } => handle_get_user_globals(broadcaster_id, user_id, services).await,
        WsRequest::TriggerCodeEvent { name, args } => {
            handle_trigger_code_event(name, args, services).await
        }
        WsRequest::GetEvents { limit, since } => handle_get_events(limit, since, services).await,
        WsRequest::ReplayEvent { event_id } => handle_replay_event(event_id, services).await,
        WsRequest::GetActiveViewers => handle_get_active_viewers(services).await,
        WsRequest::GetOverlayFiles { recursive } => {
            handle_get_overlay_files(recursive, services).await
        }
    }
}

fn access_refused(access: RequestAccess, ctx: &DispatchContext) -> bool {
    access.needs_credential(&ctx.services.auth_state) && !is_authenticated(ctx)
}

async fn handle_subscribe(events: Vec<WireEventFilter>, ctx: &DispatchContext) -> WsResponse {
    let new_filters = match parse_wire_filters(&events) {
        Ok(filters) => filters,
        Err(message) => return invalid_filter(message),
    };
    let adapter = &ctx.services.bus_adapter;
    let client_id = ctx.session.client.id;
    let mut current = adapter.current_subscriptions(client_id).await;
    current.extend(new_filters);
    if current.len() > MAX_SUBSCRIPTIONS_PER_CLIENT {
        return too_many_subscriptions();
    }
    adapter
        .update_subscriptions(client_id, ClientFilterSet::new(current))
        .await;
    WsResponse::Ok(serde_json::json!({ "subscribed": events }))
}

async fn handle_unsubscribe(events: Vec<WireEventFilter>, ctx: &DispatchContext) -> WsResponse {
    let to_remove = match parse_wire_filters(&events) {
        Ok(filters) => filters,
        Err(message) => return invalid_filter(message),
    };
    let adapter = &ctx.services.bus_adapter;
    let client_id = ctx.session.client.id;
    let mut current = adapter.current_subscriptions(client_id).await;
    current.retain(|f| !to_remove.contains(f));
    adapter
        .update_subscriptions(client_id, ClientFilterSet::new(current))
        .await;
    WsResponse::Ok(serde_json::json!({ "unsubscribed": events }))
}

#[cfg(test)]
mod tests {
    use forge_types::{Action, ActionId, EventId, QueueId, Secret, SubActionStep, Variant};
    use std::collections::{BTreeSet, HashSet};
    use std::sync::Arc;

    use std::collections::BTreeMap;

    use forge_events::{
        Event, EventChannel, EventObserver, EventPublisher, EventSource, EventSubscription,
    };
    use forge_runtime::{EventHistory, RuntimeError};
    use forge_storage::{DataProvider, StorageError};

    use super::*;
    use crate::delivery::bus_adapter::{ClientFilterSet, EventFilter};
    use crate::lifecycle::server_info::ServerInfo;
    use forge_storage::GlobalEntry;
    use time::OffsetDateTime;

    use crate::test_helpers::{TestDataProvider, dispatch_ctx, registered_dispatch_ctx, test_dp};

    const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(120);

    fn make_ctx(authenticated: bool, auth_required_for_reads: bool) -> DispatchContext {
        let mut tdp = TestDataProvider::new();
        tdp.action().expect_list().returning(|| Ok(vec![]));
        tdp.action().expect_get().returning(|_| Ok(None));
        tdp.globals().expect_get().returning(|_| Ok(None));
        tdp.globals().expect_set().returning(|_, _, _| Ok(()));
        tdp.user_globals()
            .expect_list_for_broadcaster()
            .returning(|_| Ok(vec![]));
        let ctx = dispatch_ctx(Arc::new(tdp), auth_required_for_reads);
        if authenticated {
            ctx.session
                .client
                .mark_bearer_authenticated(ctx.services.auth_state.token_generation());
        }
        ctx
    }

    fn make_ctx_with_dp(authenticated: bool, dp: Arc<dyn DataProvider>) -> DispatchContext {
        let ctx = dispatch_ctx(dp, false);
        if authenticated {
            ctx.session
                .client
                .mark_bearer_authenticated(ctx.services.auth_state.token_generation());
        }
        ctx
    }

    async fn make_registered_ctx(
        authenticated: bool,
        auth_required_for_reads: bool,
    ) -> DispatchContext {
        let ctx = registered_dispatch_ctx(test_dp(), auth_required_for_reads).await;
        if authenticated {
            ctx.session
                .client
                .mark_bearer_authenticated(ctx.services.auth_state.token_generation());
        }
        ctx
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
            sub_actions: vec![SubActionStep {
                kind_id: "core.log.write".to_owned(),
                config: BTreeMap::from([
                    ("level".to_owned(), Variant::String("info".to_owned())),
                    ("message".to_owned(), Variant::String("hello".to_owned())),
                ]),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        }
    }

    #[test]
    fn parse_request_envelope_get_info() {
        let json = r#"{"id":"1","request":"getInfo"}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, Some("1".into()));
        assert!(matches!(env.inner, WsRequest::GetInfo));
    }

    #[test]
    fn parse_request_envelope_do_action_camel_case_fields() {
        let json = r#"{"id":"5","request":"doAction","actionId":"abc","args":{"user":"Alice"}}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, Some("5".into()));
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
            id: Some("1".into()),
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
            id: Some("2".into()),
            inner: WsResponse::Error {
                code: WsErrorCode::NotFound,
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
    async fn mutating_methods_without_auth_return_unauthenticated() {
        let ctx = make_ctx(false, false);
        let requests = vec![
            WsRequest::DoAction {
                action_id: "fake-id".to_owned(),
                args: serde_json::Value::Null,
            },
            WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(42),
                persisted: true,
            },
            WsRequest::TriggerCodeEvent {
                name: "my_event".to_owned(),
                args: serde_json::json!({}),
            },
        ];
        for inner in requests {
            let req = WsEnvelope {
                id: Some("x".into()),
                inner,
            };
            let label = format!("{:?}", req.inner);
            let resp = dispatch(req, &ctx).await;
            match resp.inner {
                WsResponse::Error { code, .. } => {
                    assert_eq!(code.as_str(), "UNAUTHENTICATED", "for {label}")
                }
                other => panic!("expected UNAUTHENTICATED for {label}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn get_actions_returns_empty_list_for_null_dp() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("5".into()),
            inner: WsRequest::GetActions,
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("5".into()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
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
            id: Some("5".into()),
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
            id: Some("6".into()),
            inner: WsRequest::DoAction {
                action_id: action_id_str,
                args: serde_json::json!({ "user": "Alice", "count": 3 }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("6".into()));
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
            id: Some("7".into()),
            inner: WsRequest::DoAction {
                action_id: nonexistent,
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("7".into()));
        match &resp.inner {
            WsResponse::Error { code, message } => {
                assert_eq!(code.as_str(), "NOT_FOUND");
                assert!(!message.is_empty());
            }
            other => panic!("expected NOT_FOUND error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn do_action_emits_no_action_invoked_but_engine_still_emits_action_start() {
        let action = sample_action();
        let action_id_str = action.id.to_string();
        let action_clone = action.clone();
        let mut tdp = TestDataProvider::new();
        let listed = action.clone();
        tdp.action()
            .expect_get()
            .returning(move |_| Ok(Some(action_clone.clone())));
        tdp.action()
            .expect_list()
            .returning(move || Ok(vec![listed.clone()]));
        tdp.trigger_instance()
            .expect_instances_by_action()
            .returning(|| Ok(Default::default()));
        tdp.history().expect_save().returning(|_| Ok(()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(true, dp);
        let mut sub = ctx.services.bus.observe("ws_dispatch_test");

        let req = WsEnvelope {
            id: Some("9".into()),
            inner: WsRequest::DoAction {
                action_id: action_id_str,
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(serialize_response_frame(&resp)["status"], "ok");

        let mut kinds = Vec::new();
        for _ in 0..40 {
            match tokio::time::timeout(std::time::Duration::from_secs(30), sub.recv()).await {
                Ok(Ok(ev)) => {
                    let kind = ev.kind.clone();
                    kinds.push(kind);
                    if ev.kind == "action.start" {
                        break;
                    }
                }
                _ => break,
            }
        }

        assert!(
            kinds.iter().any(|k| k == "action.start"),
            "engine must still emit action.start for the WS-dispatched execution"
        );
        assert!(
            !kinds.iter().any(|k| k == "action.invoked"),
            "WS do_action must not emit the removed action.invoked event"
        );
    }

    #[tokio::test]
    async fn get_info_succeeds_without_auth_by_default() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("8".into()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn read_only_blocked_when_auth_required_for_reads() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("9".into()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_events_empty_bus_returns_empty_list() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("13".into()),
            inner: WsRequest::GetEvents {
                limit: None,
                since: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["events"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_events_with_three_events_returns_all_in_shape() {
        let ctx = make_ctx(false, false);
        for i in 0u32..3 {
            ctx.services.bus.publish(Event::new(
                EventSource::Core,
                "test.event",
                serde_json::json!({ "i": i }),
            ));
        }
        let req = WsEnvelope {
            id: Some("13".into()),
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

    struct ScriptedHistory {
        channel: EventChannel,
        asked: std::sync::Mutex<Vec<(usize, Option<EventId>)>>,
        replay_failure: std::sync::Mutex<Option<RuntimeError>>,
    }

    impl ScriptedHistory {
        fn failing_replay_with(failure: Option<RuntimeError>) -> Arc<Self> {
            Arc::new(Self {
                channel: EventChannel::new(16),
                asked: std::sync::Mutex::new(Vec::new()),
                replay_failure: std::sync::Mutex::new(failure),
            })
        }

        fn asked(&self) -> Vec<(usize, Option<EventId>)> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl EventPublisher for ScriptedHistory {
        fn publish(&self, event: Event) {
            self.channel.publish(event);
        }
    }

    impl EventObserver for ScriptedHistory {
        fn observe(&self, _consumer: &'static str) -> EventSubscription {
            self.channel.subscribe()
        }
    }

    #[async_trait::async_trait]
    impl EventHistory for ScriptedHistory {
        async fn recent_since(&self, limit: usize, since: Option<EventId>) -> Vec<Event> {
            self.asked.lock().unwrap().push((limit, since));
            Vec::new()
        }

        async fn replay_and_publish(&self, _event_id: EventId) -> Result<(), RuntimeError> {
            match self.replay_failure.lock().unwrap().take() {
                Some(failure) => Err(failure),
                None => Ok(()),
            }
        }
    }

    fn ctx_over(history: Arc<ScriptedHistory>) -> DispatchContext {
        let mut ctx = make_ctx(true, false);
        ctx.services.bus = history;
        ctx
    }

    fn get_events(limit: Option<u32>, since: Option<String>) -> WsEnvelope<WsRequest> {
        WsEnvelope {
            id: Some("13".into()),
            inner: WsRequest::GetEvents { limit, since },
        }
    }

    #[tokio::test]
    async fn get_events_asks_history_for_the_requested_limit_clamped_to_the_maximum() {
        let history = ScriptedHistory::failing_replay_with(None);
        let ctx = ctx_over(Arc::clone(&history));

        for limit in [None, Some(499), Some(500), Some(501), Some(u32::MAX)] {
            dispatch(get_events(limit, None), &ctx).await;
        }

        let limits: Vec<usize> = history
            .asked()
            .into_iter()
            .map(|(limit, _)| limit)
            .collect();
        assert_eq!(limits, vec![100, 499, 500, 500, 500]);
    }

    #[tokio::test]
    async fn get_events_forwards_the_since_anchor_to_history() {
        let history = ScriptedHistory::failing_replay_with(None);
        let ctx = ctx_over(Arc::clone(&history));
        let anchor = EventId::new();

        dispatch(get_events(Some(1), Some(anchor.to_string())), &ctx).await;

        assert_eq!(history.asked(), vec![(1, Some(anchor))]);
    }

    #[tokio::test]
    async fn get_events_rejects_a_malformed_since_without_asking_history() {
        let history = ScriptedHistory::failing_replay_with(None);
        let ctx = ctx_over(Arc::clone(&history));

        let resp = dispatch(get_events(None, Some("not-an-event-id".to_owned())), &ctx).await;

        assert_eq!(error_code(&resp), Some("INVALID_PAYLOAD"));
        assert!(history.asked().is_empty());
    }

    #[test]
    fn an_arg_shaped_like_a_tagged_variant_stays_a_plain_object() {
        let stack = super::helpers::build_arg_stack(serde_json::json!({
            "reward": { "type": "bonus", "value": 5 },
        }));

        let Some(forge_types::Variant::Object(reward)) = stack.get("reward") else {
            panic!("the arg must arrive as an object");
        };
        assert_eq!(
            reward.get("type"),
            Some(&forge_types::Variant::String("bonus".to_owned()))
        );
    }

    const PRIVATE_VALUE: &str = "sk_live_private_value";
    const PRIVATE_KEY: &str = "sk_live_private_key";

    fn too_deep_to_represent() -> serde_json::Value {
        (0..=forge_types::VARIANT_MAX_DEPTH).fold(serde_json::json!(PRIVATE_VALUE), |inner, _| {
            serde_json::json!([inner])
        })
    }

    #[test]
    fn rejected_action_args_log_their_shape_and_never_their_content() {
        for (args, field, expected) in [
            (serde_json::json!(PRIVATE_VALUE), "shape", "string"),
            (serde_json::json!([PRIVATE_VALUE]), "shape", "array"),
            (
                serde_json::json!({ PRIVATE_KEY: too_deep_to_represent() }),
                "dropped",
                "1",
            ),
        ] {
            let (stack, lines) =
                forge_test_support::log_capture::capture(tracing::Level::TRACE, || {
                    super::helpers::build_arg_stack(args)
                });

            let warnings = forge_test_support::log_capture::forge_warnings(&lines);
            let [warning] = warnings.as_slice() else {
                panic!("{field}: expected one warning, got {lines:?}");
            };
            assert_eq!(warning.field(field), Some(expected));
            assert!(stack.snapshot().is_empty());
            assert!(
                !lines
                    .iter()
                    .any(|line| line.mentions(PRIVATE_VALUE) || line.mentions(PRIVATE_KEY)),
                "{lines:?}"
            );
        }
    }

    #[test]
    fn a_dropped_action_arg_leaves_its_representable_siblings_in_place() {
        let stack = super::helpers::build_arg_stack(serde_json::json!({
            "broken": too_deep_to_represent(),
            "kept": 1,
        }));

        assert_eq!(
            stack.snapshot(),
            BTreeMap::from([("kept".to_owned(), Variant::Int(1))])
        );
    }

    #[tokio::test]
    async fn replay_event_maps_history_outcomes_to_wire_error_codes() {
        let ghost = EventId::new();
        let cases = [
            (
                ghost.to_string(),
                Some(RuntimeError::EventNotFound(ghost)),
                "NOT_FOUND",
            ),
            (
                EventId::new().to_string(),
                Some(RuntimeError::Storage(StorageError::Connection {
                    reason: "database is locked".to_owned(),
                })),
                "RUNTIME_ERROR",
            ),
            ("not-an-event-id".to_owned(), None, "INVALID_PAYLOAD"),
        ];

        for (event_id, failure, expected) in cases {
            let ctx = ctx_over(ScriptedHistory::failing_replay_with(failure));
            let req = WsEnvelope {
                id: Some("14".into()),
                inner: WsRequest::ReplayEvent {
                    event_id: event_id.clone(),
                },
            };

            let resp = dispatch(req, &ctx).await;

            assert_eq!(error_code(&resp), Some(expected), "replay of {event_id:?}");
        }
    }

    #[tokio::test]
    async fn a_failed_replay_keeps_the_storage_cause_off_the_wire() {
        const CAUSE: &str = "database /home/streamer/forge.db is locked";
        let ctx = ctx_over(ScriptedHistory::failing_replay_with(Some(
            RuntimeError::Storage(StorageError::Connection {
                reason: CAUSE.to_owned(),
            }),
        )));
        let req = WsEnvelope {
            id: Some("15".into()),
            inner: WsRequest::ReplayEvent {
                event_id: EventId::new().to_string(),
            },
        };

        let resp = dispatch(req, &ctx).await;

        assert!(!format!("{resp:?}").contains("forge.db"), "{resp:?}");
    }

    #[tokio::test]
    async fn replay_event_valid_id_returns_ok_and_emits_replayed_event() {
        let ctx = make_ctx(true, false);
        let original = Event::new(EventSource::Core, "test.original", serde_json::json!({}));
        let original_id = original.id.to_string();
        ctx.services.bus.publish(original);

        let mut bus_sub = ctx.services.bus.observe("ws_dispatch_test");

        let req = WsEnvelope {
            id: Some("14".into()),
            inner: WsRequest::ReplayEvent {
                event_id: original_id,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let replayed = tokio::time::timeout(HANG_GUARD, bus_sub.recv())
            .await
            .expect("the bus delivered nothing")
            .unwrap();
        assert_eq!(replayed.kind, "test.original");
        assert!(replayed.replay);
    }

    #[tokio::test]
    async fn subscribe_registers_filter_for_client() {
        let ctx = make_registered_ctx(false, false).await;
        let req = WsEnvelope {
            id: Some("2".into()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("chat.message".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("2".into()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(
            json["subscribed"],
            serde_json::json!([{ "source": "twitch", "type": "chat.message" }])
        );

        let subs = ctx
            .services
            .bus_adapter
            .current_subscriptions(ctx.session.client.id)
            .await;
        assert!(subs.contains(&EventFilter::new(
            Some(EventSource::Twitch),
            Some("chat.message".to_owned()),
        )));
    }

    #[tokio::test]
    async fn unsubscribe_removes_filter_from_client() {
        let ctx = make_registered_ctx(false, false).await;
        let filter = EventFilter::new(Some(EventSource::Twitch), Some("chat.message".to_owned()));
        ctx.services
            .bus_adapter
            .update_subscriptions(
                ctx.session.client.id,
                ClientFilterSet::new(HashSet::from([filter.clone()])),
            )
            .await;

        let req = WsEnvelope {
            id: Some("3".into()),
            inner: WsRequest::Unsubscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("chat.message".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("3".into()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(
            json["unsubscribed"],
            serde_json::json!([{ "source": "twitch", "type": "chat.message" }])
        );

        let subs = ctx
            .services
            .bus_adapter
            .current_subscriptions(ctx.session.client.id)
            .await;
        assert!(!subs.contains(&filter));
    }

    #[tokio::test]
    async fn subscribe_is_additive_across_calls() {
        let ctx = make_registered_ctx(false, false).await;
        let req1 = WsEnvelope {
            id: Some("2".into()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: None,
                }],
            },
        };
        dispatch(req1, &ctx).await;

        let req2 = WsEnvelope {
            id: Some("3".into()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("obs".to_owned()),
                    kind: None,
                }],
            },
        };
        dispatch(req2, &ctx).await;

        let subs = ctx
            .services
            .bus_adapter
            .current_subscriptions(ctx.session.client.id)
            .await;
        assert_eq!(subs.len(), 2);
    }

    #[tokio::test]
    async fn a_filter_naming_an_unknown_source_is_refused_and_changes_no_subscription() {
        let ctx = make_registered_ctx(false, false).await;
        let known_and_unknown = || {
            vec![
                WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: None,
                },
                WireEventFilter {
                    source: Some("not-a-source".to_owned()),
                    kind: None,
                },
            ]
        };

        for request in [
            WsRequest::Subscribe {
                events: known_and_unknown(),
            },
            WsRequest::Unsubscribe {
                events: known_and_unknown(),
            },
        ] {
            let resp = dispatch(
                WsEnvelope {
                    id: Some(1.into()),
                    inner: request,
                },
                &ctx,
            )
            .await;
            let json = serialize_response_frame(&resp);

            assert_eq!(json["status"], "error");
            assert_eq!(json["error"]["code"], "INVALID_PAYLOAD");
            assert!(
                json["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("not-a-source")),
                "{json}"
            );
        }
        let subs = ctx
            .services
            .bus_adapter
            .current_subscriptions(ctx.session.client.id)
            .await;
        assert!(subs.is_empty(), "the refused frame left {subs:?}");
    }

    #[tokio::test]
    async fn subscribe_wildcard_type_registers_kind_wildcard() {
        let ctx = make_registered_ctx(false, false).await;
        let req = WsEnvelope {
            id: Some("4".into()),
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

        let subs = ctx
            .services
            .bus_adapter
            .current_subscriptions(ctx.session.client.id)
            .await;
        assert!(subs.contains(&EventFilter::new(Some(EventSource::Twitch), None)));
    }

    #[tokio::test]
    async fn get_info_no_clients_returns_empty_connected_clients() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("1".into()),
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
        assert_eq!(json["available_platforms"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn get_info_with_connected_client_returns_telemetry() {
        let ctx = make_registered_ctx(false, false).await;
        ctx.services
            .bus_adapter
            .attach_session(ctx.session.client.id, Arc::clone(&ctx.session.client))
            .await;

        let req = WsEnvelope {
            id: Some("1".into()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let clients = json["connected_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert!(clients[0]["client_id"].as_str().unwrap().starts_with("ws_"));
        assert_eq!(clients[0]["bytes_sent"], 0);
        assert_eq!(clients[0]["events_per_second"], 0.0);
        assert_eq!(clients[0]["subscriptions"], serde_json::json!([]));
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
            id: Some("8".into()),
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
            id: Some("9".into()),
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
            id: Some("9".into()),
            inner: WsRequest::GetGlobal {
                name: "missing".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "NOT_FOUND"),
            other => panic!("expected NOT_FOUND error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_global_with_auth_returns_ok() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("10".into()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(99),
                persisted: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(
            (&json["status"], &json["ok"]),
            (&serde_json::json!("ok"), &serde_json::json!(true))
        );
    }

    #[tokio::test]
    async fn set_global_event_payloads_match_the_golden() {
        let mut tdp = TestDataProvider::new();
        tdp.globals()
            .expect_get()
            .returning(|_| Ok(Some(forge_types::Variant::Int(41))));
        tdp.globals().expect_set().returning(|_, _, _| Ok(()));
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(true, dp);
        let mut bus_sub = ctx.services.bus.observe("ws_dispatch_test");
        for (value, persisted) in [
            (serde_json::json!(99), false),
            (serde_json::json!("ninety"), true),
        ] {
            let req = WsEnvelope {
                id: Some("10".into()),
                inner: WsRequest::SetGlobal {
                    name: "counter".to_owned(),
                    value,
                    persisted,
                },
            };
            dispatch(req, &ctx).await;
        }

        let mut events = Vec::new();
        while events.len() < 2 {
            let event = tokio::time::timeout(HANG_GUARD, bus_sub.recv())
                .await
                .expect("the bus delivered nothing")
                .unwrap();
            if event.kind == "global.set" {
                events.push(event);
            }
        }
        assert_matches_golden(&events, "global.set.json");
    }

    #[tokio::test]
    async fn set_global_null_value_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("10".into()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::Value::Null,
                persisted: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "INVALID_PAYLOAD"),
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
            id: Some("11".into()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["globals"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_user_globals_with_user_id_returns_filtered_globals() {
        let entry = sample_user_global_entry();
        let other_entry = forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "11111111".to_string(),
            name: "other".to_string(),
            value: forge_types::Variant::Int(0),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        };
        let stored = [entry, other_entry];
        let mut tdp = TestDataProvider::new();
        tdp.user_globals()
            .expect_list_for_user()
            .returning(move |broadcaster_id, user_id| {
                Ok(stored
                    .iter()
                    .filter(|e| e.broadcaster_id == broadcaster_id && e.user_id == user_id)
                    .cloned()
                    .collect())
            });
        let dp: Arc<dyn DataProvider> = Arc::new(tdp);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("11".into()),
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
            id: Some("11".into()),
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
    async fn trigger_code_event_with_auth_publishes_bus_event() {
        let ctx = make_ctx(true, false);
        let mut bus_sub = ctx.services.bus.observe("ws_dispatch_test");
        let req = WsEnvelope {
            id: Some("12".into()),
            inner: WsRequest::TriggerCodeEvent {
                name: "my_event".to_owned(),
                args: serde_json::json!({ "scene": "Gameplay" }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let event = tokio::time::timeout(HANG_GUARD, bus_sub.recv())
            .await
            .expect("the bus delivered nothing")
            .unwrap();
        assert_eq!(event.source, EventSource::Server);
        assert_eq!(event.kind, "custom.my_event");
        assert_eq!(event.payload["scene"], "Gameplay");
    }

    #[tokio::test]
    async fn trigger_code_event_invalid_name_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("12".into()),
            inner: WsRequest::TriggerCodeEvent {
                name: "Invalid-Name!".to_owned(),
                args: serde_json::json!({}),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn trigger_code_event_empty_name_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("12".into()),
            inner: WsRequest::TriggerCodeEvent {
                name: String::new(),
                args: serde_json::json!({}),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn authenticate_correct_token_marks_session_authenticated() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a1".into()),
            inner: WsRequest::Auth {
                token: Some(Secret::new("test-token")),
                overlay_credential: None,
                preview_connection: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Ok(data) => {
                assert_eq!(data.get("authenticated"), Some(&serde_json::json!(true)));
            }
            other => panic!("expected ok response, got {other:?}"),
        }
        assert_eq!(
            ctx.session.client.bearer_generation(),
            Some(ctx.services.auth_state.token_generation()),
            "the session must record the generation of the token it presented"
        );
    }

    #[tokio::test]
    async fn authenticate_wrong_token_returns_auth_failed() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a2".into()),
            inner: WsRequest::Auth {
                token: Some(Secret::new("wrong-token")),
                overlay_credential: None,
                preview_connection: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED error, got {other:?}"),
        }
        assert!(ctx.session.client.bearer_generation().is_none());
    }

    #[tokio::test]
    async fn get_active_viewers_empty_bus_returns_empty_list() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("av1".into()),
            inner: WsRequest::GetActiveViewers,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["viewers"].as_array().unwrap().len(), 0);
    }

    fn chat_event(
        source: EventSource,
        kind: &str,
        viewer: Option<(&str, &str)>,
        user: Option<serde_json::Value>,
    ) -> Event {
        let mut payload = serde_json::json!({ "message": "hi" });
        if let Some(user) = user {
            payload["user"] = user;
        }
        if let Some((id, name)) = viewer {
            forge_types::ChatViewer::new(id, name).attach(&mut payload);
        }
        Event::new(source, kind, payload)
    }

    async fn active_viewers(ctx: &DispatchContext) -> Vec<serde_json::Value> {
        let req = WsEnvelope {
            id: Some("av".into()),
            inner: WsRequest::GetActiveViewers,
        };
        let json = serialize_response_frame(&dispatch(req, ctx).await);
        json["viewers"].as_array().unwrap().clone()
    }

    fn viewer_of<'a>(
        viewers: &'a [serde_json::Value],
        platform: &str,
        id: &str,
    ) -> &'a serde_json::Value {
        viewers
            .iter()
            .find(|v| v["platform"] == platform && v["id"] == id)
            .unwrap_or_else(|| panic!("no {platform} viewer {id} in {viewers:?}"))
    }

    #[tokio::test]
    async fn get_active_viewers_dedupes_by_platform_and_envelope_id() {
        let ctx = make_ctx(false, false);
        let twitch = |id, name| {
            chat_event(
                EventSource::Twitch,
                "twitch.channel.chat.message",
                Some((id, name)),
                None,
            )
        };
        ctx.services.bus.publish(twitch("111", "alice"));
        ctx.services.bus.publish(twitch("111", "alice"));
        ctx.services.bus.publish(twitch("222", "bob"));
        ctx.services.bus.publish(chat_event(
            EventSource::Kick,
            "kick.chat.message.sent",
            Some(("111", "alice")),
            None,
        ));

        let viewers = active_viewers(&ctx).await;

        assert_eq!(viewers.len(), 3);
        viewer_of(&viewers, "twitch", "111");
        viewer_of(&viewers, "twitch", "222");
        viewer_of(&viewers, "kick", "111");
    }

    #[tokio::test]
    async fn get_active_viewers_serves_youtube_and_kick_chatters_by_envelope_name() {
        let ctx = make_ctx(false, false);
        ctx.services.bus.publish(chat_event(
            EventSource::YouTube,
            "youtube.chat.message",
            Some(("UCyt", "Yuliia")),
            None,
        ));
        ctx.services.bus.publish(chat_event(
            EventSource::Kick,
            "kick.chat.message.sent",
            Some(("77", "kicker")),
            None,
        ));

        let viewers = active_viewers(&ctx).await;

        assert_eq!(viewer_of(&viewers, "youtube", "UCyt")["login"], "Yuliia");
        assert_eq!(viewer_of(&viewers, "kick", "77")["login"], "kicker");
    }

    #[tokio::test]
    async fn get_active_viewers_keeps_the_twitch_login_over_the_envelope_name() {
        let ctx = make_ctx(false, false);
        ctx.services.bus.publish(chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            Some(("111", "Alice_Display")),
            Some(serde_json::json!({ "login": "alice", "roles": ["moderator", "vip"] })),
        ));
        ctx.services.bus.publish(chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            Some(("222", "Bob_Display")),
            Some(serde_json::json!({ "login": "", "roles": [] })),
        ));

        let viewers = active_viewers(&ctx).await;

        let alice = viewer_of(&viewers, "twitch", "111");
        assert_eq!(alice["login"], "alice");
        assert_eq!(alice["roles"], serde_json::json!(["moderator", "vip"]));
        assert_eq!(viewer_of(&viewers, "twitch", "222")["login"], "Bob_Display");
    }

    #[tokio::test]
    async fn get_active_viewers_skips_chat_lines_without_a_viewer_envelope() {
        let ctx = make_ctx(false, false);
        ctx.services.bus.publish(chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            None,
            Some(serde_json::json!({ "login": "legacy", "id": "9", "roles": [] })),
        ));
        ctx.services.bus.publish(chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            Some(("", "nameless")),
            None,
        ));

        assert!(active_viewers(&ctx).await.is_empty());
    }

    #[tokio::test]
    async fn get_active_viewers_counts_only_events_from_chat_platform_sources() {
        let ctx = make_ctx(false, false);
        ctx.services.bus.publish(chat_event(
            EventSource::Server,
            "server.chat.relay",
            Some(("1", "relayed")),
            None,
        ));
        ctx.services.bus.publish(chat_event(
            EventSource::Core,
            "core.chat.echo",
            Some(("2", "echoed")),
            None,
        ));
        ctx.services.bus.publish(chat_event(
            EventSource::Kick,
            "kick.chat.message.sent",
            Some(("3", "kicker")),
            None,
        ));

        let viewers = active_viewers(&ctx).await;

        assert_eq!(viewers.len(), 1);
        viewer_of(&viewers, "kick", "3");
    }

    #[tokio::test]
    async fn get_active_viewers_drops_chatters_older_than_the_window() {
        let ctx = make_ctx(false, false);
        let mut stale = chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            Some(("1", "stale")),
            None,
        );
        stale.timestamp = OffsetDateTime::now_utc() - time::Duration::seconds(301);
        let mut fresh = chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            Some(("2", "fresh")),
            None,
        );
        fresh.timestamp = OffsetDateTime::now_utc() - time::Duration::seconds(290);
        ctx.services.bus.publish(stale);
        ctx.services.bus.publish(fresh);

        let viewers = active_viewers(&ctx).await;

        assert_eq!(viewers.len(), 1);
        viewer_of(&viewers, "twitch", "2");
    }

    #[tokio::test]
    async fn get_info_reports_the_platforms_the_server_was_configured_with() {
        let mut ctx = make_ctx(false, false);
        ctx.services.server_info = ServerInfo::serving(BTreeSet::from([
            forge_types::PlatformId::Kick,
            forge_types::PlatformId::Twitch,
        ]));
        let req = WsEnvelope {
            id: Some("ap".into()),
            inner: WsRequest::GetInfo,
        };

        let json = serialize_response_frame(&dispatch(req, &ctx).await);

        assert_eq!(
            json["available_platforms"],
            serde_json::json!(["twitch", "kick"])
        );
    }

    #[tokio::test]
    async fn get_info_reports_no_platforms_when_none_are_configured() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("ap0".into()),
            inner: WsRequest::GetInfo,
        };

        let json = serialize_response_frame(&dispatch(req, &ctx).await);

        assert_eq!(json["available_platforms"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn get_active_viewers_ignores_non_chat_events() {
        let ctx = make_ctx(false, false);
        ctx.services.bus.publish(Event::new(
            EventSource::Twitch,
            "platform.connection.changed",
            serde_json::json!({ "state": "connected" }),
        ));
        ctx.services.bus.publish(Event::new(
            EventSource::Core,
            "action.start",
            serde_json::json!({}),
        ));
        let req = WsEnvelope {
            id: Some("av3".into()),
            inner: WsRequest::GetActiveViewers,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["viewers"].as_array().unwrap().len(), 0);
    }

    const OVERLAY_PAGE_CREDENTIAL: &str = "3d90f1a7c46b28e5a0d7f3b1c9e64a08";

    async fn make_overlay_credential_ctx(known: bool) -> DispatchContext {
        let mut tdp = TestDataProvider::new();
        tdp.overlay()
            .expect_get_by_credential()
            .returning(move |credential| {
                Ok(
                    (known && credential.expose() == OVERLAY_PAGE_CREDENTIAL).then(|| {
                        forge_storage::OverlayDefinition {
                            id: forge_storage::OverlayId::new("goal-box"),
                            display_name: "Sub goal".to_owned(),
                            kind_id: "overlay.goal".to_owned(),
                            enabled: true,
                            position: 0,
                            config: forge_storage::OverlayConfig::new(),
                            config_schema_version: 1,
                            generator_version: 1,
                            source_overrides: Vec::new(),
                            credential: forge_types::OverlayCredential::new(
                                OVERLAY_PAGE_CREDENTIAL,
                            ),
                            created_at: OffsetDateTime::UNIX_EPOCH,
                            updated_at: OffsetDateTime::UNIX_EPOCH,
                        }
                    }),
                )
            });
        tdp.action().expect_list().returning(|| Ok(vec![]));
        registered_dispatch_ctx(Arc::new(tdp), true).await
    }

    fn auth_frame(
        token: Option<&str>,
        overlay_credential: Option<&str>,
        preview_connection: bool,
    ) -> WsEnvelope<WsRequest> {
        WsEnvelope {
            id: Some("auth".into()),
            inner: WsRequest::Auth {
                token: token.map(forge_types::Secret::new),
                overlay_credential: overlay_credential.map(forge_types::OverlayCredential::new),
                preview_connection,
            },
        }
    }

    #[tokio::test]
    async fn authenticate_refuses_a_frame_presenting_both_credential_forms_or_neither() {
        for (token, overlay_credential, label) in [
            (
                Some("test-token"),
                Some(OVERLAY_PAGE_CREDENTIAL),
                "a valid token alongside an overlay credential",
            ),
            (None, None, "an auth frame carrying no credential at all"),
        ] {
            let ctx = make_ctx(false, true);

            let resp = dispatch(auth_frame(token, overlay_credential, false), &ctx).await;

            match resp.inner {
                WsResponse::Error { code, .. } => {
                    assert_eq!(code.as_str(), "AUTH_FAILED", "{label}")
                }
                other => panic!("{label} produced {other:?} instead of AUTH_FAILED"),
            }
            assert!(
                ctx.session.client.bearer_generation().is_none(),
                "{label} still marked the session authenticated"
            );
        }
    }

    #[tokio::test]
    async fn an_overlay_credential_opens_directed_delivery_without_authenticating_the_session() {
        let ctx = make_overlay_credential_ctx(true).await;

        let resp = dispatch(auth_frame(None, Some(OVERLAY_PAGE_CREDENTIAL), false), &ctx).await;

        assert!(
            matches!(resp.inner, WsResponse::Ok(_)),
            "a page presenting its own credential was refused: {:?}",
            resp.inner
        );
        assert!(
            ctx.session.overlay_channel_swap.lock().await.is_some(),
            "the connection was never promoted, so this proves nothing about what it grants"
        );
        assert!(
            ctx.session.client.bearer_generation().is_none(),
            "a read-scoped overlay credential unlocked every authenticated method"
        );
    }

    #[tokio::test]
    async fn a_page_that_declares_itself_a_preview_is_promoted_apart_yet_addressed_the_same() {
        for (preview_connection, expected) in [
            (
                false,
                forge_runtime::OverlayReceivers {
                    sources: 1,
                    preview_tabs: 0,
                },
            ),
            (
                true,
                forge_runtime::OverlayReceivers {
                    sources: 0,
                    preview_tabs: 1,
                },
            ),
        ] {
            let ctx = make_overlay_credential_ctx(true).await;

            let resp = dispatch(
                auth_frame(None, Some(OVERLAY_PAGE_CREDENTIAL), preview_connection),
                &ctx,
            )
            .await;

            assert!(
                matches!(resp.inner, WsResponse::Ok(_)),
                "previewConnection={preview_connection} was refused: {:?}",
                resp.inner
            );
            assert_eq!(
                ctx.services
                    .bus_adapter
                    .deliver_overlay_content(
                        &forge_storage::OverlayId::new("goal-box"),
                        &serde_json::json!({ "value": "42" }),
                        None,
                    )
                    .await,
                expected,
                "previewConnection={preview_connection} was tallied as the other class"
            );
            assert_eq!(
                ctx.services
                    .bus_adapter
                    .preview_tabs()
                    .await
                    .contains(&ctx.session.client.id),
                preview_connection,
                "previewConnection={preview_connection} put the wrong row in front of the console"
            );
        }
    }

    #[tokio::test]
    async fn an_overlay_credential_no_record_carries_is_refused_and_promotes_nothing() {
        let ctx = make_overlay_credential_ctx(false).await;

        let resp = dispatch(auth_frame(None, Some(OVERLAY_PAGE_CREDENTIAL), false), &ctx).await;

        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED, got {other:?}"),
        }
        assert!(
            ctx.session.overlay_channel_swap.lock().await.is_none(),
            "an unknown credential still put the connection onto the overlay delivery channel"
        );
    }

    #[tokio::test]
    async fn authenticate_empty_token_returns_auth_failed() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a3".into()),
            inner: WsRequest::Auth {
                token: Some(Secret::new("")),
                overlay_credential: None,
                preview_connection: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error { code, .. } => assert_eq!(code.as_str(), "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED error, got {other:?}"),
        }
        assert!(ctx.session.client.bearer_generation().is_none());
    }

    fn make_ctx_with_overlay_root(root: std::path::PathBuf) -> DispatchContext {
        let mut ctx = make_ctx(false, false);
        ctx.services.overlay_root = Arc::new(root);
        ctx
    }

    #[tokio::test]
    async fn get_overlay_files_empty_dir_returns_empty_list() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx = make_ctx_with_overlay_root(tmp.path().to_path_buf());
        let req = WsEnvelope {
            id: Some("of1".into()),
            inner: WsRequest::GetOverlayFiles { recursive: false },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(
            json.get("root_path").is_none(),
            "the overlay directory path must not reach a client"
        );
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
            id: Some("of2".into()),
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
            id: Some("of3".into()),
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

    fn subscribe_to(kinds: impl IntoIterator<Item = String>) -> WsEnvelope<WsRequest> {
        WsEnvelope {
            id: Some("sub".into()),
            inner: WsRequest::Subscribe {
                events: kinds
                    .into_iter()
                    .map(|kind| WireEventFilter {
                        source: Some("twitch".to_owned()),
                        kind: Some(kind),
                    })
                    .collect(),
            },
        }
    }

    fn error_code(resp: &WsEnvelope<WsResponse>) -> Option<&str> {
        match &resp.inner {
            WsResponse::Error { code, .. } => Some(code.as_str()),
            WsResponse::Ok(_) => None,
        }
    }

    #[tokio::test]
    async fn the_filter_cap_counts_distinct_filters_and_refuses_the_one_past_it_unchanged() {
        let ctx = make_registered_ctx(false, false).await;
        let at_cap: Vec<String> = (0..MAX_SUBSCRIPTIONS_PER_CLIENT)
            .map(|n| format!("kind.{n}"))
            .collect();

        let filled = dispatch(subscribe_to(at_cap.clone()), &ctx).await;
        assert_eq!(
            error_code(&filled),
            None,
            "the cap itself must be reachable"
        );

        let repeated = dispatch(subscribe_to(at_cap.iter().take(1).cloned()), &ctx).await;
        assert_eq!(
            error_code(&repeated),
            None,
            "a filter the connection already holds adds nothing and must not count"
        );

        let over = dispatch(subscribe_to(["kind.one-too-many".to_owned()]), &ctx).await;
        assert_eq!(error_code(&over), Some("TOO_MANY_SUBSCRIPTIONS"));
        assert_eq!(
            ctx.services
                .bus_adapter
                .current_subscriptions(ctx.session.client.id)
                .await
                .len(),
            MAX_SUBSCRIPTIONS_PER_CLIENT,
            "a refused subscribe must leave the held filters as they were"
        );
    }

    #[tokio::test]
    async fn a_session_authenticated_before_a_token_rotation_is_refused_mutations() {
        let ctx = make_ctx(false, false);
        let auth = dispatch(
            WsEnvelope {
                id: Some("a".into()),
                inner: WsRequest::Auth {
                    token: Some(Secret::new("test-token")),
                    overlay_credential: None,
                    preview_connection: false,
                },
            },
            &ctx,
        )
        .await;
        assert_eq!(error_code(&auth), None, "the session never authenticated");
        let mut creds = forge_storage::credentials::MockCredentialsRepo::new();
        creds.expect_store().returning(|_, _| Ok(()));
        ctx.services.auth_state.regenerate(&creds).await.unwrap();

        let resp = dispatch(
            WsEnvelope {
                id: Some("m".into()),
                inner: WsRequest::TriggerCodeEvent {
                    name: "my_event".to_owned(),
                    args: serde_json::json!({}),
                },
            },
            &ctx,
        )
        .await;

        assert_eq!(error_code(&resp), Some("UNAUTHENTICATED"));
    }

    fn golden_keys_sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(fields) => {
                let sorted: std::collections::BTreeMap<String, serde_json::Value> = fields
                    .into_iter()
                    .map(|(key, value)| (key, golden_keys_sorted(value)))
                    .collect();
                serde_json::Value::Object(sorted.into_iter().collect())
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(golden_keys_sorted).collect())
            }
            other => other,
        }
    }

    fn assert_matches_golden(events: &[Event], fixture: &str) {
        let emitted: Vec<serde_json::Value> = events
            .iter()
            .map(|event| {
                let mut payload = event.payload.clone();
                if let Some(duration) = payload.get_mut("duration_ms") {
                    assert!(
                        duration.is_u64(),
                        "duration_ms must stay an unsigned integer"
                    );
                    *duration = serde_json::json!(0);
                }
                serde_json::json!({ "source": event.source, "payload": payload })
            })
            .collect();
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&golden_keys_sorted(serde_json::Value::Array(emitted)))
                .unwrap()
        );
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/observability")
            .join(fixture);
        let golden = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            golden == actual,
            "{fixture} drifted:\n--- golden\n{golden}--- emitted\n{actual}"
        );
    }
}
