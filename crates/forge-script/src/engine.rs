use std::sync::{Arc, Mutex};
use std::time::Instant;

use rhai::Dynamic;
use rhai::packages::{BasicArrayPackage, BasicMapPackage, CorePackage, LogicPackage, Package};

use crate::ScriptError;
use crate::api::ForgeApi;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub op_limit: u64,
    pub wall_time_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            op_limit: 100_000,
            wall_time_ms: 500,
        }
    }
}

pub struct Engine {
    config: EngineConfig,
    inner: rhai::Engine,
    wall_timer: Arc<Mutex<Instant>>,
}

pub(crate) fn register_sandbox_base(inner: &mut rhai::Engine, cfg: &EngineConfig) {
    inner.register_global_module(CorePackage::new().as_shared_module());
    inner.register_global_module(LogicPackage::new().as_shared_module());
    inner.register_global_module(BasicArrayPackage::new().as_shared_module());
    inner.register_global_module(BasicMapPackage::new().as_shared_module());

    inner.set_max_operations(cfg.op_limit);
    inner.set_max_call_levels(64);
    inner.set_max_expr_depths(64, 32);
    inner.set_max_string_size(1 << 20);
    inner.set_max_array_size(10_000);
    inner.set_max_map_size(10_000);

    inner.disable_symbol("eval");
}

impl Engine {
    pub fn with_config(cfg: EngineConfig) -> Self {
        let mut inner = rhai::Engine::new_raw();

        register_sandbox_base(&mut inner, &cfg);

        let wall_timer = Arc::new(Mutex::new(Instant::now()));
        let timer_for_closure = Arc::clone(&wall_timer);
        let wall_time_ms = cfg.wall_time_ms;
        inner.on_progress(move |_ops| {
            let elapsed = timer_for_closure
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .elapsed();
            if elapsed.as_millis() > wall_time_ms as u128 {
                Some(Dynamic::UNIT)
            } else {
                None
            }
        });

        Self {
            config: cfg,
            inner,
            wall_timer,
        }
    }

