use std::ops::Range;

use iced::Color;
use iced::advanced::text::Highlighter as IcedHighlighter;

use crate::palette::{CATPPUCCIN_MOCHA, ForgePalette};

#[derive(Debug, Clone, PartialEq)]
pub struct RhaiHighlighterSettings {
    pub error_lines: Vec<usize>,
    pub palette: ForgePalette,
}

impl Default for RhaiHighlighterSettings {
    fn default() -> Self {
        Self {
            error_lines: Vec::new(),
            palette: CATPPUCCIN_MOCHA,
        }
    }
}

/// `in_block_comment` state recorded AFTER processing the line at index N.
pub struct RhaiHighlighter {
    settings: RhaiHighlighterSettings,
    current_line: usize,
    in_block_comment: bool,
    line_states: Vec<bool>,
}

fn token_color(kind: RhaiTokenKind, palette: &ForgePalette) -> Option<Color> {
    match kind {
        RhaiTokenKind::Keyword => Some(palette.code_keyword),
        RhaiTokenKind::FunctionCall => Some(palette.code_fn),
        RhaiTokenKind::StringLit | RhaiTokenKind::TemplateLit => Some(palette.code_str),
        RhaiTokenKind::Number => Some(palette.code_num),
        RhaiTokenKind::Namespace => Some(palette.warning),
        RhaiTokenKind::Comment => Some(palette.code_comment),
        RhaiTokenKind::Identifier | RhaiTokenKind::Operator | RhaiTokenKind::Punctuation => None,
    }
}

impl IcedHighlighter for RhaiHighlighter {
    type Settings = RhaiHighlighterSettings;
    type Highlight = Option<Color>;
    type Iterator<'a>
        = std::vec::IntoIter<(Range<usize>, Option<Color>)>
    where
        Self: 'a;

    fn new(settings: &RhaiHighlighterSettings) -> Self {
        Self {
            settings: settings.clone(),
            current_line: 0,
            in_block_comment: false,
            line_states: Vec::new(),
        }
    }

    fn update(&mut self, new_settings: &RhaiHighlighterSettings) {
        self.settings = new_settings.clone();
        self.current_line = 0;
        self.in_block_comment = false;
        self.line_states.clear();
    }

