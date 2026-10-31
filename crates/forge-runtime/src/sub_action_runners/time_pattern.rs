use forge_registry::{ConfigProblem, ConfigReason, ToggleField, TypedField};
use forge_types::{
    ArgStack, DatePattern, MessageRef, NamesLanguage, ParsedPattern, PatternZone, SubActionConfig,
    current_names_language, shared_date_pattern,
};

use crate::core_messages;

pub(crate) const TIME_FORMAT_KIND_ID: &str = "core.time.format";
pub(crate) const TIME_PARSE_KIND_ID: &str = "core.time.parse";
pub(crate) const TIME_NOW_KIND_ID: &str = "core.time.now";
pub(crate) const FORMAT_PATTERN_KEY: &str = "format_string";
pub(crate) const PARSE_PATTERN_KEY: &str = "format";
pub(crate) const CUSTOM_NOW_PATTERN_KEY: &str = "custom_format_string";
pub(crate) const UTC_KEY: &str = "utc";
pub(crate) const DEFAULT_TIME_PATTERN: &str = "yyyy-MM-dd HH:mm:ss";

pub(crate) struct ResolvedTimePattern {
    text: String,
    parsed: ParsedPattern,
    pub(crate) zone: PatternZone,
    pub(crate) language: NamesLanguage,
}

impl ResolvedTimePattern {
    pub(crate) fn runnable_pattern(&self) -> Result<&DatePattern, MessageRef> {
        self.parsed
            .as_ref()
            .as_ref()
            .map_err(|error| core_messages::time_pattern_invalid(&self.text, error))
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

fn resolve_time_pattern(
    stored: &str,
    utc: Option<bool>,
) -> Result<ResolvedTimePattern, MessageRef> {
    let text = stored.to_owned();
    let language = current_names_language();
    let zone = if utc.unwrap_or(false) {
        PatternZone::Utc
    } else {
        PatternZone::Local
    };
    let parsed = shared_date_pattern(&text);
    Ok(ResolvedTimePattern {
        text,
        parsed,
        zone,
        language,
    })
}

pub(crate) fn checked_time_pattern(
    key: &str,
    stored: &str,
    utc: Option<bool>,
) -> Result<ResolvedTimePattern, ConfigProblem> {
    let invalid = |message| ConfigProblem::field(key, ConfigReason::Coded(message));
    let resolved = resolve_time_pattern(stored, utc).map_err(invalid)?;
    resolved.runnable_pattern().map_err(invalid)?;
    Ok(resolved)
}

pub(crate) fn stored_utc(
    field: &ToggleField,
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<Option<bool>, ConfigProblem> {
    if config.contains_key(field.key) {
        field.read(config, args).map(Some)
    } else {
        Ok(None)
    }
}