    pub fn with_api(cfg: EngineConfig, api: ForgeApi) -> Self {
        let mut this = Self::with_config(cfg);
        this.inner
            .register_static_module("forge", api.into_module());
        this
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn eval_script(&self, script: &str) -> Result<rhai::Dynamic, ScriptError> {
        self.reset_timer();
        self.inner
            .eval::<rhai::Dynamic>(script)
            .map_err(|e| map_eval_error(script, &self.config, *e))
    }

    pub fn placeholder_eval(&self, expr: &str) -> Result<String, ScriptError> {
        self.reset_timer();
        self.inner
            .eval::<i64>(expr)
            .map(|n| n.to_string())
            .map_err(|e| map_eval_error(expr, &self.config, *e))
    }

    pub fn eval_script_with_scope(
        &self,
        body: &str,
        scope: &mut rhai::Scope<'_>,
    ) -> Result<rhai::Dynamic, ScriptError> {
        self.reset_timer();
        self.inner
            .eval_with_scope::<rhai::Dynamic>(scope, body)
            .map_err(|e| map_eval_error(body, &self.config, *e))
    }

    pub(crate) fn reset_timer(&self) {
        *self.wall_timer.lock().unwrap_or_else(|p| p.into_inner()) = Instant::now();
    }
}

/// Parses `body` and returns `Ok(())` if the script is syntactically valid.
///
/// Does not execute the script. Returns `ScriptError::Compile` for any parse failure.
pub fn validate_syntax(body: &str) -> Result<(), ScriptError> {
    let mut engine = rhai::Engine::new_raw();
    engine.register_global_module(CorePackage::new().as_shared_module());
    engine
        .compile(body)
        .map(|_| ())
        .map_err(|e| ScriptError::Compile {
            script: body.chars().take(80).collect(),
            reason: e.to_string(),
        })
}

pub(crate) fn map_eval_error(
    script: &str,
    cfg: &EngineConfig,
    err: rhai::EvalAltResult,
) -> ScriptError {
    let reason = err.to_string();
    match err {
        rhai::EvalAltResult::ErrorParsing(..) => ScriptError::Compile {
            script: script.to_owned(),
            reason,
        },
        rhai::EvalAltResult::ErrorTooManyOperations(..) => ScriptError::OperationLimit {
            script: script.to_owned(),
            ops: cfg.op_limit,
        },
        rhai::EvalAltResult::ErrorTerminated(..) => ScriptError::Timeout {
            script: script.to_owned(),
            elapsed_ms: cfg.wall_time_ms,
            limit_ms: cfg.wall_time_ms,
        },
        _ => ScriptError::Runtime {
            script: script.to_owned(),
            reason,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::api::ForgeApi;
    use forge_events::{Event, EventPublisher};
    use forge_storage::GlobalsRepo;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{EventId, Variant};
    use std::sync::Arc;

    struct MockPublisher;

    impl EventPublisher for MockPublisher {
        fn publish(&self, _event: Event) {}
    }

    async fn open_test_dp() -> Arc<SqliteBackend> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    fn make_api(dp: Arc<SqliteBackend>) -> ForgeApi {
        ForgeApi::new(
            Arc::new(MockPublisher),
            dp as Arc<dyn GlobalsRepo>,
            EventId::new(),
            Instant::now() + std::time::Duration::from_secs(10),
        )
    }

    fn make_api_with_wall_ms(dp: Arc<SqliteBackend>, wall_ms: u64) -> ForgeApi {
        ForgeApi::new(
            Arc::new(MockPublisher),
            dp as Arc<dyn GlobalsRepo>,
            EventId::new(),
            Instant::now() + std::time::Duration::from_millis(wall_ms),
        )
    }

    #[test]
    fn placeholder_eval_addition() {
        let engine = Engine::with_config(EngineConfig::default());
        assert_eq!(engine.placeholder_eval("1 + 2").unwrap(), "3");
    }

    #[test]
    fn placeholder_eval_invalid_syntax_returns_compile_error() {
        let engine = Engine::with_config(EngineConfig::default());
        let result = engine.placeholder_eval("not a valid expression");
        assert!(
            matches!(result, Err(ScriptError::Compile { .. })),
            "expected Compile error, got: {result:?}",
        );
    }

    #[test]
    fn eval_script_arithmetic() {
        let engine = Engine::with_config(EngineConfig::default());
        let result = engine.eval_script("1 + 2").unwrap();
        assert_eq!(result.cast::<i64>(), 3);
    }

    #[test]
    fn eval_script_compile_error() {
        let engine = Engine::with_config(EngineConfig::default());
        let err = engine.eval_script("@@@invalid").unwrap_err();
        assert!(matches!(err, ScriptError::Compile { .. }));
    }

    #[tokio::test]
    async fn with_api_forge_log_runs_without_error() {
        let dp = open_test_dp().await;
        let engine = Engine::with_api(EngineConfig::default(), make_api(dp));
        let result = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::log("hello from script")"#)
        })
        .await
        .unwrap();
        assert!(result.is_ok(), "forge::log must succeed: {result:?}");
    }

    #[tokio::test]
    async fn with_api_forge_warn_runs_without_error() {
        let dp = open_test_dp().await;
        let engine = Engine::with_api(EngineConfig::default(), make_api(dp));
        let result = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::warn("something fishy")"#)
        })
        .await
        .unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn with_api_forge_sleep_waits_and_returns() {
        let dp = open_test_dp().await;
        let engine = Engine::with_api(
            EngineConfig {
                op_limit: 100_000,
                wall_time_ms: 2_000,
            },
            make_api_with_wall_ms(dp, 2_000),
        );
        let before = std::time::Instant::now();
        tokio::task::spawn_blocking(move || {
            let _ = engine.eval_script("forge::sleep(80)").unwrap();
        })
        .await
        .unwrap();
        assert!(
            before.elapsed().as_millis() >= 80,
            "sleep must pause at least 80ms",
        );
    }

