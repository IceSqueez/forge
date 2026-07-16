//! Integration tests for the per-user variable sub-action runners
//! (`core.users.get_var` / `core.users.set_var` / `core.users.increment_var`).
//!
//! Storage is an in-memory `UserGlobalsRepo` mock - no SQLite, no services, no network.
//! The load-bearing edge under test is broadcaster-id resolution: a sub-action runner has
//! no channel identity, so absent a `%broadcaster_id%` arg every variable shares the
//! single-broadcaster `"local"` namespace.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreUsersGetVarRunner, CoreUsersIncrementVarRunner, CoreUsersSetVarRunner,
};
use forge_storage::{GlobalEntry, GlobalsRepo, StorageError, UserGlobalEntry, UserGlobalsRepo};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};
use time::OffsetDateTime;

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

/// Empty `GlobalsRepo`: the per-user runners only consult it to resolve `%global%`
/// tokens during interpolation, which these tests never exercise, so every read is `None`.
struct EmptyGlobals;

#[async_trait]
impl GlobalsRepo for EmptyGlobals {
    async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }
    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }
    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(vec![])
    }
    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }
    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }
    async fn incr(&self, name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotFound {
            key: name.to_owned(),
        })
    }
}

type Key = (String, String, String);

/// In-memory `UserGlobalsRepo`. Records every `get` key so a test can assert the exact
/// `(broadcaster_id, user_id, name)` tuple the runner resolved.
#[derive(Default)]
struct MapUserGlobals {
    store: Mutex<HashMap<Key, Variant>>,
    get_keys: Mutex<Vec<Key>>,
    fail: bool,
}

impl MapUserGlobals {
    fn new() -> Self {
        Self::default()
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    fn seeded(broadcaster: &str, user: &str, name: &str, value: Variant) -> Self {
        let me = Self::default();
        me.store.lock().unwrap().insert(
            (broadcaster.to_owned(), user.to_owned(), name.to_owned()),
            value,
        );
        me
    }

    fn stored(&self, broadcaster: &str, user: &str, name: &str) -> Option<Variant> {
        self.store
            .lock()
            .unwrap()
            .get(&(broadcaster.to_owned(), user.to_owned(), name.to_owned()))
            .cloned()
    }

    fn last_get(&self) -> Key {
        self.get_keys
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a get must have been recorded")
    }
}

#[async_trait]
impl UserGlobalsRepo for MapUserGlobals {
    async fn get(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        self.get_keys.lock().unwrap().push((
            broadcaster_id.to_owned(),
            user_id.to_owned(),
            name.to_owned(),
        ));
        if self.fail {
            return Err(StorageError::NotFound {
                key: name.to_owned(),
            });
        }
        Ok(self
            .store
            .lock()
            .unwrap()
            .get(&(
                broadcaster_id.to_owned(),
                user_id.to_owned(),
                name.to_owned(),
            ))
            .cloned())
    }

    async fn set(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        value: Variant,
    ) -> Result<(), StorageError> {
        if self.fail {
            return Err(StorageError::NotFound {
                key: name.to_owned(),
            });
        }
        self.store.lock().unwrap().insert(
            (
                broadcaster_id.to_owned(),
                user_id.to_owned(),
                name.to_owned(),
            ),
            value,
        );
        Ok(())
    }

    async fn delete(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<bool, StorageError> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .remove(&(
                broadcaster_id.to_owned(),
                user_id.to_owned(),
                name.to_owned(),
            ))
            .is_some())
    }

    async fn list_for_user(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn list_for_broadcaster(
        &self,
        _broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }
}

fn cfg(pairs: &[(&str, Variant)]) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    for (key, value) in pairs {
        c.insert((*key).to_owned(), value.clone());
    }
    c
}

async fn run<R: SubActionRunner>(
    runner: &R,
    config: &SubActionConfig,
    stack: &ArgStack,
) -> (SubActionOutcome, Option<ArgStack>) {
    let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, new_stack) = runner.execute(config, &ctx).await;
    (telemetry.outcome, new_stack)
}

fn get_config() -> SubActionConfig {
    cfg(&[
        ("user_login", Variant::String("viewer".to_owned())),
        ("var_name", Variant::String("points".to_owned())),
        ("into_var", Variant::String("result".to_owned())),
        ("default_value", Variant::String("0".to_owned())),
    ])
}

