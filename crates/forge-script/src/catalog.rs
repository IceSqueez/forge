use std::sync::{Arc, LazyLock};
use std::time::Instant;

use forge_events::{Event, EventPublisher};
use forge_types::{EventId, Variant};

use crate::api::ForgeApi;
use crate::globals_port::{GlobalDeletion, GlobalIncrement, ScriptGlobals, ScriptGlobalsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDescriptor {
    pub name: &'static str,
    pub ty: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub namespace: Option<&'static str>,
    pub name: &'static str,
    pub params: Vec<ParamDescriptor>,
    pub return_type: &'static str,
    pub doc: Option<&'static str>,
}

struct DescribeOnly;

impl EventPublisher for DescribeOnly {
    fn publish(&self, _event: Event) {}
}

#[async_trait::async_trait]
impl ScriptGlobals for DescribeOnly {
    async fn get(&self, _key: &str) -> Result<Option<Variant>, ScriptGlobalsError> {
        Err(ScriptGlobalsError::DescribeOnly)
    }

    async fn set(
        &self,
        _key: &str,
        _value: Variant,
        _persisted: bool,
        _caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptGlobalsError> {
        Err(ScriptGlobalsError::DescribeOnly)
    }

    async fn increment(
        &self,
        _key: &str,
        _delta: i64,
        _caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptGlobalsError> {
        Err(ScriptGlobalsError::DescribeOnly)
    }

    async fn delete(
        &self,
        _key: &str,
        _caused_by: EventId,
    ) -> Result<GlobalDeletion, ScriptGlobalsError> {
        Err(ScriptGlobalsError::DescribeOnly)
    }
}

static CATALOG: LazyLock<Vec<MethodDescriptor>> = LazyLock::new(|| {
    ForgeApi::new(
        Arc::new(DescribeOnly),
        Arc::new(DescribeOnly),
        EventId::new(),
        Instant::now(),
    )
    .describe()
});

pub fn catalog() -> &'static [MethodDescriptor] {
    &CATALOG
}

pub fn is_engine_bound_name(name: &str) -> bool {
    catalog()
        .iter()
        .any(|method| method.namespace.unwrap_or(method.name) == name)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::engine::{Engine, EngineConfig};
    use crate::error::ScriptError;

    type Signature = (
        Option<&'static str>,
        &'static str,
        Vec<(&'static str, &'static str)>,
        &'static str,
    );

    const FUNCTION_NOT_FOUND: &str = "Function not found";
    const DATETIME_BINDING: &str = "due";

    fn root(
        name: &'static str,
        params: &[(&'static str, &'static str)],
        returns: &'static str,
    ) -> Signature {
        (None, name, params.to_vec(), returns)
    }

    fn within(
        namespace: &'static str,
        name: &'static str,
        params: &[(&'static str, &'static str)],
        returns: &'static str,
    ) -> Signature {
        (Some(namespace), name, params.to_vec(), returns)
    }

    fn signature(method: &MethodDescriptor) -> Signature {
        (
            method.namespace,
            method.name,
            method.params.iter().map(|p| (p.name, p.ty)).collect(),
            method.return_type,
        )
    }

    #[test]
    fn catalog_lists_every_script_function_with_its_signature_grouped_by_namespace() {
        let s = ("sender", "string");
        let t = ("target", "string");
        let text = ("text", "string");
        let to = ("to", "string");
        let user = ("user", "string");
        let key = ("key", "string");
        let action = ("action", "string");
        let options = ("options", "Map");
        let expected = vec![
            root("log", &[("msg", "string")], "()"),
            root("warn", &[("msg", "string")], "()"),
            root("error", &[("msg", "string")], "()"),
            root("sleep", &[("ms", "int")], "()"),
            within("chat", "send", &[text], "()"),
            within("chat", "send", &[t, text], "()"),
            within("chat", "reply", &[to, text], "()"),
            within("chat", "reply", &[t, to, text], "()"),
            within("chat", "whisper", &[user, text], "()"),
            within("chat", "whisper", &[t, user, text], "()"),
            within("chat", "send_as", &[s, text], "()"),
            within("chat", "send_as", &[s, t, text], "()"),
            within("chat", "reply_as", &[s, to, text], "()"),
            within("chat", "reply_as", &[s, t, to, text], "()"),
            within("chat", "whisper_as", &[s, user, text], "()"),
            within("chat", "whisper_as", &[s, t, user, text], "()"),
            within("globals", "get", &[key], "Variant"),
            within(
                "globals",
                "set",
                &[key, ("value", "Variant"), ("persisted", "bool")],
                "()",
            ),
            within("globals", "incr", &[key, ("amount", "int")], "Variant"),
            within("globals", "del", &[key], "bool"),
            within("tts", "speak", &[text], "()"),
            within("tts", "speak_as", &[("voice_id", "string"), text], "()"),
            within("tts", "skip", &[], "()"),
            within("tts", "clear", &[], "()"),
            within("time", "now", &[], "string"),
            within("time", "unix", &[], "int"),
            within("http", "get", &[("url", "string")], "Map"),
            within(
                "http",
                "post",
                &[("url", "string"), ("body", "string")],
                "Map",
            ),
            within("latest", "get", &[("slot", "string")], "Variant"),
            within(
                "latest",
                "get",
                &[("slot", "string"), ("platform", "string")],
                "Variant",
            ),
            within("schedule", "after", &[action, ("seconds", "int")], "Map"),
            within(
                "schedule",
                "after",
                &[action, ("seconds", "int"), options],
                "Map",
            ),
            within("schedule", "at", &[action, ("when", "string")], "Map"),
            within(
                "schedule",
                "at",
                &[action, ("when", "string"), options],
                "Map",
            ),
            within("schedule", "at", &[action, ("when", "datetime")], "Map"),
            within(
                "schedule",
                "at",
                &[action, ("when", "datetime"), options],
                "Map",
            ),
            within("schedule", "at", &[action, ("unix_seconds", "int")], "Map"),
            within(
                "schedule",
                "at",
                &[action, ("unix_seconds", "int"), options],
                "Map",
            ),
            within("schedule", "cancel", &[key], "bool"),
        ];

        let listed: Vec<Signature> = catalog().iter().map(signature).collect();

        assert_eq!(listed, expected);
    }

    fn argument_for(ty: &str) -> &'static str {
        match ty {
            "string" => r#""x""#,
            "int" => "0",
            "bool" => "false",
            "Map" => "#{}",
            "Variant" => "1",
            "datetime" => DATETIME_BINDING,
            other => panic!("no script literal for parameter type {other}"),
        }
    }

    fn call_for(method: &MethodDescriptor) -> String {
        let path = match method.namespace {
            Some(namespace) => format!("forge::{namespace}::{}", method.name),
            None => format!("forge::{}", method.name),
        };
        let args: Vec<&str> = method.params.iter().map(|p| argument_for(p.ty)).collect();
        format!("{path}({})", args.join(", "))
    }

    fn eval_with_a_datetime_bound(
        engine: &Engine,
        script: &str,
    ) -> Result<rhai::Dynamic, ScriptError> {
        let mut scope = rhai::Scope::new();
        scope.push(
            DATETIME_BINDING,
            time::OffsetDateTime::from_unix_timestamp(1_791_118_800).unwrap(),
        );
        engine.eval_script_with_scope(script, &mut scope)
    }

    fn resolves_no_function(result: &Result<rhai::Dynamic, ScriptError>) -> bool {
        matches!(result, Err(ScriptError::Runtime { reason }) if reason.contains(FUNCTION_NOT_FOUND))
    }

    #[tokio::test]
    async fn every_catalog_entry_is_callable_at_its_arity_on_an_engine_without_dependencies() {
        let unresolved = tokio::task::spawn_blocking(|| {
            let engine = Engine::with_api(
                EngineConfig {
                    wall_time_ms: 30_000,
                    ..EngineConfig::default()
                },
                ForgeApi::new(
                    Arc::new(DescribeOnly),
                    Arc::new(DescribeOnly),
                    EventId::new(),
                    Instant::now() + Duration::from_secs(30),
                ),
            );
            assert!(
                resolves_no_function(&eval_with_a_datetime_bound(&engine, "forge::time::now(1)")),
                "a call at an unregistered arity must be detected as unresolved"
            );
            catalog()
                .iter()
                .map(call_for)
                .filter(|call| resolves_no_function(&eval_with_a_datetime_bound(&engine, call)))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap();

        assert!(unresolved.is_empty(), "unresolved calls: {unresolved:?}");
    }

    #[test]
    fn root_functions_and_namespaces_are_engine_bound_names() {
        for name in [
            "log", "warn", "error", "sleep", "chat", "globals", "tts", "time", "http", "latest",
            "schedule",
        ] {
            assert!(is_engine_bound_name(name), "{name}");
        }
    }

    #[test]
    fn namespaced_function_names_and_other_identifiers_are_not_engine_bound() {
        for name in [
            "send",
            "get",
            "after",
            "speak",
            "now",
            "forge",
            "",
            "Log",
            "chats",
            "chat::send",
        ] {
            assert!(!is_engine_bound_name(name), "{name}");
        }
    }
}
