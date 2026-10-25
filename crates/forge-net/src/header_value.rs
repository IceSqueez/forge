pub fn is_valid_header_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte == b'\t' || (byte >= b' ' && byte != DELETE))
}

const DELETE: u8 = 0x7f;

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::is_valid_header_value;

    #[test]
    fn accepts_exactly_what_the_http_header_parser_accepts() {
        for code in (0_u32..=0x0800).chain([0xFEFF, 0x1F600]) {
            let Some(character) = char::from_u32(code) else {
                continue;
            };
            let value = format!("tok{character}en");
            assert_eq!(
                is_valid_header_value(&value),
                HeaderValue::from_str(&value).is_ok(),
                "U+{code:04X}"
            );
        }
    }

    #[test]
    fn control_boundaries_reject_below_space_and_delete_but_keep_tab() {
        for (value, valid) in [
            ("", true),
            ("\t", true),
            ("\u{1f}", false),
            (" ", true),
            ("~", true),
            ("\u{7f}", false),
            ("\u{80}", true),
            ("\r\n", false),
        ] {
            assert_eq!(is_valid_header_value(value), valid, "{value:?}");
        }
    }
}
