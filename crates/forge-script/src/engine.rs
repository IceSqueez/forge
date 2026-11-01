use crate::error::ScriptError;
use forge_types::{VARIANT_MAX_COLLECTION_LEN, Variant};
use std::time::Duration;

use forge_storage::{SettingsRepo, read_setting_or_default};
use rhai::packages::{BasicArrayPackage, BasicMapPackage, CorePackage, LogicPackage, Package};

use crate::api::ForgeApi;
use crate::convert::{dynamic_to_variant, register_variant_types};
use crate::data_audit::{ensure_result_fits, install_data_audit};
use crate::script_thread::on_script_stack;
use crate::settings;
use crate::wall_budget::{SandboxClock, WallBudget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub op_limit: u64,
    pub wall_time_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            op_limit: u64::from(settings::OP_LIMIT.codec().default),
            wall_time_ms: u64::from(settings::TIMEOUT_MS.codec().default),
        }
    }
}

impl EngineConfig {
    pub async fn read(repo: &dyn SettingsRepo) -> Self {
        Self {
            op_limit: u64::from(read_setting_or_default(repo, &settings::OP_LIMIT).await),
            wall_time_ms: u64::from(read_setting_or_default(repo, &settings::TIMEOUT_MS).await),
        }
    }
}

pub struct Engine {
    config: EngineConfig,
    inner: rhai::Engine,
    budget: WallBudget,
}

pub(crate) fn register_sandbox_base(inner: &mut rhai::Engine, cfg: &EngineConfig) {
    inner.register_global_module(CorePackage::new().as_shared_module());
    inner.register_global_module(LogicPackage::new().as_shared_module());
    inner.register_global_module(BasicArrayPackage::new().as_shared_module());
    inner.register_global_module(BasicMapPackage::new().as_shared_module());
    register_variant_types(inner);

    inner.set_max_operations(cfg.op_limit);
    inner.set_max_call_levels(64);
    inner.set_max_expr_depths(64, 32);
    inner.set_max_string_size(1 << 20);
    inner.set_max_array_size(VARIANT_MAX_COLLECTION_LEN);
    inner.set_max_map_size(VARIANT_MAX_COLLECTION_LEN);
    install_data_audit(inner);

    inner.set_optimization_level(rhai::OptimizationLevel::None);
    inner.disable_symbol("eval");
    inner.disable_symbol("curry");
}

pub(crate) fn budgeted_sandbox(
    cfg: &EngineConfig,
    clock: SandboxClock,
) -> (rhai::Engine, WallBudget) {
    let mut inner = rhai::Engine::new_raw();
    register_sandbox_base(&mut inner, cfg);
    let budget = WallBudget::start(clock, cfg.wall_time_ms);
    budget.enforce_on(&mut inner);
    (inner, budget)
}

impl Engine {
    pub fn with_config(cfg: EngineConfig) -> Self {
        Self::on_clock(cfg, SandboxClock::default())
    }

    pub(crate) fn on_clock(cfg: EngineConfig, clock: SandboxClock) -> Self {
        let (inner, budget) = budgeted_sandbox(&cfg, clock);
        Self {
            config: cfg,
            inner,
            budget,
        }
    }

    pub fn with_api(cfg: EngineConfig, api: ForgeApi) -> Self {
        Self::with_api_on_clock(cfg, api, SandboxClock::default())
    }

