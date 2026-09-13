use serde_json::Value;

const CLIPPED: &str = "...";

/// An inline code span that stays one span whatever backticks the text holds.
pub(crate) fn code(text: &str) -> String {
    if text.is_empty() {
        return "` `".to_owned();
    }
    let ticks = "`".repeat(longest_backtick_run(text) + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

/// A fenced block whose fence is longer than any backtick run inside it.
pub(crate) fn fence(language: &str, body: &str) -> String {
    let ticks = "`".repeat(longest_backtick_run(body).max(2) + 1);
    let body = body.strip_suffix('\n').unwrap_or(body);
    format!("{ticks}{language}\n{body}\n{ticks}\n")
}

/// Table cells cannot hold pipes or line breaks.
pub(crate) fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\r', '\n'], " ")
}

/// At most `max_chars` characters, marking a cut with a trailing ellipsis.
pub(crate) fn clip(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}{CLIPPED}", &text[..cut]),
        None => text.to_owned(),
    }
}

pub(crate) fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

pub(crate) fn compact(value: &Value, max_chars: usize) -> String {
    clip(&value.to_string(), max_chars)
}

/// Whole seconds read as `2s`; anything else stays in milliseconds.
pub(crate) fn span_ms(ms: u64) -> String {
    if ms > 0 && ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms} ms")
    }
}

pub(crate) fn elapsed(ms: u64) -> String {
    format!("{}.{}s", ms / 1000, (ms % 1000) / 100)
}

pub(crate) fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

pub(crate) fn code_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| code(item))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=,@%+-".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn longest_backtick_run(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn code_span_outgrows_the_backticks_inside_it() {
        for (text, expected) in [
            ("plain", "`plain`"),
            ("a`b", "``a`b``"),
            ("`edge`", "`` `edge` ``"),
            ("", "` `"),
        ] {
            assert_eq!(code(text), expected, "{text:?}");
        }
    }

    #[test]
    fn fence_outgrows_a_fence_inside_the_body() {
        assert_eq!(
            fence("text", "before\n````\nafter\n"),
            "`````text\nbefore\n````\nafter\n`````\n"
        );
    }

    #[test]
    fn clip_keeps_text_at_the_limit_and_cuts_one_past_it_on_a_char_boundary() {
        assert_eq!(clip("ab\u{e9}", 3), "ab\u{e9}");
        assert_eq!(clip("ab\u{e9}d", 3), "ab\u{e9}...");
    }

    #[test]
    fn span_reads_whole_seconds_as_seconds_and_everything_else_in_milliseconds() {
        for (ms, expected) in [
            (0, "0 ms"),
            (999, "999 ms"),
            (1000, "1s"),
            (1001, "1001 ms"),
            (2000, "2s"),
        ] {
            assert_eq!(span_ms(ms), expected, "{ms}");
        }
    }

    #[test]
    fn shell_word_quotes_only_words_the_shell_would_split_or_expand() {
        for (word, expected) in [
            ("/tmp/run-1/scenario.json", "/tmp/run-1/scenario.json"),
            ("/tmp/my scenarios/a.json", "'/tmp/my scenarios/a.json'"),
            ("it's", "'it'\\''s'"),
            ("info,forge=debug", "info,forge=debug"),
            ("$HOME", "'$HOME'"),
        ] {
            assert_eq!(shell_word(word), expected, "{word:?}");
        }
    }
}