// ---- get_var --------------------------------------------------------------

#[tokio::test]
async fn get_var_binds_existing_value_into_output_argument() {
    let repo = Arc::new(MapUserGlobals::seeded(
        "local",
        "viewer",
        "points",
        Variant::Int(5),
    ));
    let runner = CoreUsersGetVarRunner::new(Arc::new(EmptyGlobals), repo);
    let (outcome, stack) = run(&runner, &get_config(), &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    let stack = stack.expect("get_var returns an updated stack");
    assert!(matches!(stack.get("result"), Some(Variant::Int(5))));
}

#[tokio::test]
async fn get_var_uses_default_when_variable_is_missing() {
    let repo = Arc::new(MapUserGlobals::new());
    let runner = CoreUsersGetVarRunner::new(Arc::new(EmptyGlobals), repo);
    let (outcome, stack) = run(&runner, &get_config(), &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    // Missing var -> default "0" parsed via parse_variant into Int(0).
    assert!(matches!(
        stack.unwrap().get("result"),
        Some(Variant::Int(0))
    ));
}

#[tokio::test]
async fn get_var_queries_local_namespace_with_login_as_user_id() {
    // No %broadcaster_id% arg -> "local" namespace; the login string IS the user_id key.
    let repo = Arc::new(MapUserGlobals::new());
    let runner = CoreUsersGetVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
    let config = cfg(&[
        ("user_login", Variant::String("viewer1".to_owned())),
        ("var_name", Variant::String("points".to_owned())),
        ("into_var", Variant::String("result".to_owned())),
        ("default_value", Variant::String("0".to_owned())),
    ]);
    run(&runner, &config, &ArgStack::new()).await;
    assert_eq!(
        repo.last_get(),
        (
            "local".to_owned(),
            "viewer1".to_owned(),
            "points".to_owned()
        )
    );
}

#[tokio::test]
async fn get_var_uses_broadcaster_arg_as_namespace_when_present() {
    let repo = Arc::new(MapUserGlobals::new());
    let runner = CoreUsersGetVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
    let stack = ArgStack::new().set(
        "broadcaster_id".to_owned(),
        Variant::String("chan_99".to_owned()),
    );
    run(&runner, &get_config(), &stack).await;
    assert_eq!(repo.last_get().0, "chan_99");
}

#[tokio::test]
async fn get_var_reports_failed_on_repo_error() {
    let repo = Arc::new(MapUserGlobals::failing());
    let runner = CoreUsersGetVarRunner::new(Arc::new(EmptyGlobals), repo);
    let (outcome, stack) = run(&runner, &get_config(), &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    assert!(stack.is_none());
}

// ---- set_var --------------------------------------------------------------

#[tokio::test]
async fn set_var_writes_value_under_broadcaster_user_and_name() {
    let repo = Arc::new(MapUserGlobals::new());
    let runner = CoreUsersSetVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
    let config = cfg(&[
        ("user_login", Variant::String("viewer".to_owned())),
        ("var_name", Variant::String("points".to_owned())),
        ("value", Variant::String("42".to_owned())),
    ]);
    let (outcome, _) = run(&runner, &config, &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(matches!(
        repo.stored("local", "viewer", "points"),
        Some(Variant::Int(42))
    ));
}

#[tokio::test]
async fn set_var_then_get_var_round_trips_the_value() {
    let repo = Arc::new(MapUserGlobals::new());
    let globals: Arc<dyn GlobalsRepo> = Arc::new(EmptyGlobals);
    let setter = CoreUsersSetVarRunner::new(globals.clone(), repo.clone());
    let getter = CoreUsersGetVarRunner::new(globals, repo.clone());
    let set_cfg = cfg(&[
        ("user_login", Variant::String("viewer".to_owned())),
        ("var_name", Variant::String("points".to_owned())),
        ("value", Variant::String("7".to_owned())),
    ]);
    run(&setter, &set_cfg, &ArgStack::new()).await;
    let (outcome, stack) = run(&getter, &get_config(), &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(matches!(
        stack.unwrap().get("result"),
        Some(Variant::Int(7))
    ));
}

// ---- increment_var --------------------------------------------------------

#[tokio::test]
async fn increment_starts_missing_variable_at_amount() {
    let repo = Arc::new(MapUserGlobals::new());
    let runner = CoreUsersIncrementVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
    let config = cfg(&[
        ("user_login", Variant::String("viewer".to_owned())),
        ("var_name", Variant::String("points".to_owned())),
        ("amount", Variant::Int(5)),
    ]);
    let (outcome, _) = run(&runner, &config, &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(matches!(
        repo.stored("local", "viewer", "points"),
        Some(Variant::Int(5))
    ));
}

#[tokio::test]
async fn increment_saturating_adds_to_existing_int() {
    for (start, amount, expected) in [
        (10_i64, 5_i64, 15_i64),
        (i64::MAX, 1, i64::MAX),
        (i64::MIN, -1, i64::MIN),
    ] {
        let repo = Arc::new(MapUserGlobals::seeded(
            "local",
            "viewer",
            "points",
            Variant::Int(start),
        ));
        let runner = CoreUsersIncrementVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
        let config = cfg(&[
            ("user_login", Variant::String("viewer".to_owned())),
            ("var_name", Variant::String("points".to_owned())),
            ("amount", Variant::Int(amount)),
        ]);
        let (outcome, _) = run(&runner, &config, &ArgStack::new()).await;
        assert!(
            matches!(outcome, SubActionOutcome::Success),
            "{start}+{amount}"
        );
        assert!(
            matches!(repo.stored("local", "viewer", "points"), Some(Variant::Int(v)) if v == expected),
            "{start}+{amount} should saturate to {expected}"
        );
    }
}

#[tokio::test]
async fn increment_adds_to_existing_float() {
    let repo = Arc::new(MapUserGlobals::seeded(
        "local",
        "viewer",
        "ratio",
        Variant::float(1.5).unwrap(),
    ));
    let runner = CoreUsersIncrementVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
    let config = cfg(&[
        ("user_login", Variant::String("viewer".to_owned())),
        ("var_name", Variant::String("ratio".to_owned())),
        ("amount", Variant::Int(2)),
    ]);
    let (outcome, _) = run(&runner, &config, &ArgStack::new()).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(matches!(
        repo.stored("local", "viewer", "ratio"),
        Some(Variant::Float(f)) if (f - 3.5).abs() < 1e-12
    ));
}

#[tokio::test]
async fn increment_non_numeric_variable_fails_and_leaves_value_unchanged() {
    let repo = Arc::new(MapUserGlobals::seeded(
        "local",
        "viewer",
        "name",
        Variant::String("alice".to_owned()),
    ));
    let runner = CoreUsersIncrementVarRunner::new(Arc::new(EmptyGlobals), repo.clone());
    let config = cfg(&[
        ("user_login", Variant::String("viewer".to_owned())),
        ("var_name", Variant::String("name".to_owned())),
        ("amount", Variant::Int(1)),
    ]);
    let (outcome, _) = run(&runner, &config, &ArgStack::new()).await;
    match outcome {
        SubActionOutcome::Failed(msg) => {
            assert!(
                msg.contains("numeric"),
                "message should name the cause: {msg}"
            )
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(matches!(
        repo.stored("local", "viewer", "name"),
        Some(Variant::String(s)) if s == "alice"
    ));
}

// ---- args vs users independence -------------------------------------------

#[tokio::test]
async fn user_variable_read_is_independent_of_same_named_arg_stack_entry() {
    // A transient ArgStack arg named "points" must not satisfy a per-user repo read:
    // get_var resolves the variable's value from the repo only. The output write also
    // must not clobber the unrelated arg.
    let repo = Arc::new(MapUserGlobals::seeded(
        "local",
        "viewer",
        "points",
        Variant::Int(5),
    ));
    let runner = CoreUsersGetVarRunner::new(Arc::new(EmptyGlobals), repo);
    let stack = ArgStack::new().set("points".to_owned(), Variant::Int(999));
    let (_, out) = run(&runner, &get_config(), &stack).await;
    let out = out.unwrap();
    assert!(
        matches!(out.get("result"), Some(Variant::Int(5))),
        "value comes from the repo, not the same-named arg"
    );
    assert!(
        matches!(out.get("points"), Some(Variant::Int(999))),
        "the transient arg is left untouched"
    );
}