    pub(crate) fn with_api_on_clock(cfg: EngineConfig, api: ForgeApi, clock: SandboxClock) -> Self {
        let mut this = Self::on_clock(cfg, clock.clone());
        this.inner
            .register_static_module("forge", api.on_clock(clock).into_module());
        this
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn eval_script(&self, script: &str) -> Result<rhai::Dynamic, ScriptError> {
        self.eval_script_with_scope(script, &mut rhai::Scope::new())
    }

    #[cfg(feature = "test-support")]
    pub fn eval_in_scope(
        &self,
        body: &str,
        scope: &mut rhai::Scope<'_>,
    ) -> Result<rhai::Dynamic, ScriptError> {
        self.eval_script_with_scope(body, scope)
    }

    #[cfg(feature = "test-support")]
    pub fn eval_in_scope_as_variant(
        &self,
        body: &str,
        scope: &mut rhai::Scope<'_>,
    ) -> Result<Option<Variant>, ScriptError> {
        self.eval_script_with_scope_as_variant(body, scope)
    }

    pub(crate) fn eval_script_with_scope(
        &self,
        body: &str,
        scope: &mut rhai::Scope<'_>,
    ) -> Result<rhai::Dynamic, ScriptError> {
        on_script_stack(|| {
            self.budget.restart();
            let outcome = self
                .inner
                .eval_with_scope::<rhai::Dynamic>(scope, body)
                .and_then(|value| ensure_result_fits(&value).map(|()| value))
                .map_err(|e| map_eval_error(&self.config, *e, self.budget.elapsed()));
            scope.clear();
            outcome
        })
    }

    pub(crate) fn eval_script_with_scope_as_variant(
        &self,
        body: &str,
        scope: &mut rhai::Scope<'_>,
    ) -> Result<Option<Variant>, ScriptError> {
        let value = self.eval_script_with_scope(body, scope)?;
        if value.is_unit() {
            return Ok(None);
        }
        dynamic_to_variant(value)
            .map(Some)
            .map_err(|e| ScriptError::Runtime {
                reason: e.to_string(),
            })
    }
}

pub fn validate_syntax(body: &str) -> Result<(), ScriptError> {
    let mut engine = rhai::Engine::new_raw();
    register_sandbox_base(&mut engine, &EngineConfig::default());
    engine
        .compile(body)
        .map(|_| ())
        .map_err(|e| ScriptError::Compile {
            reason: e.to_string(),
        })
}

pub(crate) fn map_eval_error(
    cfg: &EngineConfig,
    err: rhai::EvalAltResult,
    elapsed: Duration,
) -> ScriptError {
    let reason = err.to_string();
    match err {
        rhai::EvalAltResult::ErrorParsing(..) => ScriptError::Compile { reason },
        rhai::EvalAltResult::ErrorTooManyOperations(..) => {
            ScriptError::OperationLimit { ops: cfg.op_limit }
        }
        rhai::EvalAltResult::ErrorTerminated(..) => ScriptError::Timeout {
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            limit_ms: cfg.wall_time_ms,
        },
        _ => ScriptError::Runtime { reason },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ForgeApi;
    use forge_events::test_support::CapturingPublisher;
    use forge_storage::DataProvider;
    use forge_storage::test_support::MemorySettings;
    use forge_storage_sqlite::SqliteBackend;

    use crate::test_support::{
        Sandboxed, ScriptedClock, api_publishing_to, quiet_api, quiet_api_until, repo_globals,
        sandboxed_backend,
    };
    use forge_types::{EventId, Variant};
    use std::sync::Arc;
    use std::time::Instant;

    const WALL_LIMIT_MS: u64 = 1_000;
    const WALL_LIMIT: Duration = Duration::from_millis(WALL_LIMIT_MS);

    fn unhurried() -> EngineConfig {
        EngineConfig {
            wall_time_ms: 30_000,
            ..EngineConfig::default()
        }
    }

    async fn open_test_dp() -> Sandboxed<Arc<SqliteBackend>> {
        sandboxed_backend().await.map(Arc::new)
    }

    fn make_api(dp: Arc<SqliteBackend>) -> ForgeApi {
        quiet_api(repo_globals(dp.globals_repo()))
    }

    fn make_api_with_deadline(dp: Arc<SqliteBackend>, deadline: Instant) -> ForgeApi {
        quiet_api_until(repo_globals(dp.globals_repo()), deadline)
    }

    fn bounded() -> EngineConfig {
        EngineConfig {
            op_limit: 10_000,
            wall_time_ms: WALL_LIMIT_MS,
        }
    }

    #[test]
    fn eval_script_arithmetic() {
        let engine = Engine::with_config(unhurried());
        let result = engine.eval_script("1 + 2").unwrap();
        assert_eq!(result.cast::<i64>(), 3);
    }

    #[test]
    fn eval_script_compile_error() {
        let engine = Engine::with_config(unhurried());
        let err = engine.eval_script("@@@invalid").unwrap_err();
        assert!(matches!(err, ScriptError::Compile { .. }));
    }

    #[test]
    fn failed_eval_message_omits_script_body() {
        const SECRET: &str = "sk-live-0123456789";
        let engine = Engine::with_config(unhurried());
        for body in [
            format!("let token = \"{SECRET}\"; @@@"),
            format!("let token = \"{SECRET}\"; let x = 1 / 0;"),
            format!("let token = \"{SECRET}\"; undefined_fn(token.len())"),
        ] {
            let err = engine.eval_script(&body).unwrap_err();
            assert!(!format!("{err} {err:?}").contains(SECRET), "{err:?}");
        }
    }

    #[test]
    fn validate_syntax_error_omits_script_body() {
        let err = validate_syntax("let token = \"sk-live-0123456789\"; @@@").unwrap_err();
        assert!(!format!("{err} {err:?}").contains("sk-live"), "{err:?}");
    }

    fn nested_parens(depth: usize) -> String {
        format!("{}1{}", "(".repeat(depth), ")".repeat(depth))
    }

    #[test]
    fn validation_refuses_what_the_sandbox_refuses_to_compile() {
        let far_past_the_depth_limit = nested_parens(200);
        for body in [r#"eval("1 + 1")"#, far_past_the_depth_limit.as_str()] {
            assert!(
                matches!(validate_syntax(body), Err(ScriptError::Compile { .. })),
                "validation accepted {body}"
            );
        }
    }

    #[test]
    fn validation_agrees_with_the_sandbox_at_every_nesting_depth() {
        let sandbox = Engine::with_config(EngineConfig::default());
        for depth in 0..=80 {
            let body = nested_parens(depth);
            assert_eq!(
                validate_syntax(&body).is_ok(),
                sandbox.eval_script(&body).is_ok(),
                "{depth} nested parentheses"
            );
        }
    }

    #[test]
    fn timeout_reports_measured_elapsed_not_the_limit() {
        let cfg = EngineConfig {
            wall_time_ms: 100,
            ..EngineConfig::default()
        };
        let err = map_eval_error(
            &cfg,
            rhai::EvalAltResult::ErrorTerminated(rhai::Dynamic::UNIT, rhai::Position::NONE),
            Duration::from_secs(2),
        );
        assert!(
            matches!(
                err,
                ScriptError::Timeout {
                    elapsed_ms: 2_000,
                    limit_ms: 100
                }
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn with_api_forge_sleep_waits_at_least_the_requested_time() {
        const REQUESTED_MS: u64 = 5;
        let dp = open_test_dp().await;
        let clock = ScriptedClock::frozen();
        let before = clock.peek();
        let engine = Engine::with_api_on_clock(
            unhurried(),
            make_api_with_deadline(dp.clone(), before + WALL_LIMIT),
            clock.sandbox(),
        );
        tokio::task::spawn_blocking(move || {
            drop(
                engine
                    .eval_script(&format!("forge::sleep({REQUESTED_MS})"))
                    .unwrap(),
            );
        })
        .await
        .unwrap();
        assert_eq!(clock.peek() - before, Duration::from_millis(REQUESTED_MS));
    }

    #[tokio::test]
    async fn sleep_is_clamped_to_what_is_left_of_the_budget() {
        let dp = open_test_dp().await;
        let clock = ScriptedClock::frozen();
        let deadline = clock.peek() + Duration::from_millis(5);
        let engine = Engine::with_api_on_clock(
            unhurried(),
            make_api_with_deadline(dp.clone(), deadline),
            clock.sandbox(),
        );
        tokio::task::spawn_blocking(move || engine.eval_script("forge::sleep(1000)").map(|_| ()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(clock.peek(), deadline);
    }

    #[tokio::test]
    async fn a_script_that_keeps_sleeping_is_refused_once_the_budget_is_spent() {
        const BUDGET_MS: u64 = 5;
        let dp = open_test_dp().await;
        let clock = ScriptedClock::frozen();
        let deadline = clock.peek() + Duration::from_millis(BUDGET_MS);
        let engine = Engine::with_api_on_clock(
            unhurried(),
            make_api_with_deadline(dp.clone(), deadline),
            clock.sandbox(),
        );
        let result =
            tokio::task::spawn_blocking(move || engine.eval_script("loop { forge::sleep(1) }"))
                .await
                .unwrap();
        assert!(
            matches!(&result, Err(ScriptError::Runtime { reason }) if reason.contains("deadline exceeded")),
            "{result:?}"
        );
        assert_eq!(clock.peek(), deadline);
    }

    #[tokio::test]
    async fn sleep_returns_err_when_deadline_already_passed() {
        let dp = open_test_dp().await;
        let expired_deadline = Instant::now()
            .checked_sub(Duration::from_millis(50))
            .unwrap();
        let api = quiet_api_until(repo_globals(dp.globals_repo()), expired_deadline);
        let engine = Engine::with_api(unhurried(), api);
        let result = tokio::task::spawn_blocking(move || engine.eval_script("forge::sleep(100)"))
            .await
            .unwrap();
        assert!(
            matches!(&result, Err(ScriptError::Runtime { reason }) if reason.contains("deadline exceeded")),
            "sleep with already-expired deadline must return error, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn read_config_falls_back_to_defaults_when_settings_absent() {
        let cfg = EngineConfig::read(&MemorySettings::new()).await;
        assert_eq!(cfg, EngineConfig::default());
    }

    #[tokio::test]
    async fn read_config_reads_persisted_op_limit_and_timeout() {
        let store = MemorySettings::new();
        store.seed_raw(&settings::OP_LIMIT, "7777");
        store.seed_raw(&settings::TIMEOUT_MS, "1234");
        let cfg = EngineConfig::read(&store).await;
        assert_eq!(
            cfg,
            EngineConfig {
                op_limit: 7777,
                wall_time_ms: 1234,
            }
        );
    }

    #[tokio::test]
    async fn read_config_falls_back_to_defaults_for_unparseable_values() {
        let defaults = EngineConfig::default();
        for bad in ["", "abc", "-5", "3.5", "99999999999999999999999999"] {
            let store = MemorySettings::new();
            store.seed_raw(&settings::OP_LIMIT, bad);
            store.seed_raw(&settings::TIMEOUT_MS, bad);
            let cfg = EngineConfig::read(&store).await;
            assert_eq!(
                cfg, defaults,
                "unparseable {bad:?} must fall back to defaults"
            );
        }
    }

    #[tokio::test]
    async fn read_config_clamps_persisted_limits_into_the_sandbox_range() {
        for (stored_ops, stored_ms, op_limit, wall_time_ms) in [
            ("0", "0", 1_000, 50),
            ("999", "49", 1_000, 50),
            ("1000", "50", 1_000, 50),
            ("1001", "51", 1_001, 51),
            ("9999999", "9999", 9_999_999, 9_999),
            ("10000000", "10000", 10_000_000, 10_000),
            ("10000001", "10001", 10_000_000, 10_000),
            ("4294967295", "4294967295", 10_000_000, 10_000),
        ] {
            let store = MemorySettings::new();
            store.seed_raw(&settings::OP_LIMIT, stored_ops);
            store.seed_raw(&settings::TIMEOUT_MS, stored_ms);

            let cfg = EngineConfig::read(&store).await;

            assert_eq!(
                cfg,
                EngineConfig {
                    op_limit,
                    wall_time_ms,
                },
                "stored ops {stored_ops:?}, ms {stored_ms:?}"
            );
        }
    }

    #[tokio::test]
    async fn persisted_op_limit_below_the_minimum_aborts_runaway_script_at_the_minimum() {
        let store = MemorySettings::new();
        store.seed_raw(&settings::OP_LIMIT, "500");
        let cfg = EngineConfig::read(&store).await;
        let engine = Engine::with_config(cfg);
        let err = tokio::task::spawn_blocking(move || engine.eval_script("loop {}"))
            .await
            .unwrap()
            .unwrap_err();
        assert!(
            matches!(err, ScriptError::OperationLimit { ops: 1_000, .. }),
            "runaway script must abort at the clamped op-limit (1000), got: {err:?}",
        );
    }

    #[test]
    fn a_runaway_script_is_stopped_at_the_wall_budget_with_the_time_it_took() {
        let engine = Engine::on_clock(bounded(), ScriptedClock::ticking(WALL_LIMIT).sandbox());
        let err = engine.eval_script("loop {}").unwrap_err();
        assert!(
            matches!(
                err,
                ScriptError::Timeout { elapsed_ms, limit_ms: WALL_LIMIT_MS }
                    if elapsed_ms >= WALL_LIMIT_MS
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_stopped_clock_leaves_only_the_op_limit_to_end_a_runaway_script() {
        let engine = Engine::on_clock(bounded(), ScriptedClock::frozen().sandbox());
        let err = engine.eval_script("loop {}").unwrap_err();
        assert!(matches!(err, ScriptError::OperationLimit { .. }), "{err:?}");
    }

    #[test]
    fn each_evaluation_gets_a_fresh_wall_budget() {
        let clock = ScriptedClock::frozen();
        let engine = Engine::on_clock(bounded(), clock.sandbox());
        for _ in 0..2 {
            assert!(engine.eval_script("1 + 1").is_ok());
            clock.advance(WALL_LIMIT);
        }
    }

    #[tokio::test]
    async fn with_api_globals_get_returns_stored_value() {
        let dp = open_test_dp().await;
        dp.globals_repo()
            .set("counter", Variant::Int(42), false)
            .await
            .unwrap();
        let engine = Engine::with_api(unhurried(), make_api(Arc::clone(&dp)));
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
        let engine = Engine::with_api(unhurried(), make_api(Arc::clone(&dp)));
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
        let engine = Engine::with_api(unhurried(), make_api(dp.clone()));
        tokio::task::spawn_blocking(move || {
            drop(
                engine
                    .eval_script(r#"forge::globals::set("score", 99, true)"#)
                    .unwrap(),
            );
        })
        .await
        .unwrap();
        let stored = dp_check.globals_repo().get("score").await.unwrap();
        assert_eq!(stored, Some(Variant::Int(99)));
    }

    #[tokio::test]
    async fn with_api_chat_send_publishes_event() {
        let dp = open_test_dp().await;
        let captured = CapturingPublisher::shared();
        let publisher = Arc::clone(&captured);
        let caused_by = EventId::new();
        let api = api_publishing_to(publisher, repo_globals(dp.globals_repo()), caused_by);
        let engine = Engine::with_api(unhurried(), api);

        tokio::task::spawn_blocking(move || {
            drop(
                engine
                    .eval_script(r#"forge::chat::send("hello chat")"#)
                    .unwrap(),
            );
        })
        .await
        .unwrap();

        let events = captured.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "chat.send.request");
        assert_eq!(events[0].caused_by, Some(caused_by));
    }

    fn sample_datetime() -> time::OffsetDateTime {
        time::OffsetDateTime::from_unix_timestamp(1_791_118_800).unwrap()
    }

    #[test]
    fn a_script_returning_nothing_converts_to_no_value() {
        let engine = Engine::with_config(unhurried());
        for body in ["()", "let x = 1;", "if false { 1 }"] {
            let result = engine.eval_script_with_scope_as_variant(body, &mut rhai::Scope::new());
            assert!(matches!(result, Ok(None)), "{body}: {result:?}");
        }
    }

    #[test]
    fn a_script_returning_an_unstorable_value_fails_as_a_runtime_error() {
        let engine = Engine::with_config(unhurried());
        for body in ["[1, ()]", "#{a: ()}", "0.0 / 0.0", "Fn(\"x\")"] {
            let result = engine.eval_script_with_scope_as_variant(body, &mut rhai::Scope::new());
            assert!(
                matches!(result, Err(ScriptError::Runtime { .. })),
                "{body}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn globals_set_stores_arrays_maps_and_datetimes_as_their_own_type() {
        let when = sample_datetime();
        for (value_expr, expected) in [
            (
                r#"[1, "a", [true]]"#,
                Variant::Array(vec![
                    Variant::Int(1),
                    Variant::String("a".to_owned()),
                    Variant::Array(vec![Variant::Bool(true)]),
                ]),
            ),
            (
                "#{n: 1, at: when}",
                Variant::Object(std::collections::BTreeMap::from([
                    ("n".to_owned(), Variant::Int(1)),
                    ("at".to_owned(), Variant::Datetime(when)),
                ])),
            ),
            ("when", Variant::Datetime(when)),
        ] {
            let dp = open_test_dp().await;
            let engine = Engine::with_api(unhurried(), make_api(Arc::clone(&dp)));
            let body = format!(r#"forge::globals::set("stored", {value_expr}, false)"#);
            let result = tokio::task::spawn_blocking(move || {
                let mut scope = rhai::Scope::new();
                scope.push("when", when);
                engine.eval_script_with_scope(&body, &mut scope).map(|_| ())
            })
            .await
            .unwrap();

            assert!(result.is_ok(), "{value_expr}: {result:?}");
            let stored = dp.globals_repo().get("stored").await.unwrap();
            assert_eq!(stored, Some(expected), "{value_expr}");
        }
    }

    #[tokio::test]
    async fn a_global_read_and_written_back_by_a_script_keeps_its_stored_type() {
        let when = sample_datetime();
        for original in [
            Variant::Datetime(when),
            Variant::Array(vec![Variant::Datetime(when), Variant::Array(vec![])]),
            Variant::Object(std::collections::BTreeMap::from([(
                "inner".to_owned(),
                Variant::Object(std::collections::BTreeMap::new()),
            )])),
        ] {
            let dp = open_test_dp().await;
            dp.globals_repo()
                .set("source", original.clone(), false)
                .await
                .unwrap();
            let engine = Engine::with_api(unhurried(), make_api(Arc::clone(&dp)));
            let result = tokio::task::spawn_blocking(move || {
                engine
                    .eval_script(
                        r#"forge::globals::set("copy", forge::globals::get("source"), false)"#,
                    )
                    .map(|_| ())
            })
            .await
            .unwrap();

            assert!(result.is_ok(), "{original:?}: {result:?}");
            let copy = dp.globals_repo().get("copy").await.unwrap();
            assert_eq!(copy, Some(original.clone()), "{original:?}");
        }
    }
}
