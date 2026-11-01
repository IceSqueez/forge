use forge_storage::MediaFormat;

pub(crate) fn mime_for_extension(ext: &str) -> Option<&'static str> {
    let lowered = ext.to_ascii_lowercase();
    if let Some(format) = MediaFormat::parse(&lowered) {
        return Some(format.media_type());
    }
    match lowered.as_str() {
        "html" | "htm" => Some("text/html"),
        "js" | "mjs" => Some("application/javascript"),
        "css" => Some("text/css"),
        "json" => Some("application/json"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaFormat, mime_for_extension};

    #[test]
    fn every_accepted_media_format_is_served_with_its_own_media_type() {
        for format in MediaFormat::ACCEPTED {
            assert_eq!(
                mime_for_extension(format.as_str()),
                Some(format.media_type()),
                "the overlay host does not serve .{} as {}",
                format.as_str(),
                format.media_type()
            );
        }
    }

    #[test]
    fn mime_for_extension_is_case_insensitive_for_every_served_asset_type() {
        for (ext, expected) in [
            ("html", "text/html"),
            ("htm", "text/html"),
            ("js", "application/javascript"),
            ("mjs", "application/javascript"),
            ("css", "text/css"),
            ("json", "application/json"),
            ("png", "image/png"),
            ("jpg", "image/jpeg"),
            ("svg", "image/svg+xml"),
            ("gif", "image/gif"),
            ("webp", "image/webp"),
            ("woff2", "font/woff2"),
            ("wav", "audio/wav"),
            ("mp3", "audio/mpeg"),
        ] {
            assert_eq!(mime_for_extension(ext), Some(expected), "lowercase {ext}");
            assert_eq!(
                mime_for_extension(&ext.to_ascii_uppercase()),
                Some(expected),
                "uppercase {ext}"
            );
        }
    }

    #[test]
    fn mime_for_extension_leaves_unmapped_extensions_untyped() {
        for ext in ["", "bin", "woff", "json5", "htmlx", "webp2", " json"] {
            assert!(
                mime_for_extension(ext).is_none(),
                "unexpected mapping for {ext:?}"
            );
        }
    }
}
