use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource};
use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::EventId;
use rhai::{EvalAltResult, ImmutableString, Module};
use tokio::runtime::Handle;

use crate::convert::{dynamic_to_variant, variant_to_dynamic};

/// The god-object exposed to rhai scripts as the `forge::*` namespace.
///
/// Holds `Arc` clones of the event publisher and storage needed by script-callable
/// methods. Created once per script execution with the `caused_by` event id so all
/// events emitted during the execution carry a correct causation chain.
pub struct ForgeApi {
    publisher: Arc<dyn EventPublisher>,
    dp: Arc<dyn DataProvider>,
    caused_by: EventId,
}

impl ForgeApi {
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        dp: Arc<dyn DataProvider>,
        caused_by: EventId,
    ) -> Self {
        Self {
            publisher,
            dp,
            caused_by,
        }
    }

    /// Consumes `self` and builds the full `forge::*` module tree ready for
    /// `Engine::register_static_module("forge", module)`.
    pub fn into_module(self) -> Arc<Module> {
        let mut root = Module::new();

        let caused_by = self.caused_by;
        root.set_native_fn(
            "log",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::info!(caused_by = %caused_by, message = %msg);
                Ok(())
            },
        );
        root.set_native_fn(
            "warn",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::warn!(caused_by = %caused_by, message = %msg);
                Ok(())
            },
        );
        root.set_native_fn(
            "error",
            move |msg: ImmutableString| -> Result<(), Box<EvalAltResult>> {
                tracing::error!(caused_by = %caused_by, message = %msg);
                Ok(())
            },
        );

        root.set_native_fn("sleep", |ms: i64| -> Result<(), Box<EvalAltResult>> {
            let clamped = (ms.max(0) as u64).min(5_000);
            Handle::current().block_on(tokio::time::sleep(Duration::from_millis(clamped)));
            Ok(())
        });

        let chat = build_chat_module(self.publisher, self.caused_by);
        root.set_sub_module("chat", chat);

        let globals = build_globals_module(self.dp);
        root.set_sub_module("globals", globals);

        root.set_sub_module("audio", Module::new());
        root.set_sub_module("tts", Module::new());
        root.set_sub_module("obs", Module::new());
        root.set_sub_module("http", Module::new());

        Arc::new(root)
    }
}

fn build_chat_module(publisher: Arc<dyn EventPublisher>, caused_by: EventId) -> Module {
    let mut m = Module::new();

    let pub_send = Arc::clone(&publisher);
    m.set_native_fn(
        "send",
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            pub_send.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_reply = Arc::clone(&publisher);
    m.set_native_fn(
        "reply",
        move |to: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            pub_reply.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str(), "reply_to": to.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    let pub_whisper = publisher;
    m.set_native_fn(
        "whisper",
        move |user: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            pub_whisper.publish(Event::caused_by(
                EventSource::Rhai,
                "chat.send.request",
                serde_json::json!({"message": text.as_str(), "whisper_to": user.as_str()}),
                caused_by,
            ));
            Ok(())
        },
    );

    m
}

fn build_globals_module(dp: Arc<dyn DataProvider>) -> Module {
    let mut m = Module::new();

    let dp_get = Arc::clone(&dp);
    m.set_native_fn(
        "get",
        move |key: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            match Handle::current().block_on(GlobalsRepo::get(dp_get.as_ref(), key.as_str())) {
                Ok(Some(v)) => Ok(variant_to_dynamic(v)),
                Ok(None) => Ok(rhai::Dynamic::UNIT),
                Err(e) => Err(e.to_string().into()),
            }
        },
    );

    let dp_set = Arc::clone(&dp);
    m.set_native_fn(
        "set",
        move |key: ImmutableString,
              val: rhai::Dynamic,
              persisted: bool|
              -> Result<(), Box<EvalAltResult>> {
            let variant =
                dynamic_to_variant(val).map_err(|e| -> Box<EvalAltResult> { e.into() })?;
            Handle::current()
                .block_on(GlobalsRepo::set(
                    dp_set.as_ref(),
                    key.as_str(),
                    variant,
                    persisted,
                ))
                .map_err(|e| e.to_string().into())
        },
    );

    let dp_incr = Arc::clone(&dp);
    m.set_native_fn(
        "incr",
        move |key: ImmutableString, amount: i64| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            Handle::current()
                .block_on(GlobalsRepo::incr(dp_incr.as_ref(), key.as_str(), amount))
                .map(variant_to_dynamic)
                .map_err(|e| e.to_string().into())
        },
    );

    let dp_del = dp;
    m.set_native_fn(
        "del",
        move |key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Handle::current()
                .block_on(GlobalsRepo::delete(dp_del.as_ref(), key.as_str()))
                .map_err(|e| e.to_string().into())
        },
    );

    m
}
