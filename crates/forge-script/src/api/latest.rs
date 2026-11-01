use super::*;

pub(super) fn build_latest_module(reader: Option<Arc<dyn LatestValueReader>>) -> ApiModule {
    let mut m = ApiModule::default();

    let merged = reader.clone();
    m.register(
        ScriptFn {
            name: "get",
            params: ["slot"],
            doc: Some("Most recent value across platforms; () when the slot is empty."),
        },
        move |slot: ImmutableString| -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let merged = available(&merged, LATEST_UNAVAILABLE_REASON)?;
            Ok(latest_dynamic(
                merged.as_ref(),
                slot.as_str(),
                LatestScope::MostRecentAcrossPlatforms,
            ))
        },
    );

    m.register(
        ScriptFn {
            name: "get",
            params: ["slot", "platform"],
            doc: Some("Value from one platform or service; () when it has none."),
        },
        move |slot: ImmutableString,
              platform: ImmutableString|
              -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let reader = available(&reader, LATEST_UNAVAILABLE_REASON)?;
            Ok(latest_dynamic(
                reader.as_ref(),
                slot.as_str(),
                LatestScope::from_platform_filter(platform.as_str()),
            ))
        },
    );

    m
}

pub(super) fn latest_dynamic(
    reader: &dyn LatestValueReader,
    slot: &str,
    scope: LatestScope<'_>,
) -> rhai::Dynamic {
    reader
        .latest(slot, scope)
        .map(|value| variant_to_dynamic(value.to_variant()))
        .unwrap_or(rhai::Dynamic::UNIT)
}
