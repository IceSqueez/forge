use super::*;

pub(super) fn build_globals_module(
    caused_by: EventId,
    globals: Arc<dyn ScriptGlobals>,
) -> ApiModule {
    let mut m = ApiModule::default();

    let globals_get = Arc::clone(&globals);
    m.register(
        ScriptFn {
            name: "get",
            params: ["key"],
            doc: Some("Returns () when the key is absent."),
        },
        move |key: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            match Handle::current().block_on(globals_get.get(key.as_str())) {
                Ok(Some(v)) => Ok(variant_to_dynamic(v)),
                Ok(None) => {
                    tracing::warn!(
                        global_name = key.as_str(),
                        "script read an unknown global; it may have been renamed or deleted"
                    );
                    Ok(rhai::Dynamic::UNIT)
                }
                Err(e) => Err(e.to_string().into()),
            }
        },
    );

    let globals_set = Arc::clone(&globals);
    m.register(
        ScriptFn {
            name: "set",
            params: ["key", "value", "persisted"],
            doc: None,
        },
        move |key: ImmutableString,
              val: rhai::Dynamic,
              persisted: bool|
              -> Result<(), Box<EvalAltResult>> {
            let variant = dynamic_to_variant(val)
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Handle::current()
                .block_on(globals_set.set(key.as_str(), variant, persisted, caused_by))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Ok(())
        },
    );

    let globals_incr = Arc::clone(&globals);
    m.register(
        ScriptFn {
            name: "incr",
            params: ["key", "amount"],
            doc: Some("Returns the new value."),
        },
        move |key: ImmutableString, amount: i64| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let increment = Handle::current()
                .block_on(globals_incr.increment(key.as_str(), amount, caused_by))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Ok(variant_to_dynamic(increment.current))
        },
    );

    let globals_del = globals;
    m.register(
        ScriptFn {
            name: "del",
            params: ["key"],
            doc: Some("Returns true if the key existed."),
        },
        move |key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let deletion = Handle::current()
                .block_on(globals_del.delete(key.as_str(), caused_by))
                .map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
            Ok(matches!(deletion, GlobalDeletion::Deleted { .. }))
        },
    );

    m
}