    fn change_line(&mut self, line: usize) {
        if line < self.current_line {
            self.in_block_comment = if line == 0 {
                false
            } else {
                self.line_states.get(line - 1).copied().unwrap_or(false)
            };
            self.line_states.truncate(line);
            self.current_line = line;
        }
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let (raw_tokens, next_state) = tokenize_line(line, self.in_block_comment);

        if self.current_line < self.line_states.len() {
            self.line_states[self.current_line] = next_state;
        } else {
            self.line_states.push(next_state);
        }
        self.in_block_comment = next_state;

        let error = self.settings.error_lines.contains(&self.current_line);
        self.current_line += 1;

        let palette = self.settings.palette;
        raw_tokens
            .into_iter()
            .map(move |(range, kind)| {
                let color = if error {
                    Some(palette.random)
                } else {
                    token_color(kind, &palette)
                };
                (range, color)
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RhaiTokenKind {
    Keyword,
    Number,
    StringLit,
    TemplateLit,
    Comment,
    Namespace,
    FunctionCall,
    Identifier,
    Operator,
    Punctuation,
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "const", "if", "else", "while", "for", "loop", "return", "break", "continue",
    "do", "until", "switch", "try", "catch", "throw", "in", "true", "false", "import", "export",
    "as", "private",
];

/// Byte ranges are relative to `line`; caller threads `in_block_comment` across lines.
pub fn tokenize_line(
    line: &str,
    in_block_comment: bool,
) -> (Vec<(Range<usize>, RhaiTokenKind)>, bool) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut tokens: Vec<(Range<usize>, RhaiTokenKind)> = Vec::new();
    let mut pos = 0usize;
    let mut block_open = in_block_comment;

    if block_open {
        match find_block_close(bytes, 0) {
            Some(close_star) => {
                let end = close_star + 2;
                tokens.push((0..end, RhaiTokenKind::Comment));
                block_open = false;
                pos = end;
            }
            None => {
                if !line.is_empty() {
                    tokens.push((0..len, RhaiTokenKind::Comment));
                }
                return (tokens, true);
            }
        }
    }

    while pos < len {
        let b = bytes[pos];

        if matches!(b, b' ' | b'\t' | b'\r') {
            pos += 1;
            continue;
        }

        if pos + 1 < len && b == b'/' && bytes[pos + 1] == b'/' {
            tokens.push((pos..len, RhaiTokenKind::Comment));
            return (tokens, false);
        }

        if pos + 1 < len && b == b'/' && bytes[pos + 1] == b'*' {
            let start = pos;
            pos += 2;
            match find_block_close(bytes, pos) {
                Some(close_star) => {
                    let end = close_star + 2;
                    tokens.push((start..end, RhaiTokenKind::Comment));
                    pos = end;
                }
                None => {
                    tokens.push((start..len, RhaiTokenKind::Comment));
                    return (tokens, true);
                }
            }
            continue;
        }

        if b == b'"' {
            let start = pos;
            pos += 1;
            while pos < len {
                let c = bytes[pos];
                if c == b'\\' {
                    pos += 1;
                    if pos < len {
                        if bytes[pos] == b'x' {
                            pos = (pos + 3).min(len);
                        } else if bytes[pos] == b'u' {
                            pos += 1;
                            if pos < len && bytes[pos] == b'{' {
                                pos += 1;
                                while pos < len && bytes[pos] != b'}' {
                                    pos += 1;
                                }
                                pos = (pos + 1).min(len);
                            }
                        } else {
                            pos += 1;
                        }
                    }
                } else if c == b'"' {
                    pos += 1;
                    break;
                } else {
                    pos += 1;
                }
            }
            tokens.push((start..pos, RhaiTokenKind::StringLit));
            continue;
        }

        if b == b'`' {
            let start = pos;
            pos += 1;
            while pos < len {
                let c = bytes[pos];
                if c == b'\\' {
                    pos += 1;
                    if pos < len {
                        pos += 1;
                    }
                } else if c == b'`' {
                    pos += 1;
                    break;
                } else {
                    pos += 1;
                }
            }
            tokens.push((start..pos, RhaiTokenKind::TemplateLit));
            continue;
        }

        if b.is_ascii_digit() {
            let start = pos;
            if b == b'0'
                && pos + 1 < len
                && matches!(bytes[pos + 1], b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            {
                pos += 2;
                while pos < len && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                    pos += 1;
                }
                tokens.push((start..pos, RhaiTokenKind::Number));
                continue;
            }
            while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                pos += 1;
            }
            if pos + 1 < len && bytes[pos] == b'.' && bytes[pos + 1].is_ascii_digit() {
                pos += 1;
                while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                    pos += 1;
                }
            }
            if pos < len && matches!(bytes[pos], b'e' | b'E') {
                let exp_start = pos;
                pos += 1;
                if pos < len && matches!(bytes[pos], b'+' | b'-') {
                    pos += 1;
                }
                if pos < len && bytes[pos].is_ascii_digit() {
                    while pos < len && bytes[pos].is_ascii_digit() {
                        pos += 1;
                    }
                } else {
                    pos = exp_start;
                }
            }
            tokens.push((start..pos, RhaiTokenKind::Number));
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' {
            let start = pos;
            pos += 1;
            while pos < len && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                pos += 1;
            }
            let word = &line[start..pos];

            if pos + 1 < len && bytes[pos] == b':' && bytes[pos + 1] == b':' {
                tokens.push((start..pos, RhaiTokenKind::Namespace));
                tokens.push((pos..pos + 2, RhaiTokenKind::Punctuation));
                pos += 2;
                continue;
            }

            let mut look = pos;
            while look < len && matches!(bytes[look], b' ' | b'\t') {
                look += 1;
            }
            if look < len && bytes[look] == b'(' {
                if KEYWORDS.contains(&word) {
                    tokens.push((start..pos, RhaiTokenKind::Keyword));
                } else {
                    tokens.push((start..pos, RhaiTokenKind::FunctionCall));
                }
                continue;
            }

            if KEYWORDS.contains(&word) {
                tokens.push((start..pos, RhaiTokenKind::Keyword));
            } else {
                tokens.push((start..pos, RhaiTokenKind::Identifier));
            }
            continue;
        }

        if pos + 2 < len && bytes[pos] == b'.' && bytes[pos + 1] == b'.' && bytes[pos + 2] == b'=' {
            tokens.push((pos..pos + 3, RhaiTokenKind::Operator));
            pos += 3;
            continue;
        }

        if pos + 1 < len {
            let (a, c) = (bytes[pos], bytes[pos + 1]);
            if matches!(
                (a, c),
                (b'=', b'=')
                    | (b'!', b'=')
                    | (b'<', b'=')
                    | (b'>', b'=')
                    | (b'&', b'&')
                    | (b'|', b'|')
                    | (b'?', b'?')
                    | (b'?', b'.')
                    | (b'.', b'.')
                    | (b'<', b'<')
                    | (b'>', b'>')
                    | (b'+', b'=')
                    | (b'-', b'=')
                    | (b'*', b'=')
                    | (b'/', b'=')
                    | (b'%', b'=')
            ) {
                tokens.push((pos..pos + 2, RhaiTokenKind::Operator));
                pos += 2;
                continue;
            }
        }

        if matches!(
            b,
            b'+' | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'<'
                | b'>'
                | b'&'
                | b'|'
                | b'^'
                | b'!'
                | b'?'
        ) {
            tokens.push((pos..pos + 1, RhaiTokenKind::Operator));
            pos += 1;
            continue;
        }

        tokens.push((pos..pos + 1, RhaiTokenKind::Punctuation));
        pos += 1;
    }

