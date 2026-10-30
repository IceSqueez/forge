pub fn canonical_status_reason(status: u16) -> Option<&'static str> {
    reqwest::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
}

pub fn status_class(status: u16) -> &'static str {
    match reqwest::StatusCode::from_u16(status) {
        Ok(code) if code.is_client_error() => "client error",
        Ok(code) if code.is_server_error() => "server error",
        Ok(code) if code.is_redirection() => "redirect",
        Ok(code) if code.is_success() => "success",
        _ => "unexpected status",
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_status_reason, status_class};

    #[test]
    fn status_class_names_each_range_and_flags_out_of_range_codes() {
        for (status, class) in [
            (200, "success"),
            (302, "redirect"),
            (404, "client error"),
            (503, "server error"),
            (100, "unexpected status"),
            (99, "unexpected status"),
            (1000, "unexpected status"),
            (0, "unexpected status"),
        ] {
            assert_eq!(status_class(status), class, "{status}");
        }
    }

    #[test]
    fn known_codes_carry_their_reason_and_others_carry_none() {
        for (status, reason) in [
            (404, Some("Not Found")),
            (429, Some("Too Many Requests")),
            (599, None),
            (99, None),
            (1000, None),
            (0, None),
        ] {
            assert_eq!(canonical_status_reason(status), reason, "{status}");
        }
    }
}
