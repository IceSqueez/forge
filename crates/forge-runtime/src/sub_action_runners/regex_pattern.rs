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
    use super::*;

    fn problem(key: &str, pattern: &str) -> ConfigProblem {
        compiled_pattern(key, pattern).unwrap_err()
    }

    #[test]
    fn a_valid_pattern_compiles() {
        assert!(
            compiled_pattern("search", r"^\w+$")
                .unwrap()
                .is_match("abc")
        );
    }

    #[test]
    fn a_syntax_error_is_reduced_to_its_one_line_reason_on_the_given_field() {
        assert_eq!(
            problem("search", "(ab"),
            ConfigProblem::field(
                "search",
                ConfigReason::InvalidFormat("unclosed group".to_owned())
            )
        );
    }

    #[test]
    fn a_syntax_error_detail_drops_the_parse_error_banner_and_the_echoed_pattern() {
        for pattern in ["(ab", "[a-", "a{2,1}", "(?P<x"] {
            let ConfigReason::InvalidFormat(detail) = problem("pattern", pattern).reason else {
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

        assert_eq!(one_line_summary(&error), error.to_string());
    }
}
