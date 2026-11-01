use super::*;

pub(super) fn build_time_module() -> ApiModule {
    let mut m = ApiModule::default();

    m.register(
        ScriptFn {
            name: "now",
            params: [],
            doc: Some("Current UTC time as RFC 3339 text."),
        },
        || -> Result<ImmutableString, Box<EvalAltResult>> {
            Ok(forge_types::datetime_to_text(&time::OffsetDateTime::now_utc()).into())
        },
    );

    m.register(
        ScriptFn {
            name: "unix",
            params: [],
            doc: Some("Current unix time in seconds."),
        },
        || -> Result<i64, Box<EvalAltResult>> {
            Ok(time::OffsetDateTime::now_utc().unix_timestamp())
        },
    );

    m
}
