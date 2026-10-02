const BOUNDARY_PARAMETER: &str = "boundary=";
const LINE_BREAK: &[u8] = b"\r\n";
const HEADER_END: &[u8] = b"\r\n\r\n";
const CLOSING_DASHES: &[u8] = b"--";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FormPart {
    pub(super) name: Option<String>,
    pub(super) file_name: Option<String>,
    pub(super) data: Vec<u8>,
}

pub(super) fn boundary(content_type: &str) -> Option<&str> {
    let (media_type, parameters) = content_type.split_once(';')?;
    if !media_type
        .trim()
        .eq_ignore_ascii_case("multipart/form-data")
    {
        return None;
    }
    parameters
        .split(';')
        .map(str::trim)
        .find_map(|parameter| parameter.strip_prefix(BOUNDARY_PARAMETER))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
}

pub(super) fn form_parts(boundary: &str, body: &[u8]) -> Vec<FormPart> {
    let delimiter = [CLOSING_DASHES, boundary.as_bytes()].concat();
    let mut parts = Vec::new();
    let Some(first) = find(body, &delimiter, 0) else {
        return parts;
    };
    let mut cursor = first + delimiter.len();
    loop {
        if body[cursor..].starts_with(CLOSING_DASHES) {
            return parts;
        }
        let Some(next) = find(body, &delimiter, cursor) else {
            return parts;
        };
        if let Some(part) = parse_part(&body[cursor..next]) {
            parts.push(part);
        }
        cursor = next + delimiter.len();
    }
}

fn parse_part(segment: &[u8]) -> Option<FormPart> {
    let segment = segment.strip_prefix(LINE_BREAK).unwrap_or(segment);
    let header_end = find(segment, HEADER_END, 0)?;
    let headers = String::from_utf8_lossy(&segment[..header_end]);
    let data = &segment[header_end + HEADER_END.len()..];
    let data = data.strip_suffix(LINE_BREAK).unwrap_or(data);
    let disposition = headers.split("\r\n").find(|line| {
        line.to_ascii_lowercase()
            .starts_with("content-disposition:")
    })?;
    Some(FormPart {
        name: disposition_value(disposition, "name"),
        file_name: disposition_value(disposition, "filename"),
        data: data.to_vec(),
    })
}

fn disposition_value(disposition: &str, key: &str) -> Option<String> {
    disposition.split(';').map(str::trim).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches('"').to_owned())
    })
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn part(name: &str, file_name: Option<&str>, data: &[u8]) -> FormPart {
        FormPart {
            name: Some(name.to_owned()),
            file_name: file_name.map(str::to_owned),
            data: data.to_vec(),
        }
    }

    #[test]
    fn boundary_is_read_from_a_multipart_content_type_only() {
        for (content_type, expected) in [
            ("multipart/form-data; boundary=abc123", Some("abc123")),
            (
                "Multipart/Form-Data; charset=utf-8; boundary=\"q-b\"",
                Some("q-b"),
            ),
            ("multipart/form-data; boundary=", None),
            ("multipart/form-data", None),
            ("application/json; boundary=abc", None),
        ] {
            assert_eq!(boundary(content_type), expected, "{content_type}");
        }
    }

    #[test]
    fn form_parts_split_text_and_file_fields_keeping_binary_data_intact() {
        let body = b"--XB\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\r\n{\"content\":\"hi\"}\r\n--XB\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"clip.png\"\r\nContent-Type: image/png\r\n\r\n\x00\r\n\xff\r\n--XB--\r\n";
        assert_eq!(
            form_parts("XB", body),
            [
                part("payload_json", None, b"{\"content\":\"hi\"}"),
                part("files[0]", Some("clip.png"), b"\x00\r\n\xff"),
            ]
        );
    }

    #[test]
    fn form_parts_of_a_body_without_the_boundary_is_empty() {
        assert!(form_parts("XB", b"{\"content\":\"hi\"}").is_empty());
    }

    #[test]
    fn form_parts_stop_at_a_truncated_body_keeping_the_complete_parts() {
        let body = b"--XB\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n--XB\r\nContent-Disposition: form-data; name=\"b\"\r\n\r\n2";
        assert_eq!(form_parts("XB", body), [part("a", None, b"1")]);
    }
}
