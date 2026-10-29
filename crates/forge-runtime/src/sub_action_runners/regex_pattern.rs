use forge_registry::{ConfigProblem, ConfigReason};
use regex::Regex;

const SYNTAX_DETAIL_PREFIX: &str = "error: ";

pub(super) fn compiled_pattern(key: &str, pattern: &str) -> Result<Regex, ConfigProblem> {
    Regex::new(pattern).map_err(|error| {
        ConfigProblem::field(key, ConfigReason::InvalidFormat(one_line_summary(&error)))
    })
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

#[cfg(test)]
mod tests {
    use forge_registry::ConfigProblem;

    use super::*;

    fn parse_error(pattern: &str) -> regex::Error {
        regex::Regex::new(pattern).unwrap_err()
    }

    fn problem(key: &str, error: &regex::Error) -> ConfigProblem {
        invalid_pattern(key, error).into_config_problem()
    }

    #[test]
    fn a_syntax_error_is_reduced_to_its_one_line_reason_on_the_given_field() {
        let error = parse_error("(ab");

        assert_eq!(
            problem("search", &error),
            ConfigProblem::field(
                "search",
                ConfigReason::InvalidFormat("unclosed group".to_owned())
            )
        );
    }

    #[test]
    fn a_syntax_error_detail_drops_the_parse_error_banner_and_the_echoed_pattern() {
        for pattern in ["(ab", "[a-", "a{2,1}", "(?P<x"] {
            let error = parse_error(pattern);
            let ConfigReason::InvalidFormat(detail) = problem("pattern", &error).reason else {
                panic!("{pattern:?}: not a format problem");
            };
            assert!(
                !detail.contains('\n')
                    && !detail.contains("regex parse error")
                    && !detail.starts_with(SYNTAX_DETAIL_PREFIX)
                    && !detail.is_empty(),
                "{pattern:?}: {detail:?}"
            );
        }
    }

    #[test]
    fn a_size_limit_error_keeps_its_own_message() {
        let error = regex::Error::CompiledTooBig(10);

        assert_eq!(
            problem("pattern", &error).reason,
            ConfigReason::InvalidFormat(error.to_string())
        );
    }
}
