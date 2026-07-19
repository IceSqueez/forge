use forge_storage::GlobalsRepo;
use forge_types::ArgStack;

const SINGLE_BROADCASTER_NAMESPACE: &str = "local";

/// Per-user variables are keyed by `(broadcaster_id, user_id, name)`, but a sub-action
/// runner has no channel identity in its `RunContext`. Triggers that carry one set a
/// `broadcaster_id` arg (chat triggers do not); when it is absent every user variable
/// shares the single-broadcaster `"local"` namespace.
pub(super) async fn resolve_broadcaster_id(
    arg_stack: &ArgStack,
    globals: &dyn GlobalsRepo,
) -> String {
    let from_scope = arg_stack.interpolate("%broadcaster_id%");
    let resolved = if from_scope == "%broadcaster_id%" {
        match globals.get("broadcaster_id").await {
            Ok(Some(value)) => value.to_string(),
            _ => from_scope,
        }
    } else {
        from_scope
    };
    if resolved.is_empty() || resolved == "%broadcaster_id%" {
        SINGLE_BROADCASTER_NAMESPACE.to_owned()
    } else {
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::Variant;
    use time::OffsetDateTime;

    // Returns `broadcaster` only for a `get("broadcaster_id")` lookup, so a
    // regression that queried the wrong key would surface as the "local"
    // fallback. `forbid_get` turns any lookup into a contract violation, used
    // to prove the arg-stack value short-circuits before globals are consulted.
    struct FakeGlobals {
        broadcaster: Option<Variant>,
        forbid_get: bool,
    }

    #[async_trait]
    impl GlobalsRepo for FakeGlobals {
        async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
            assert!(
                !self.forbid_get,
                "globals must not be consulted when scope carries broadcaster_id"
            );
            if name == "broadcaster_id" {
                Ok(self.broadcaster.clone())
            } else {
                Ok(None)
            }
        }
        async fn set(&self, _name: &str, _value: Variant, _p: bool) -> Result<(), StorageError> {
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
        async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
            Ok(Variant::Int(0))
        }
    }

    #[tokio::test]
    async fn scope_broadcaster_id_wins_without_consulting_globals() {
        let stack = ArgStack::new().set(
            "broadcaster_id".to_owned(),
            Variant::String("chan-42".to_owned()),
        );
        let globals = FakeGlobals {
            broadcaster: Some(Variant::String("other".to_owned())),
            forbid_get: true,
        };
        assert_eq!(resolve_broadcaster_id(&stack, &globals).await, "chan-42");
    }

    #[tokio::test]
    async fn absent_from_scope_falls_back_to_named_global() {
        let globals = FakeGlobals {
            broadcaster: Some(Variant::String("chan-from-global".to_owned())),
            forbid_get: false,
        };
        assert_eq!(
            resolve_broadcaster_id(&ArgStack::new(), &globals).await,
            "chan-from-global"
        );
    }

    #[tokio::test]
    async fn absent_everywhere_uses_local_namespace() {
        let globals = FakeGlobals {
            broadcaster: None,
            forbid_get: false,
        };
        assert_eq!(
            resolve_broadcaster_id(&ArgStack::new(), &globals).await,
            "local"
        );
    }
}