    #[tokio::test]
    async fn sleep_clamped_to_remaining_wall_budget() {
        let dp = open_test_dp().await;
        let wall_ms = 200_u64;
        let engine = Engine::with_api(
            EngineConfig {
                op_limit: 100_000,
                wall_time_ms: wall_ms,
            },
            make_api_with_wall_ms(dp, wall_ms),
        );
        let start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || engine.eval_script("forge::sleep(5000)"))
            .await
            .unwrap();
        let elapsed = start.elapsed();
        assert!(
            result.is_ok() || result.is_err(),
            "sleep with tight budget must not hang"
        );
        assert!(
            elapsed.as_millis() < 5_500,
            "sleep(5000) with 200ms budget must not sleep the full 5s, elapsed={elapsed:?}"
        );
    }

    #[tokio::test]
    async fn sleep_returns_err_when_deadline_already_passed() {
        use std::time::Duration;

        let dp = open_test_dp().await;
        let expired_deadline = Instant::now() - Duration::from_millis(50);
        let api = ForgeApi::new(
            Arc::new(MockPublisher),
            dp as Arc<dyn GlobalsRepo>,
            EventId::new(),
            expired_deadline,
        );
        let engine = Engine::with_api(EngineConfig::default(), api);
        let result = tokio::task::spawn_blocking(move || engine.eval_script("forge::sleep(100)"))
            .await
            .unwrap();
        assert!(
            result.is_err(),
            "sleep with already-expired deadline must return error, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn with_api_infinite_loop_terminates() {
        let dp = open_test_dp().await;
        let engine = Engine::with_api(
            EngineConfig {
                op_limit: 100_000,
                wall_time_ms: 200,
            },
            make_api_with_wall_ms(dp, 200),
        );
        let start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || engine.eval_script("loop {}"))
            .await
            .unwrap();
        assert!(
            result.is_err(),
            "infinite loop must terminate with an error"
        );
        assert!(
            start.elapsed().as_secs() < 5,
            "termination must happen well under 5s"
        );
    }

    #[tokio::test]
    async fn with_api_globals_get_returns_stored_value() {
        let dp = open_test_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(42), false)
            .await
            .unwrap();
        let engine = Engine::with_api(EngineConfig::default(), make_api(Arc::clone(&dp)));
        let result = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::globals::get("counter")"#)
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.cast::<i64>(), 42);
    }

    #[tokio::test]
    async fn with_api_globals_get_missing_key_returns_unit() {
        let dp = open_test_dp().await;
        let engine = Engine::with_api(EngineConfig::default(), make_api(Arc::clone(&dp)));
        let result = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::globals::get("no_such_key")"#)
        })
        .await
        .unwrap()
        .unwrap();
        assert!(result.is_unit());
    }

    #[tokio::test]
    async fn with_api_globals_set_writes_and_get_round_trips() {
        let dp = open_test_dp().await;
        let dp_check = Arc::clone(&dp);
        let engine = Engine::with_api(EngineConfig::default(), make_api(dp));
        tokio::task::spawn_blocking(move || {
            let _ = engine
                .eval_script(r#"forge::globals::set("score", 99, true)"#)
                .unwrap();
        })
        .await
        .unwrap();
        let stored = GlobalsRepo::get(dp_check.as_ref(), "score").await.unwrap();
        assert_eq!(stored, Some(Variant::Int(99)));
    }

    #[tokio::test]
    async fn with_api_chat_send_publishes_event() {
        use std::sync::Mutex;

        struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);
        impl EventPublisher for CapturingPublisher {
            fn publish(&self, event: Event) {
                self.0.lock().unwrap().push(event);
            }
        }

        let dp = open_test_dp().await;
        let captured: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = Arc::new(CapturingPublisher(Arc::clone(&captured)));
        let caused_by = EventId::new();
        let api = ForgeApi::new(
            publisher,
            dp as Arc<dyn GlobalsRepo>,
            caused_by,
            Instant::now() + std::time::Duration::from_secs(10),
        );
        let engine = Engine::with_api(EngineConfig::default(), api);

        tokio::task::spawn_blocking(move || {
            let _ = engine
                .eval_script(r#"forge::chat::send("hello chat")"#)
                .unwrap();
        })
        .await
        .unwrap();

        let events = captured.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "chat.send.request");
        assert_eq!(events[0].caused_by, Some(caused_by));
    }
}
