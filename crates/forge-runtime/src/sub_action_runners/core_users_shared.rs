use forge_registry::{TemplateField, TextEditor, TextField};
use forge_storage::{GlobalsRepo, StorageError};
use forge_types::{ArgStack, MessageRef, variable_references};

use crate::core_messages;

const SINGLE_BROADCASTER_NAMESPACE: &str = "local";
const USER_LOGIN_TEMPLATE: &str = "%user_login%";
const BROADCASTER_ID_KEY: &str = "broadcaster_id";

pub(super) const USER_LOGIN: TemplateField = TemplateField {
    key: "user_login",
    label: "User Login",
    placeholder: USER_LOGIN_TEMPLATE,
    default: USER_LOGIN_TEMPLATE,
    required: true,
};

pub(super) const VAR_NAME: TextField = TextField {
    key: "var_name",
    label: "Variable Name",
    editor: TextEditor::Line {
        placeholder: "points",
    },
    default: "",
    required: true,
};

pub(super) async fn resolve_broadcaster_id(
    arg_stack: &ArgStack,
    globals: &dyn GlobalsRepo,
) -> Result<String, StorageError> {
    let resolved = match arg_stack.get(BROADCASTER_ID_KEY) {
        Some(value) => Some(value.to_string()),
        None => globals
            .peek(BROADCASTER_ID_KEY)
            .await?
            .map(|value| value.to_string()),
    };
    Ok(resolved
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| SINGLE_BROADCASTER_NAMESPACE.to_owned()))
}

pub(super) fn resolve_user_login(
    template: &str,
    arg_stack: &ArgStack,
) -> Result<String, MessageRef> {
    if let Some(missing) =
        variable_references(template).find(|name| arg_stack.resolve(name).is_none())
    {
        return Err(core_messages::user_login_unset(missing));
    }
    let interpolated = arg_stack.interpolate(template);
    let trimmed = interpolated.trim();
    let login = trimmed
        .strip_prefix('@')
        .unwrap_or(trimmed)
        .trim_start()
        .to_lowercase();
    if login.is_empty() {
        Err(core_messages::user_login_empty())
    } else {
        Ok(login)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::Variant;
    use time::OffsetDateTime;

    struct FakeGlobals {
        broadcaster: Option<Variant>,
        forbid_get: bool,
    }

    struct UnreadableGlobals;

    #[async_trait]
    impl GlobalsRepo for UnreadableGlobals {
        async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
            Err(StorageError::Connection {
                reason: "disk gone".to_owned(),
            })
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
        assert_eq!(
            resolve_broadcaster_id(&stack, &globals).await.unwrap(),
            "chan-42"
        );
    }

    #[tokio::test]
    async fn absent_from_scope_falls_back_to_named_global() {
        let globals = FakeGlobals {
            broadcaster: Some(Variant::String("chan-from-global".to_owned())),
            forbid_get: false,
        };
        assert_eq!(
            resolve_broadcaster_id(&ArgStack::new(), &globals)
                .await
                .unwrap(),
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
            resolve_broadcaster_id(&ArgStack::new(), &globals)
                .await
                .unwrap(),
            "local"
        );
    }

    #[tokio::test]
    async fn unreadable_globals_are_an_error_not_the_local_namespace() {
        let resolved = resolve_broadcaster_id(&ArgStack::new(), &UnreadableGlobals).await;

        assert!(
            matches!(resolved, Err(StorageError::Connection { .. })),
            "{resolved:?}"
        );
    }

    #[tokio::test]
    async fn an_empty_broadcaster_in_scope_uses_the_local_namespace() {
        let stack =
            ArgStack::new().set("broadcaster_id".to_owned(), Variant::String(String::new()));
        let globals = FakeGlobals {
            broadcaster: Some(Variant::String("unused".to_owned())),
            forbid_get: true,
        };

        assert_eq!(
            resolve_broadcaster_id(&stack, &globals).await.unwrap(),
            "local"
        );
    }

    fn login_stack(login: &str) -> ArgStack {
        ArgStack::new().set("user_login".to_owned(), Variant::String(login.to_owned()))
    }

    #[test]
    fn user_login_is_trimmed_stripped_of_at_and_lowercased() {
        for (template, stack, expected) in [
            ("%user_login%", login_stack("@Name "), "name"),
            ("%user_login%", login_stack("  @ MiXeD"), "mixed"),
            ("%user_login%", login_stack("viewer"), "viewer"),
            ("  @Literal  ", ArgStack::new(), "literal"),
        ] {
            assert_eq!(
                resolve_user_login(template, &stack).as_deref(),
                Ok(expected),
                "{template:?}"
            );
        }
    }

    #[test]
    fn user_login_fails_naming_the_variable_when_it_is_not_in_scope() {
        let reason = resolve_user_login("%user_login%", &ArgStack::new()).unwrap_err();
        assert_eq!(reason.code(), &crate::core_messages::USER_LOGIN_UNSET);
        assert_eq!(
            reason.arg("variable"),
            Some(&forge_types::MessageArg::Verbatim(
                forge_types::VerbatimText::new("user_login")
            ))
        );
    }

    #[test]
    fn user_login_that_resolves_to_nothing_fails() {
        for (template, stack) in [
            ("", ArgStack::new()),
            ("   ", ArgStack::new()),
            ("@", ArgStack::new()),
            ("%user_login%", login_stack("")),
            ("%user_login%", login_stack("  @  ")),
        ] {
            let reason = resolve_user_login(template, &stack).unwrap_err();
            assert_eq!(
                reason.code(),
                &crate::core_messages::USER_LOGIN_EMPTY,
                "{template:?}"
            );
        }
    }
}