    (tokens, block_open)
}

fn find_block_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Multi-line `let` patterns (assignment continues on next line) always return `None`.
pub fn scan_type_hint(line: &str) -> Option<(String, &'static str)> {
    let (tokens, _) = tokenize_line(line, false);

    let get = |i: usize| -> Option<(&Range<usize>, &RhaiTokenKind)> {
        tokens.get(i).map(|(r, k)| (r, k))
    };

    let (r, k) = get(0)?;
    if *k != RhaiTokenKind::Keyword || &line[r.clone()] != "let" {
        return None;
    }

    let (ir, ik) = get(1)?;
    if *ik != RhaiTokenKind::Identifier {
        return None;
    }
    let ident = line[ir.clone()].to_owned();

    let (er, ek) = get(2)?;
    if *ek != RhaiTokenKind::Operator || &line[er.clone()] != "=" {
        return None;
    }

    let mut tok_pos = 3usize;
    let mut namespaces: Vec<&str> = Vec::new();

    loop {
        let (tr, tk) = get(tok_pos)?;
        tok_pos += 1;
        match tk {
            RhaiTokenKind::Namespace => {
                namespaces.push(&line[tr.clone()]);
                let (sr, sk) = get(tok_pos)?;
                tok_pos += 1;
                if *sk != RhaiTokenKind::Punctuation || &line[sr.clone()] != "::" {
                    return None;
                }
            }
            RhaiTokenKind::FunctionCall => {
                let fn_name = &line[tr.clone()];
                let descriptor = match namespaces.len() {
                    0 => return None,
                    1 => forge_script::catalog()
                        .iter()
                        .find(|d| d.namespace.is_none() && d.name == fn_name),
                    _ => {
                        let ns = namespaces[namespaces.len() - 1];
                        forge_script::catalog()
                            .iter()
                            .find(|d| d.namespace == Some(ns) && d.name == fn_name)
                    }
                };
                return descriptor.map(|d| (ident, d.return_type));
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighter_forward_walk_line_states_and_current_line() {
        use iced::advanced::text::Highlighter as IcedHighlighter;

        let mut h = RhaiHighlighter::new(&RhaiHighlighterSettings::default());
        assert_eq!(h.current_line(), 0);

        let _ = h.highlight_line("let x = 1;").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 1);
        assert_eq!(h.line_states.len(), 1);
        assert!(!h.line_states[0]);

        let _ = h.highlight_line("/* open").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 2);
        assert_eq!(h.line_states.len(), 2);
        assert!(h.line_states[1]);

        let _ = h.highlight_line("still in comment */").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 3);
        assert_eq!(h.line_states.len(), 3);
        assert!(!h.line_states[2]);
    }

    #[test]
    fn highlighter_backward_change_line_rewinds_block_comment_state() {
        use iced::advanced::text::Highlighter as IcedHighlighter;

        let mut h = RhaiHighlighter::new(&RhaiHighlighterSettings::default());
        let _ = h.highlight_line("let x = 1;").collect::<Vec<_>>();
        let _ = h.highlight_line("/* open block").collect::<Vec<_>>();
        let _ = h.highlight_line("still inside */").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 3);

        h.change_line(0);
        assert_eq!(h.current_line(), 0);
        assert!(!h.in_block_comment);
        assert!(h.line_states.is_empty());
    }

    #[test]
    fn highlighter_error_lines_override_all_token_colors_to_random() {
        use iced::advanced::text::Highlighter as IcedHighlighter;

        let settings = RhaiHighlighterSettings {
            error_lines: vec![1],
            palette: CATPPUCCIN_MOCHA,
        };
        let mut h = RhaiHighlighter::new(&settings);
        let _ = h.highlight_line("let x = 1;").collect::<Vec<_>>();
        let tokens: Vec<_> = h.highlight_line("let y = \"hello\";").collect();
        assert!(!tokens.is_empty());
        for (_, color) in &tokens {
            assert_eq!(*color, Some(CATPPUCCIN_MOCHA.random));
        }
    }

    #[test]
    fn empty_line_no_block_comment() {
        let (tokens, next) = tokenize_line("", false);
        assert!(tokens.is_empty());
        assert!(!next);
    }

    #[test]
    fn empty_line_in_block_comment() {
        let (tokens, next) = tokenize_line("", true);
        assert!(tokens.is_empty());
        assert!(next);
    }

    #[test]
    fn globals_call_exact_token_sequence() {
        let line = r#"let q = sl::globals::get("counter");"#;
        let (tokens, next) = tokenize_line(line, false);
        assert!(!next);
        let kinds: Vec<RhaiTokenKind> = tokens.iter().map(|(_, k)| *k).collect();
        assert_eq!(
            kinds,
            [
                RhaiTokenKind::Keyword,
                RhaiTokenKind::Identifier,
                RhaiTokenKind::Operator,
                RhaiTokenKind::Namespace,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::Namespace,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::FunctionCall,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::StringLit,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::Punctuation,
            ]
        );
        assert_eq!(tokens[0].0, 0..3);
        assert_eq!(tokens[1].0, 4..5);
        assert_eq!(tokens[2].0, 6..7);
        assert_eq!(tokens[3].0, 8..10);
        assert_eq!(tokens[4].0, 10..12);
        assert_eq!(tokens[5].0, 12..19);
        assert_eq!(tokens[6].0, 19..21);
        assert_eq!(tokens[7].0, 21..24);
        assert_eq!(tokens[8].0, 24..25);
        assert_eq!(tokens[9].0, 25..34);
        assert_eq!(tokens[10].0, 34..35);
        assert_eq!(tokens[11].0, 35..36);
    }

    #[test]
    fn block_comment_across_three_lines() {
        let (t1, s1) = tokenize_line("/* a", false);
        assert_eq!(t1, [(0..4, RhaiTokenKind::Comment)]);
        assert!(s1);

        let (t2, s2) = tokenize_line("b", s1);
        assert_eq!(t2, [(0..1, RhaiTokenKind::Comment)]);
        assert!(s2);

        let (t3, s3) = tokenize_line("c */", s2);
        assert_eq!(t3, [(0..4, RhaiTokenKind::Comment)]);
        assert!(!s3);
    }

    #[test]
    fn template_literal_entire_span() {
        let line = "`hello ${name}`";
        let (tokens, next) = tokenize_line(line, false);
        assert!(!next);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].1, RhaiTokenKind::TemplateLit);
        assert_eq!(tokens[0].0, 0..line.len());
    }

    #[test]
    fn number_tokenizer_recognises_each_literal_form() {
        for input in ["42", "3.14", "0xFF", "1_000", "1.5e10", "0b1010"] {
            let (tokens, _) = tokenize_line(input, false);
            assert_eq!(
                tokens,
                [(0..input.len(), RhaiTokenKind::Number)],
                "input={input}"
            );
        }
    }

    #[test]
    fn highlighter_500_line_script_within_budget() {
        use std::time::Instant;

        use iced::advanced::text::Highlighter as IcedHighlighter;

        let script_lines = build_500_line_script();
        assert_eq!(
            script_lines.len(),
            500,
            "script generator must emit exactly 500 lines"
        );

        // Measure tokenize_line over all 500 lines.
        let t0 = Instant::now();
        let mut block_state = false;
        let all_tokens: Vec<_> = script_lines
            .iter()
            .map(|line| {
                let (tokens, next) = tokenize_line(line, block_state);
                block_state = next;
                tokens
            })
            .collect();
        let tokenize_ms = t0.elapsed().as_millis();

        // Measure highlight_line over all 500 lines via the iced Highlighter path.
        let settings = RhaiHighlighterSettings::default();
        let mut h = RhaiHighlighter::new(&settings);
        let t1 = Instant::now();
        for line in &script_lines {
            let _ = h.highlight_line(line).collect::<Vec<_>>();
        }
        let highlight_ms = t1.elapsed().as_millis();

        // Prevent the optimizer from eliding the tokenize pass.
        let _ = all_tokens;

        assert!(
            tokenize_ms < 50,
            "tokenize 500 lines took {tokenize_ms}ms, budget is 50ms"
        );
        assert!(
            highlight_ms < 100,
            "highlight_line 500 lines took {highlight_ms}ms, budget is 100ms"
        );
    }

    fn build_500_line_script() -> Vec<String> {
        let mut lines: Vec<String> = Vec::with_capacity(500);

        lines.push("// @input user: string".into());
        lines.push("// @input count: int".into());
        lines.push("// @input enabled: bool".into());
        lines.push("// @return string".into());
        lines.push("".into());

        for i in 0..20usize {
            lines.push(format!("fn handler_{i}(user, count) {{"));
            lines.push(format!(
                "    let greeting = \"Hello, \" + user + \" #{i}\";"
            ));
            lines.push(format!("    let limit = {};", (i + 1) * 5));
            lines.push(format!(
                "    let flag = {};",
                if i.is_multiple_of(2) { "true" } else { "false" }
            ));
            lines.push("    if count > limit {".into());
            lines.push("        let val = forge::globals::get(\"counter\");".into());
            lines.push(format!("        forge::globals::incr(\"counter_{i}\", 1);"));
            lines.push("        forge::tts::speak(val);".into());
            lines.push("    } else {".into());
            lines.push(format!("        forge::globals::set(\"counter_{i}\", 0);"));
            lines.push("        forge::chat::send(greeting);".into());
            lines.push("    }".into());
            lines.push(format!("    for j in 0..{} {{", i + 1));
            lines.push(format!("        let x = j * {};", i + 2));
            lines.push("        if x == 0 { continue; }".into());
            lines.push(format!("        let hex_check = 0x{:02X};", i * 4));
            lines.push(format!("        let bin_check = 0b{:08b};", i));
            lines.push("    }".into());
            lines.push("    /* begin multi-line block".into());
            lines.push(format!("       iteration {i} processed */"));
            lines.push("    let ts = forge::time::now();".into());
            lines.push("    let unix = forge::time::unix();".into());
            lines.push(format!("    forge::log(\"handler_{i} at: \" + ts);"));
            lines.push(format!("    `result of handler_{i}: ${{greeting}}`"));
            lines.push(format!(
                "    let result_{i} = forge::globals::get(\"state_{i}\");"
            ));
            lines.push(format!("    result_{i}"));
            lines.push("}".into());
            lines.push("".into());
        }

        // Fill the remainder with varied single-statement lines so we reach exactly 500.
        while lines.len() < 500 {
            let idx = lines.len();
            let line = match idx % 8 {
                0 => format!("let var_{idx} = {idx};"),
                1 => format!("let str_{idx} = \"value {idx}\";"),
                2 => format!("// single-line comment at position {idx}"),
                3 => format!("forge::globals::set(\"key_{idx}\", {idx});"),
                4 => format!("let f_{idx} = {};", (idx as f64) * 0.5),
                5 => format!(
                    "let b_{idx} = {};",
                    if idx.is_multiple_of(2) {
                        "true"
                    } else {
                        "false"
                    }
                ),
                6 => format!("forge::log(\"step {idx}\");"),
                _ => format!("let x_{idx} = 0x{idx:02X};"),
            };
            lines.push(line);
        }

        lines.truncate(500);
        lines
    }

    #[test]
    fn scan_let_globals_get_returns_variant() {
        let result = scan_type_hint(r#"let x = forge::globals::get("counter");"#);
        assert_eq!(result, Some(("x".to_owned(), "Variant")));
    }

    #[test]
    fn scan_let_namespaced_chat_send_returns_unit() {
        let result = scan_type_hint(r#"let y = forge::chat::send("hello");"#);
        assert_eq!(result, Some(("y".to_owned(), "()")));
    }

    #[test]
    fn scan_unknown_fn_returns_none() {
        let result = scan_type_hint("let z = foo::bar(1);");
        assert_eq!(result, None);
    }

    #[test]
    fn scan_no_let_keyword_returns_none() {
        let result = scan_type_hint(r#"x = forge::globals::get("k");"#);
        assert_eq!(result, None);
    }

    #[test]
    fn scan_let_without_call_returns_none() {
        let result = scan_type_hint("let x = 42;");
        assert_eq!(result, None);
    }
}
