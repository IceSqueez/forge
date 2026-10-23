pub fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_replaces_every_markup_character() {
        for (input, expected) in [
            ("&", "&amp;"),
            ("<", "&lt;"),
            (">", "&gt;"),
            ("\"", "&quot;"),
            ("'", "&apos;"),
            (
                "Tom & \"Jerry\" say <hi> it's",
                "Tom &amp; &quot;Jerry&quot; say &lt;hi&gt; it&apos;s",
            ),
            ("&amp;", "&amp;amp;"),
            ("привіт 🎉", "привіт 🎉"),
            ("", ""),
        ] {
            assert_eq!(xml_escape(input), expected, "input={input:?}");
        }
    }
}
