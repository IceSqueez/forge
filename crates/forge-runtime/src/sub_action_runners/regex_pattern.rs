use forge_registry::{ConfigReason, RegistryError};

const SYNTAX_DETAIL_PREFIX: &str = "error: ";

pub(super) fn invalid_pattern(key: &str, error: &regex::Error) -> RegistryError {
    RegistryError::invalid_field(key, ConfigReason::InvalidFormat(one_line_summary(error)))
}

fn one_line_summary(error: &regex::Error) -> String {
    let rendered = error.to_string();
    rendered
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(|line| {
            line.strip_prefix(SYNTAX_DETAIL_PREFIX)
                .unwrap_or(line)
                .to_owned()
        })
        .unwrap_or(rendered)
}
