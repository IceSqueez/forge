/// Strips trailing whitespace per line, collapses runs of three or more consecutive
/// blank lines to at most two, and ensures the file ends with exactly one newline.
/// No AST manipulation, no indentation changes, no operator-spacing changes.
pub fn format_script(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut blank_run = 0usize;

    for line in source.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                output.push('\n');
            }
        } else {
            blank_run = 0;
            output.push_str(trimmed);
            output.push('\n');
        }
    }

    while output.ends_with("\n\n") {
        output.pop();
    }

    if output.is_empty() {
        output.push('\n');
    }

    output
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn format_strips_trailing_whitespace() {
        let input = "let x = 1;   \nlet y = 2;  \n";
        let result = format_script(input);
        assert_eq!(result, "let x = 1;\nlet y = 2;\n");
    }

    #[test]
    fn format_collapses_three_blank_lines_to_two() {
        let input = "hello\n\n\n\nworld\n";
        let result = format_script(input);
        assert_eq!(result, "hello\n\n\nworld\n");
    }

    #[test]
    fn format_ensures_trailing_newline() {
        let input = "let x = 1;";
        let result = format_script(input);
        assert!(result.ends_with('\n'));
        assert_eq!(result, "let x = 1;\n");
    }

    #[test]
    fn format_idempotent() {
        let inputs = [
            "let x = 1;  \n\n\n\nlet y = 2;\n",
            "hello\nworld",
            "\n\n\n",
            "",
            "  indented\n  code\n",
        ];
        for input in &inputs {
            let once = format_script(input);
            let twice = format_script(&once);
            assert_eq!(once, twice, "not idempotent for: {input:?}");
        }
    }

    #[test]
    fn format_preserves_meaningful_indentation() {
        let input = "fn foo() {\n    let x = 1;\n    x\n}\n";
        let result = format_script(input);
        assert_eq!(result, "fn foo() {\n    let x = 1;\n    x\n}\n");
    }
}
